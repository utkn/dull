use std::path::PathBuf;

use anyhow::Context;
use itertools::Itertools;
use walkdir::WalkDir;

use crate::utils;

use super::{Ephemeral, Transaction};

/// Constructs a transaction that removes the given directory with all of its contents.
pub fn remove_dir(target: &PathBuf) -> anyhow::Result<Transaction<Ephemeral>> {
    if target.is_symlink() || target.is_file() {
        anyhow::bail!("target {:?} is not a directory", target)
    }
    // Construct the transaction.
    let mut tx = Transaction::empty("RemoveDir");
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
pub fn remove_any(target: &PathBuf) -> anyhow::Result<Transaction<Ephemeral>> {
    let mut tx = Transaction::empty("RemoveAny");
    if target.is_symlink() || target.is_file() {
        tx.remove_file(target);
    } else {
        tx.append(remove_dir(target)?);
    }
    Ok(tx)
}

pub fn build_at_root<P: Into<PathBuf>>(
    root: P,
    links: Vec<utils::ResolvedLink>,
) -> anyhow::Result<Transaction<Ephemeral>> {
    let mut tx = Transaction::empty("BuildAtRoot");
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
        tx.try_create_dirs(curr_virt_target_parent);
        tx.link(link.abs_source, curr_virt_target);
    }
    Ok(tx)
}
