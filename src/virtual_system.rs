use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;
use path_absolutize::Absolutize;
use walkdir::WalkDir;

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

pub struct VirtualSystem {
    pub path: PathBuf,
}

impl VirtualSystem {
    pub fn build<P: Into<PathBuf>>(root: P, links: Vec<ResolvedLink>) -> anyhow::Result<Self> {
        let root: PathBuf = root.into();
        println!("* creating a virtual system under {:?}", root);
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
        Ok(Self { path: root })
    }

    pub fn deploy(self, clear_target: bool) -> anyhow::Result<()> {
        println!("* deploying the virtual system under {:?}", self.path);
        let leafs = WalkDir::new(self.path.clone())
            .follow_links(false)
            .into_iter()
            .flatten()
            .map(|p| p.path().to_path_buf())
            .filter(|p| p.is_symlink())
            .collect_vec();
        for source in leafs {
            // The target is already encoded in the
            let target = PathBuf::from("/").join(source.strip_prefix(&self.path).context("")?);
            let abs_target = ResolvedLink::expand_path(target)?;
            let abs_source = ResolvedLink::expand_path(source)?;
            if clear_target {
                _ = std::fs::remove_dir_all(&abs_target);
                _ = std::fs::remove_file(&abs_target);
            }
            abs_target
                .parent()
                .context(format!("could not get the parent of {:?}", abs_target))
                .and_then(|target_parent| {
                    std::fs::create_dir_all(target_parent)
                        .context(format!("could not create the dirs {:?}", abs_target))
                })?;
            std::os::unix::fs::symlink(&abs_source, &abs_target).context(format!(
                "could not create the symlink {:?} => {:?}",
                abs_target, abs_source,
            ))?;
        }
        println!("* done =)");
        Ok(())
    }
}
