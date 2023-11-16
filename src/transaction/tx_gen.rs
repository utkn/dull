use std::path::PathBuf;

use itertools::Itertools;
use walkdir::WalkDir;

use super::FsTransaction;

/// Constructs a transaction that removes the given directory with all of its contents.
pub fn remove_dir(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    if target.is_symlink() || target.is_file() {
        anyhow::bail!("target {:?} is not a directory", target)
    }
    // Construct the transaction.
    let mut tx = FsTransaction::empty("remove dir");
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
        if inner_target.is_symlink() || inner_target.is_file() {
            tx.remove_file(inner_target);
        } else {
            tx.remove_empty_dir(inner_target);
        }
    }
    Ok(tx)
}

/// Constructs a transaction that removes anything in the given target. If `target` is a symlink, only removes the symlink.
pub fn remove_any(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    let mut tx = FsTransaction::empty("remove any");
    if target.is_symlink() || target.is_file() {
        tx.remove_file(target);
    } else {
        tx.append(remove_dir(target)?);
    }
    Ok(tx)
}
