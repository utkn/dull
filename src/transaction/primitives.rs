use std::path::PathBuf;

use anyhow::Context;
use rand::Rng;

use crate::utils;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FsPrimitive {
    Link { original: PathBuf, target: PathBuf },
    CopyFile { source: PathBuf, target: PathBuf },
    RemoveFile(PathBuf),
    RemoveDir(PathBuf),
    TryCreateDirs(PathBuf),
    Nop,
}

impl std::fmt::Display for FsPrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsPrimitive::Link { original, target } => f.write_fmt(format_args!(
                "Link {} <= {}",
                original.display(),
                target.display()
            )),
            FsPrimitive::CopyFile { source, target } => f.write_fmt(format_args!(
                "CopyFile {} => {}",
                source.display(),
                target.display()
            )),
            FsPrimitive::RemoveFile(path) => {
                f.write_fmt(format_args!("RemoveFile {}", path.display()))
            }
            FsPrimitive::RemoveDir(path) => {
                f.write_fmt(format_args!("RemoveDir {}", path.display()))
            }
            FsPrimitive::TryCreateDirs(path) => {
                f.write_fmt(format_args!("TryCreateDirs {}", path.display()))
            }
            FsPrimitive::Nop => f.write_fmt(format_args!("Nop")),
        }
    }
}

impl FsPrimitive {
    /// Applies the primitive, modifying the filesystem. Returns the inverse primitive which restores the filesystem to its previous state.
    pub fn apply(self, backup_dir: Option<&PathBuf>) -> anyhow::Result<FsPrimitive> {
        let backup_name = format!("{}", rand::thread_rng().gen::<u32>());
        match self {
            FsPrimitive::Link { original, target } => {
                std::os::unix::fs::symlink(&original, &target)
                    .context(format!("could not link {:?} to {:?}", target, original,))?;
                Ok(Self::RemoveFile(target))
            }
            FsPrimitive::CopyFile { source, target } => {
                if let Ok(_) = std::fs::symlink_metadata(&target) {
                    anyhow::bail!("file at {:?} already exists", target);
                }
                utils::copy_file_or_symlink(&source, &target).context(format!(
                    "could not copy the file/symlink {:?} to {:?}",
                    source, target
                ))?;
                Ok(Self::RemoveFile(target))
            }
            FsPrimitive::RemoveFile(path) => {
                let undo_mod = if let Some(backup_dir) = backup_dir {
                    let backup = backup_dir.join(backup_name);
                    utils::copy_file_or_symlink(&path, &backup)
                        .context(format!("could not backup {:?} to {:?}", path, backup))?;
                    Self::CopyFile {
                        source: backup,
                        target: path.clone(),
                    }
                } else {
                    Self::Nop
                };
                std::fs::remove_file(&path).context("could not remove file {:?}")?;
                Ok(undo_mod)
            }
            FsPrimitive::RemoveDir(path) => {
                std::fs::remove_dir(&path).context(format!("could not remove dir {:?}", path))?;
                Ok(Self::TryCreateDirs(path))
            }
            FsPrimitive::TryCreateDirs(path) => {
                // No need to create a directory if it already exists.
                if path.symlink_metadata().is_ok() {
                    return Ok(FsPrimitive::Nop);
                }
                let first_created_dir = path.ancestors().find(|ancestor| {
                    let exists_or_unreachable = ancestor.try_exists().unwrap_or(true);
                    !exists_or_unreachable // definitely doesn't exist
                });
                std::fs::create_dir_all(&path).context(format!("could not create {:?}", path))?;
                if let Some(first_created_dir) = first_created_dir {
                    Ok(Self::RemoveDir(first_created_dir.to_path_buf()))
                } else {
                    Ok(FsPrimitive::Nop)
                }
            }
            FsPrimitive::Nop => Ok(FsPrimitive::Nop),
        }
    }
}
