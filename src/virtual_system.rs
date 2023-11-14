use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;
use path_absolutize::Absolutize;
use rand::Rng;
use walkdir::WalkDir;

use crate::{
    config_parser::{Config, GlobalConfig, ModuleConfig},
    module_parser::ModuleParser,
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
        for link in generated_links.iter() {
            if verbose {
                println!("{:?} => {:?}", link.abs_target, link.abs_source);
            }
        }
        let effective_build_name = if let Some(build_name) = build_name {
            build_name
        } else {
            format!("{}", rand::thread_rng().gen::<u32>())
        };
        let build_dir = PathBuf::from("builds").join(&effective_build_name);
        Self::build_at_root(build_dir.clone(), generated_links, verbose)
            .context("virtual system generation failed, possibly conflicting modules")?;
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
        verbose: bool,
    ) -> anyhow::Result<()> {
        let root: PathBuf = root.into();
        if verbose {
            println!("* creating a virtual system under {:?}", root);
        }
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
            curr_virt_target
                .parent()
                .context(format!(
                    "could not get the parent of {:?}",
                    curr_virt_target
                ))
                .and_then(|parent| {
                    std::fs::create_dir_all(parent)
                        .context(format!("could not create virtual directory {:?}", parent))
                })?;
            // Perform the linkage
            std::os::unix::fs::symlink(&link.abs_source, &curr_virt_target).context(format!(
                "could not create the symlink {:?} => {:?}",
                curr_virt_target, link.abs_source,
            ))?;
        }
        Ok(())
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
            path: path,
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

    pub fn undeploy(self, verbose: bool) -> anyhow::Result<()> {
        println!("* undeploying the build under {:?}", self.path);
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = ResolvedLink::expand_path(target)?;
            if verbose {
                println!("remove {:?}", abs_target);
            }
            std::fs::remove_dir_all(&abs_target)
                .context(format!("could not remove {:?}", abs_target))?;
        }
        println!("* done =)");
        Ok(())
    }

    pub fn deploy(
        self,
        hard: bool,
        clear_target: bool,
        verbose: bool,
        ignore_filenames: &[&str],
    ) -> anyhow::Result<()> {
        println!("* deploying the virtual system under {:?}", self.path);
        let leaves = self.get_leaves();
        for leaf in leaves {
            // The target is already encoded in the leaf source.
            let target = PathBuf::from("/").join(
                leaf.strip_prefix(&self.path)
                    .context("leaf path is malformed")?,
            );
            let abs_target = ResolvedLink::expand_path(target)?;
            let abs_source = ResolvedLink::expand_path(leaf)?;
            if clear_target {
                _ = std::fs::remove_dir_all(&abs_target);
                _ = std::fs::remove_file(&abs_target);
            }
            // Create the directories leading to the target.
            abs_target
                .parent()
                .context(format!("could not get the parent of {:?}", abs_target))
                .and_then(|target_parent| {
                    std::fs::create_dir_all(target_parent)
                        .context(format!("could not create the dirs {:?}", abs_target))
                })?;
            if hard {
                // Traverse through the regular files indicated by the leaf.
                let inner = WalkDir::new(&abs_source)
                    .follow_links(true)
                    .follow_root_links(true)
                    .into_iter()
                    .flatten()
                    .map(|p| p.path().to_path_buf())
                    // Only consider regular files.
                    .filter(|p| p.is_file())
                    // Make sure that the files are not in the ignored filenames list.
                    .filter(|p| {
                        p.file_name()
                            .map(|file_name| file_name.to_string_lossy())
                            .map(|file_name| !ignore_filenames.contains(&file_name.as_ref()))
                            .unwrap_or(false)
                    });
                for inner_abs_source in inner {
                    let inner_abs_target =
                        abs_target.join(inner_abs_source.strip_prefix(&abs_source).unwrap());
                    // Create the directories leading to the inner target.
                    inner_abs_target
                        .parent()
                        .context(format!("could not get the parent of {:?}", abs_target))
                        .and_then(|target_parent| {
                            std::fs::create_dir_all(target_parent)
                                .context(format!("could not create the dirs {:?}", abs_target))
                        })?;
                    if verbose {
                        println!("copy {:?} to {:?}", inner_abs_source, inner_abs_target);
                    }
                    if let Ok(_) = std::fs::metadata(&inner_abs_target) {
                        anyhow::bail!(
                            "file at {:?} already exists, use --force to clear the target paths",
                            inner_abs_target
                        );
                    }
                    std::fs::copy(&inner_abs_source, &inner_abs_target).context(format!(
                        "could not copy {:?} to {:?}",
                        inner_abs_source, inner_abs_target
                    ))?;
                }
            } else {
                // Get the original source in the module directory.
                let abs_source_canon = abs_source.canonicalize().context(format!(
                    "could not canonicalize the source {:?}",
                    abs_source
                ))?;
                if verbose {
                    println!("link {:?} to {:?}", abs_target, abs_source_canon);
                }
                // Create the symlink.
                std::os::unix::fs::symlink(&abs_source_canon, &abs_target).context(format!(
                "could not create the symlink {:?} to {:?}, use --force to clear the target paths",
                abs_target, abs_source_canon,
            ))?;
            }
        }
        println!("* done =)");
        Ok(())
    }
}
