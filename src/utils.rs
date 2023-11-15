use std::path::PathBuf;

use anyhow::Context;
use walkdir::WalkDir;

use crate::{
    config_parser::{Config, GlobalConfig},
    transaction::{FsMod, FsTransaction},
};

pub fn ignore_filenames<'a>(config: &'a GlobalConfig) -> Vec<&'a str> {
    vec![&config.linkthis_file, &config.linkthese_file]
}

pub fn read_config<P: Into<PathBuf>>(p: P) -> Config {
    let config_file_path = p.into();
    let config: Config = std::fs::read_to_string(&config_file_path)
        .context(format!("could not read config file {:?}", config_file_path))
        .and_then(|file_contents| {
            toml::from_str(&file_contents).context(format!(
                "could not parse config file {:?}",
                config_file_path
            ))
        })
        .map_err(|err| {
            println!("{:?}", err);
            println!("fallback to default config");
            ()
        })
        .unwrap_or_default();
    config
}

pub fn get_state<P: Into<PathBuf>>(p: P, global_config: &GlobalConfig) -> anyhow::Result<String> {
    let root_path = p.into();
    let state_file = root_path.join(&global_config.state_file);
    std::fs::read_to_string(&state_file)
        .context(format!("could not get the state file {:?}", state_file))
}

pub fn set_state<P: Into<PathBuf>>(
    p: P,
    global_config: &GlobalConfig,
    contents: &str,
) -> anyhow::Result<()> {
    let root_path = p.into();
    let state_file = root_path.join(&global_config.state_file);
    std::fs::write(&state_file, contents)
        .context(format!("could not set the state file {:?}", state_file))
}

pub fn copy_file_or_symlink(source: &PathBuf, target: &PathBuf) -> anyhow::Result<()> {
    if std::fs::metadata(target).is_ok() {
        anyhow::bail!("target {:?} exists", target);
    }
    if source.is_symlink() {
        let canon_source = source
            .canonicalize()
            .context(format!("could not canonicalize {:?}", source))?;
        std::os::unix::fs::symlink(&canon_source, target).context(format!(
            "could not create the link {:?} to {:?}",
            target, canon_source
        ))?;
    } else {
        std::fs::copy(source, target)
            .context(format!("could not copy file {:?} to {:?}", source, target))?;
    }
    Ok(())
}

pub fn remove_dir_tx(target: &PathBuf) -> anyhow::Result<FsTransaction> {
    let mut rm_files_tx = FsTransaction::empty();
    let mut rm_dirs_tx = FsTransaction::empty();
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
        // Create the directories leading to the inner target.
        let inner_target_parent = inner_target
            .parent()
            .context(format!("could not get the parent of {:?}", inner_target))?;
        rm_files_tx.push(FsMod::RemoveFile(inner_target.clone()));
        let rm_parent_mod = FsMod::RemoveDir(inner_target_parent.to_path_buf());
        if !rm_dirs_tx.mods.contains(&rm_parent_mod) {
            rm_dirs_tx.push(rm_parent_mod);
        }
    }
    rm_dirs_tx.mods.sort_by_key(|tx| match tx {
        FsMod::RemoveDir(path) => path.components().count(),
        _ => unreachable!(),
    });
    rm_dirs_tx.mods.reverse();
    rm_dirs_tx.mods.push(FsMod::RemoveDir(target.clone()));
    rm_files_tx.append(rm_dirs_tx);
    Ok(rm_files_tx)
}
