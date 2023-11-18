use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;
use walkdir::WalkDir;

use crate::utils;

use super::TxBuilder;

/// Captures the current state of the filesystem.
pub struct ActualFilesystem;

impl TxBuilder {
    /// Instruct to remove the given directory with all of its contents.
    pub fn remove_dir_all(
        &mut self,
        target: &PathBuf,
        _fs: &ActualFilesystem,
    ) -> anyhow::Result<()> {
        if target.is_symlink() || target.is_file() {
            anyhow::bail!("target {:?} is not a directory", target)
        }
        // Construct the transaction.
        let target_files = WalkDir::new(target)
            .follow_root_links(false)
            .follow_links(false)
            .into_iter()
            .flatten()
            .map(|p| p.path().to_path_buf())
            // Start removing from the innermost paths (stable sort is important)
            .sorted_by_key(|p| p.components().count())
            .rev();
        for inner_target in target_files {
            let is_file = inner_target.is_symlink() || inner_target.is_file();
            if is_file {
                self.remove_file(inner_target);
            } else {
                self.remove_dir(inner_target);
            }
        }
        Ok(())
    }

    /// Instruct to remove anything in the given target. If `target` is a symlink, only removes the symlink.
    pub fn remove_any(&mut self, target: &PathBuf, fs: &ActualFilesystem) -> anyhow::Result<()> {
        if target.is_symlink() || target.is_file() {
            self.remove_file(target);
        } else {
            self.remove_dir_all(target, fs)?;
        }
        Ok(())
    }

    /// Instruct to create the given symlinks.
    pub fn create_links<P: Into<PathBuf>>(
        &mut self,
        root: P,
        links: Vec<utils::ResolvedLink>,
        fs: &ActualFilesystem,
    ) -> anyhow::Result<()> {
        let root: PathBuf = root.into();
        for link in links.into_iter() {
            let mut curr_virt_target = root.clone();
            let relativized_target = if link.abs_target.is_absolute() {
                link.abs_target.strip_prefix("/")?
            } else {
                link.abs_target.as_path()
            };
            curr_virt_target.push(relativized_target);
            curr_virt_target = utils::expand_path(&curr_virt_target)?;
            // Create the virtual directory if it does not exist.
            let curr_virt_target_parent = curr_virt_target.parent().context(format!(
                "could not get the parent of {:?}",
                curr_virt_target
            ))?;
            self.ensure_dirs(curr_virt_target_parent, fs)?;
            self.link(link.abs_source, curr_virt_target);
        }
        Ok(())
    }

    /// Instruct to ensure the existence of the given directory.
    pub fn ensure_dirs<P: Into<PathBuf>>(
        &mut self,
        target: P,
        _fs: &ActualFilesystem,
    ) -> anyhow::Result<()> {
        let path = target.into();
        // Get the ancestor paths.
        let ancestors = path
            .ancestors()
            .map(|ancestor| ancestor.to_path_buf())
            .collect_vec();
        // For each parent subdirectory that does not exist, add a new create dir primitive.
        ancestors
            .into_iter()
            .rev()
            .filter(|subdir| !subdir.symlink_metadata().is_ok())
            .filter(|subdir| !self.will_create_dir(subdir))
            .collect_vec()
            .into_iter()
            .for_each(|subdir| self.create_dir(subdir.clone()));
        Ok(())
    }
}
