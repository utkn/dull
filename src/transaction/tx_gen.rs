use std::{collections::HashSet, path::PathBuf};

use anyhow::Context;
use itertools::Itertools;
use walkdir::WalkDir;

use super::{FsPrimitive, FsTransaction};

/// Constructs a transaction that removes the given directory with all of its contents.
pub fn remove_dir(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    let mut rm_files_mods = HashSet::<FsPrimitive>::new();
    let mut rm_dirs_mods = HashSet::<FsPrimitive>::new();
    if !target.is_dir() {
        anyhow::bail!("target {:?} is not a directory", target)
    }
    let target_files = WalkDir::new(target)
        .follow_root_links(false)
        .follow_links(false)
        .into_iter()
        .flatten()
        .map(|p| p.path().to_path_buf())
        // Only consider symlinks or regular files.
        .filter(|p| p.is_symlink() || p.is_file());
    for inner_target in target_files {
        let inner_target_parent = inner_target
            .parent()
            .context(format!("could not get the parent of {:?}", inner_target))?;
        rm_files_mods.insert(FsPrimitive::RemoveFile(inner_target.clone()));
        rm_dirs_mods.insert(FsPrimitive::RemoveDir(inner_target_parent.to_path_buf()));
    }
    rm_dirs_mods.insert(FsPrimitive::RemoveDir(target.clone()));
    let mut tx = FsTransaction::empty();
    tx.mods.extend(rm_files_mods);
    tx.mods.extend(
        rm_dirs_mods
            .into_iter()
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
        tx.push(FsPrimitive::RemoveFile(target.clone()));
    } else {
        tx.append(remove_dir(target)?);
    }
    Ok(tx)
}
