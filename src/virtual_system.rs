use std::{marker::PhantomData, path::PathBuf};

use anyhow::Context;
use itertools::Itertools;
use path_absolutize::Absolutize;
use rand::Rng;
use walkdir::WalkDir;

use crate::{
    config_parser::{Config, GlobalConfig, ModuleConfig},
    module_parser::ModuleParser,
    transaction::{tx_gen, FsTransaction, FsTransactionResult},
};

#[derive(Clone, Debug)]
pub struct ResolvedLink {
    pub abs_source: PathBuf,
    pub abs_target: PathBuf,
}

impl ResolvedLink {
    fn expand_path(path: &PathBuf) -> anyhow::Result<PathBuf> {
        let expanded_path = expanduser::expanduser(path.as_os_str().to_string_lossy())
            .context(format!("could not expand the path {:?}", path))?;
        let absolute_path = expanded_path
            .absolutize()
            .context(format!(
                "could not absolutize the target path {:?}",
                expanded_path
            ))
            .map(|p| p.into());
        absolute_path
    }

    pub fn new(source: &PathBuf, target: &PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            abs_source: ResolvedLink::expand_path(source)?,
            abs_target: ResolvedLink::expand_path(target)?,
        })
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DeploymentState {}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VirtualSystemState {
    name: String,
    leafs: Vec<PathBuf>,
}

pub struct VirtualSystemBuilder<'a> {
    modules_config: &'a [ModuleConfig],
    global_config: &'a GlobalConfig,
}

impl<'a> VirtualSystemBuilder<'a> {
    pub fn from_config(config: &'a Config) -> Self {
        Self {
            modules_config: &config.module,
            global_config: &config.global,
        }
    }

    pub fn build(self, build_name: Option<String>, verbose: bool) -> anyhow::Result<PathBuf> {
        let mut parsed_modules = vec![];
        for module_config in self.modules_config.iter() {
            let parsed_module =
                ModuleParser::from_config(module_config, &self.global_config).parse()?;
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
        Self::build_at_root(build_dir.clone(), generated_links)?
            .with_name("build")
            .run_haphazard(verbose)
            .as_tx_result()?;
        // Write the build information
        let build_info_path = build_dir.join(&self.global_config.build_file);
        std::fs::write(&build_info_path, format!("{}", effective_build_name)).context(format!(
            "could not generate the build information at {:?}",
            build_info_path
        ))?;
        Ok(build_dir)
    }

    fn build_at_root<P: Into<PathBuf>>(
        root: P,
        links: Vec<ResolvedLink>,
    ) -> anyhow::Result<FsTransaction> {
        let mut tx = FsTransaction::empty();
        let root: PathBuf = root.into();
        for link in links.into_iter() {
            let mut curr_virt_target = root.clone();
            let relativized_target = if link.abs_target.is_absolute() {
                link.abs_target.strip_prefix("/")?
            } else {
                link.abs_target.as_path()
            };
            curr_virt_target.push(relativized_target);
            curr_virt_target = ResolvedLink::expand_path(&curr_virt_target)?;
            // Create the virtual directory if it does not exist.
            let curr_virt_target_parent = curr_virt_target.parent().context(format!(
                "could not get the parent of {:?}",
                curr_virt_target
            ))?;
            tx.try_create_dirs(curr_virt_target_parent);
            tx.link(link.abs_source, curr_virt_target);
        }
        Ok(tx)
    }
}

pub struct Deployable;
pub struct Undeployable;

#[derive(Clone, Debug)]
pub struct VirtualSystem<T> {
    pub name: String,
    pub path: PathBuf,
    pub pd: PhantomData<T>,
}

impl<T> VirtualSystem<T> {
    /// From a leaf node, extracts and returns the absolute source and target paths.
    fn parse_leaf(&self, leaf: &PathBuf) -> anyhow::Result<(PathBuf, PathBuf)> {
        // The target is already encoded in the leaf source.
        let target = PathBuf::from("/").join(
            leaf.strip_prefix(&self.path)
                .context("leaf path is malformed")?,
        );
        let abs_target = ResolvedLink::expand_path(&target)?;
        let abs_source = ResolvedLink::expand_path(leaf)?;
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
            .map(|p| p.path().to_path_buf())
            .filter(|p| p.is_symlink())
            .collect_vec()
    }

    pub fn undeploy(self, verbose: bool) -> anyhow::Result<()> {
        let mut tx = FsTransaction::empty();
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = ResolvedLink::expand_path(&target)?;
            tx.append(tx_gen::remove_any(&abs_target)?);
        }
        let res = tx.with_name("undeploy").run_atomic(verbose)?;
        res.display_report();
        res.as_tx_result()?;
        Ok(())
    }
}

impl VirtualSystem<Undeployable> {
    /// Reads the virtual system at the given path.
    pub fn read(path: PathBuf, build_file_name: &str) -> anyhow::Result<Self> {
        let build_file_path = path.join(build_file_name);
        let build_name = std::fs::read_to_string(&build_file_path).context(format!(
            "could not read the build file {:?}",
            build_file_path
        ))?;
        Ok(Self {
            path,
            name: build_name,
            pd: Default::default(),
        })
    }

    /// Prepares the virtual system for deployment to the actual system.
    pub fn prepare_deployment(
        self,
        clear_target: bool,
        verbose: bool,
    ) -> anyhow::Result<VirtualSystem<Deployable>> {
        let mut tx = FsTransaction::empty().with_name("prepare");
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (_, abs_target) = self.parse_leaf(&leaf)?;
            // Clear the target.
            if clear_target && abs_target.symlink_metadata().is_ok() {
                tx.append(tx_gen::remove_any(&abs_target)?);
            }
            // Create the directories leading to the target.
            let abs_target_parent = abs_target
                .parent()
                .context(format!("could not get the parent of {:?}", abs_target))?;
            tx.try_create_dirs(abs_target_parent);
        }
        let deployment_result = tx.run_atomic(verbose)?;
        deployment_result.display_report();
        deployment_result.as_tx_result()?;
        Ok(VirtualSystem {
            name: self.name,
            path: self.path,
            pd: Default::default(),
        })
    }
}

impl VirtualSystem<Deployable> {
    pub fn soft_deploy(self, verbose: bool) -> anyhow::Result<()> {
        let mut tx = FsTransaction::empty();
        let leaves = self.get_leaves();
        for leaf in leaves {
            let (source, target) = self
                .parse_leaf(&leaf)
                .context(format!("could not parse the leaf {:?}", leaf))?;
            tx.link(source, target);
        }
        let res = tx.with_name("soft deploy").run_atomic(verbose)?;
        res.display_report();
        res.as_tx_result()?;
        Ok(())
    }

    pub fn hard_deploy(self, ignore_filenames: &[&str], verbose: bool) -> anyhow::Result<()> {
        let mut tx = FsTransaction::empty();
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
                });
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
                if !tx.has_dir(inner_target_parent) {
                    tx.try_create_dirs(inner_target_parent);
                }
                tx.copy_file(inner_source, inner_target);
            }
        }
        let res = tx.with_name("hard deploy").run_atomic(verbose)?;
        res.display_report();
        res.as_tx_result()?;
        Ok(())
    }
}
