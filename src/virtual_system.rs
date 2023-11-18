use std::{marker::PhantomData, path::PathBuf};

use anyhow::Context;
use itertools::Itertools;

use rand::Rng;
use walkdir::WalkDir;

use crate::{
    config_parser::{ModuleConfig, ResolvedConfig},
    globals,
    module_parser::ModuleParser,
    transaction::{ActualFilesystem, TxBuilder, TxProcessor},
    utils,
};

pub struct VirtualSystemBuilder<'a> {
    modules_config: &'a [ModuleConfig],
}

impl<'a> VirtualSystemBuilder<'a> {
    pub fn from_config(config: &'a ResolvedConfig) -> Self {
        Self {
            modules_config: &config.modules,
        }
    }

    pub fn build(self, build_name: Option<String>, verbose: bool) -> anyhow::Result<PathBuf> {
        let mut parsed_modules = vec![];
        for module_config in self.modules_config.iter() {
            let parsed_module = ModuleParser::from_config(module_config).parse()?;
            parsed_modules.push(parsed_module);
        }
        let generated_links = parsed_modules
            .into_iter()
            .zip(self.modules_config.iter())
            .flat_map(|(m, conf)| m.emplace(&conf.target))
            .collect_vec();
        let effective_build_name = if let Some(build_name) = build_name {
            build_name
        } else {
            format!("{}", rand::thread_rng().gen::<u32>())
        };
        // Generate the virtual system.
        let build_dir = PathBuf::from("builds").join(&effective_build_name);
        // Generate the transaction and run it unatomically.
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        txb.create_links(build_dir.clone(), generated_links, &fs)?;
        txb.build("Build")?
            .run_haphazard(verbose)
            .context("build failed")?;
        // Write the build information
        let build_info_path = build_dir.join(globals::BUILD_FILE_NAME);
        std::fs::write(&build_info_path, format!("{}", effective_build_name)).context(format!(
            "could not generate the build information at {:?}",
            build_info_path
        ))?;
        Ok(build_dir)
    }
}

pub struct Deployable;
pub struct Undeployable;

#[derive(Clone, Debug)]
pub struct VirtualSystem<T> {
    pub path: PathBuf,
    pub pd: PhantomData<T>,
}

impl VirtualSystem<Undeployable> {
    /// Reads the virtual system at the given path.
    pub fn read(path: PathBuf) -> anyhow::Result<Self> {
        let build_file_path = path.join(globals::BUILD_FILE_NAME);
        std::fs::read_to_string(&build_file_path).context(format!(
            "could not read the build file {:?}",
            build_file_path
        ))?;
        Ok(Self {
            path,
            pd: Default::default(),
        })
    }
}

impl<T> VirtualSystem<T> {
    /// From a leaf node, extracts and returns the absolute source and target paths.
    fn parse_leaf(&self, leaf: &PathBuf) -> anyhow::Result<(PathBuf, PathBuf)> {
        // The target is already encoded in the leaf source.
        let target = PathBuf::from("/").join(
            leaf.strip_prefix(&self.path)
                .context("leaf path is malformed")?,
        );
        let abs_target = utils::expand_path(&target)?;
        let abs_source = utils::expand_path(leaf)?;
        // Get the original source, pointing to the regular file in the module directory.
        let abs_source_canon = abs_source.canonicalize().context(format!(
            "could not canonicalize the source {:?}",
            abs_source
        ))?;
        Ok((abs_source_canon, abs_target))
    }

    /// Returns the leaves of the virtual system.
    fn get_leaves(&self) -> Vec<PathBuf> {
        WalkDir::new(&self.path)
            .follow_links(false)
            .follow_root_links(false)
            .into_iter()
            .flatten()
            // Symlinks are the only leafs
            .filter(|p| p.path_is_symlink())
            .map(|p| p.path().to_path_buf())
            .collect_vec()
    }

    pub fn undeploy(self, tx_proc: &mut TxProcessor) -> anyhow::Result<()> {
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = utils::expand_path(&target)?;
            txb.remove_any(&abs_target, &fs);
        }
        txb.build("Undeploy")
            .and_then(|tx| tx_proc.run_required(tx))
    }
}

impl VirtualSystem<Undeployable> {
    /// Clears the target files/folders in the actual filesystem.
    pub fn clear_targets(self, tx_proc: &mut TxProcessor) -> anyhow::Result<Self> {
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (_, abs_target) = self.parse_leaf(&leaf)?;
            txb.remove_any(&abs_target, &fs);
        }
        txb.build("ClearTargets")
            .and_then(|tx| tx_proc.run_required(tx))?;
        Ok(self)
    }

    /// Prepares the virtual system for deployment to the actual system.
    /// Returns a deployable system.
    pub fn prepare_deployment(
        self,
        tx_proc: &mut TxProcessor,
    ) -> anyhow::Result<VirtualSystem<Deployable>> {
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (_, abs_target) = self.parse_leaf(&leaf)?;
            // Create the directories leading to the target.
            let abs_target_parent = abs_target
                .parent()
                .context(format!("could not get the parent of {:?}", abs_target))?;
            txb.ensure_dirs(abs_target_parent, &fs)?;
        }
        txb.build("Prepare")
            .and_then(|tx| tx_proc.run_required(tx))?;
        Ok(VirtualSystem {
            path: self.path,
            pd: Default::default(),
        })
    }
}

impl VirtualSystem<Deployable> {
    pub fn soft_deploy(self, tx_proc: &mut TxProcessor) -> anyhow::Result<()> {
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (source, target) = self
                .parse_leaf(&leaf)
                .context(format!("could not parse the leaf {:?}", leaf))?;
            txb.link(source, target);
        }
        txb.build("SoftDeploy")
            .and_then(|tx| tx_proc.run_required(tx))
    }

    pub fn hard_deploy(
        self,
        ignore_filenames: &[&str],
        tx_proc: &mut TxProcessor,
    ) -> anyhow::Result<()> {
        let mut txb = TxBuilder::empty();
        let fs = ActualFilesystem;
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (source, target) = self
                .parse_leaf(&leaf)
                .context(format!("could not parse the leaf {:?}", leaf))?;
            // Traverse through the regular files indicated by the leaf.
            let inner = WalkDir::new(&source)
                .follow_root_links(true)
                .follow_links(false)
                .into_iter()
                .flatten()
                .map(|p| p.path().to_path_buf())
                // Only consider regular files or symlinks.
                .filter(|p| p.is_symlink() || p.is_file())
                // Make sure that the files are not in the ignored filenames list.
                .filter(|p| {
                    p.file_name()
                        .map(|file_name| file_name.to_string_lossy())
                        .map(|file_name| !ignore_filenames.contains(&file_name.as_ref()))
                        .unwrap_or(false)
                })
                // Always start from the shortest path (stable sort is important)
                .sorted_by_key(|p| p.components().count());
            for inner_source in inner {
                let inner_target = if inner_source == source {
                    target.clone()
                } else {
                    target.join(inner_source.strip_prefix(&source).unwrap())
                };
                // Create the directories leading to the inner target.
                let inner_target_parent = inner_target
                    .parent()
                    .context(format!("could not get the parent of {:?}", inner_target))?;
                txb.ensure_dirs(inner_target_parent, &fs);
                // Copy the file in.
                txb.copy_file(inner_source, inner_target);
            }
        }
        txb.build("HardDeploy")
            .and_then(|tx| tx_proc.run_required(tx))
    }
}
