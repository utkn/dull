use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;
use path_absolutize::Absolutize;
use rand::Rng;
use walkdir::WalkDir;

use crate::{
    config_parser::{Config, GlobalConfig, ModuleConfig},
    module_parser::ModuleParser,
    transaction::{FsMod, FsTransaction},
    utils,
};

#[derive(Clone, Debug)]
pub struct ResolvedLink {
    pub abs_source: PathBuf,
    pub abs_target: PathBuf,
}

impl ResolvedLink {
    fn expand_path(path: PathBuf) -> anyhow::Result<PathBuf> {
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

    pub fn new(source: PathBuf, target: PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            abs_source: ResolvedLink::expand_path(source)?,
            abs_target: ResolvedLink::expand_path(target)?,
        })
    }
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

    pub fn build(self, build_name: Option<String>, verbose: bool) -> anyhow::Result<VirtualSystem> {
        let mut parsed_modules = vec![];
        for module_config in self.modules_config.iter() {
            let parsed_module =
                ModuleParser::from_config(module_config, &self.global_config).parse()?;
            parsed_modules.push(parsed_module);
        }
        let generated_links = parsed_modules
            .into_iter()
            .zip(self.modules_config.iter())
            .flat_map(|(m, conf)| m.emplace(conf.target.clone()))
            .collect_vec();
        let effective_build_name = if let Some(build_name) = build_name {
            build_name
        } else {
            format!("{}", rand::thread_rng().gen::<u32>())
        };
        // Generate the virtual system.
        let build_dir = PathBuf::from("builds").join(&effective_build_name);
        Self::build_at_root(build_dir.clone(), generated_links)
            .and_then(|tx| tx.run_atomic(verbose))?
            .display_report();
        // Write the build information
        std::fs::write(
            build_dir.join(&self.global_config.build_file),
            format!("{}", effective_build_name),
        )
        .context("could not generate the build information")?;
        Ok(VirtualSystem {
            name: effective_build_name,
            path: build_dir,
        })
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
            curr_virt_target = ResolvedLink::expand_path(curr_virt_target)?;
            // Create the virtual directory if it does not exist.
            let curr_virt_target_parent = curr_virt_target.parent().context(format!(
                "could not get the parent of {:?}",
                curr_virt_target
            ))?;
            tx.push(FsMod::CreateDirs(curr_virt_target_parent.to_path_buf()));
            tx.push(FsMod::Link {
                original: link.abs_source,
                target: curr_virt_target,
            });
        }
        Ok(tx)
    }
}

#[derive(Clone, Debug)]
pub struct VirtualSystem {
    pub name: String,
    pub path: PathBuf,
}

impl VirtualSystem {
    pub fn read(path: PathBuf, build_file_name: &str) -> anyhow::Result<Self> {
        let build_file_path = path.join(build_file_name);
        let build_name = std::fs::read_to_string(&build_file_path).context(format!(
            "could not read the build file {:?}",
            build_file_path
        ))?;
        Ok(Self {
            path,
            name: build_name,
        })
    }

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

    pub fn undeploy(self) -> anyhow::Result<FsTransaction> {
        let mut tx = FsTransaction::empty();
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = ResolvedLink::expand_path(target)?;
            if abs_target.is_symlink() || abs_target.is_file() {
                tx.push(FsMod::RemoveFile(abs_target));
            } else {
                tx.append(utils::remove_dir_tx(&abs_target)?);
            }
        }
        Ok(tx)
    }

    pub fn deploy(
        self,
        hard: bool,
        clear_target: bool,
        ignore_filenames: &[&str],
    ) -> anyhow::Result<FsTransaction> {
        let mut tx = FsTransaction::empty();
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = ResolvedLink::expand_path(target)?;
            let abs_source = ResolvedLink::expand_path(leaf)?;
            // Clear the target.
            if clear_target && abs_target.symlink_metadata().is_ok() {
                if abs_target.is_symlink() || abs_target.is_file() {
                    tx.push(FsMod::RemoveFile(abs_target.clone()));
                } else {
                    tx.append(utils::remove_dir_tx(&abs_target)?);
                }
            }
            // Create the directories leading to the target.
            let abs_target_parent = abs_target
                .parent()
                .context(format!("could not get the parent of {:?}", abs_target))?;
            tx.push(FsMod::CreateDirs(abs_target_parent.to_path_buf()));
            // Get the original source, pointing to the regular file in the module directory.
            let abs_source_canon = abs_source.canonicalize().context(format!(
                "could not canonicalize the source {:?}",
                abs_source
            ))?;
            // Perform the actual linking.
            if hard {
                // Traverse through the regular files indicated by the leaf.
                let inner = WalkDir::new(&abs_source_canon)
                    .follow_root_links(true)
                    .follow_links(false)
                    .into_iter()
                    .flatten()
                    .map(|p| p.path().to_path_buf())
                    // Only consider regular files or symlinks.
                    .filter(|p| p.is_file() || p.is_symlink())
                    // Make sure that the files are not in the ignored filenames list.
                    .filter(|p| {
                        p.file_name()
                            .map(|file_name| file_name.to_string_lossy())
                            .map(|file_name| !ignore_filenames.contains(&file_name.as_ref()))
                            .unwrap_or(false)
                    });
                for inner_source in inner {
                    let inner_target =
                        abs_target.join(inner_source.strip_prefix(&abs_source_canon).unwrap());
                    // Create the directories leading to the inner target.
                    let inner_target_parent = inner_target
                        .parent()
                        .context(format!("could not get the parent of {:?}", inner_target))?;
                    let create_dirs_mod = FsMod::CreateDirs(inner_target_parent.to_path_buf());
                    if !tx.mods.contains(&create_dirs_mod)
                        && inner_target_parent.symlink_metadata().is_err()
                    {
                        tx.push(create_dirs_mod);
                    }
                    tx.push(FsMod::CopyFile {
                        source: inner_source,
                        target: inner_target,
                    });
                }
            } else {
                // Create the symlink.
                let abs_target_parent = abs_target
                    .parent()
                    .context(format!("could not get the parent of {:?}", abs_target))?;
                tx.push(FsMod::CreateDirs(abs_target_parent.to_path_buf()));
                tx.push(FsMod::Link {
                    original: abs_source_canon,
                    target: abs_target,
                });
            }
        }
        Ok(tx)
    }
}
