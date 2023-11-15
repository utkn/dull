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

pub fn copy_recursively(
    source: &PathBuf,
    target: &PathBuf,
    ignore_filenames: &[&str],
) -> anyhow::Result<FsTransaction> {
    let mut tx = FsTransaction::empty();
    // Traverse through the regular files indicated by the leaf.
    let inner = WalkDir::new(source)
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
    for inner_source in inner {
        let inner_target = target.join(inner_source.strip_prefix(&source).unwrap());
        if let Ok(_) = std::fs::metadata(&inner_target) {
            anyhow::bail!("file at {:?} already exists", inner_target);
        }
        // Create the directories leading to the inner target.
        let target_parent = inner_target
            .parent()
            .context(format!("could not get the parent of {:?}", inner_target))?;
        tx.push(FsMod::CreateDirs(target_parent.to_path_buf()));
        tx.push(FsMod::CopyFile {
            source: inner_source,
            target: inner_target,
        });
    }
    Ok(tx)
}
