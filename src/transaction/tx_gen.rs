use std::{collections::HashSet, path::PathBuf};

use itertools::Itertools;
use walkdir::WalkDir;

use super::{FsPrimitive, FsTransaction};

/// Constructs a transaction that removes the given directory with all of its contents.
pub fn remove_dir(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    if target.is_symlink() || target.is_file() {
        anyhow::bail!("target {:?} is not a directory", target)
    }
    // Maintain the removal modifications for files and directories separately.
    let mut rm_files_mods = HashSet::<FsPrimitive>::new();
    let mut rm_dirs_mods = HashSet::<FsPrimitive>::new();
    let target_files = WalkDir::new(target)
        .follow_root_links(false)
        .follow_links(false)
        .into_iter()
        .flatten()
        .map(|p| p.path().to_path_buf());
    for inner_target in target_files {
        if inner_target.is_symlink() || inner_target.is_file() {
            rm_files_mods.insert(FsPrimitive::RemoveFile(inner_target));
        } else {
            rm_dirs_mods.insert(FsPrimitive::RemoveDir(inner_target));
        }
    }
    // Construct the transaction.
    let mut tx = FsTransaction::empty();
    // First, remove the files (in any order)
    tx.mods.extend(rm_files_mods);
    // Then, remove the directories
    tx.mods.extend(
        rm_dirs_mods
            .into_iter()
            // Remove the inner directories first.
            .sorted_by_key(|tx| match tx {
                FsPrimitive::RemoveDir(path) => path.components().count(),
                _ => unreachable!(),
            })
            .rev(),
    );
    Ok(tx)
}

/// Constructs a transaction that removes anything in the given target. If `target` is a symlink, only removes the symlink.
pub fn remove_any(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    let mut tx = FsTransaction::empty();
    if target.is_symlink() || target.is_file() {
        tx.remove_file(target);
    } else {
        tx.append(remove_dir(target)?);
    }
    Ok(tx)
}
