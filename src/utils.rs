use std::path::PathBuf;

use anyhow::Context;
use path_absolutize::Absolutize;

use crate::config_parser::{Config, GlobalConfig};

#[derive(Clone, Debug)]
pub struct ResolvedLink {
    pub abs_source: PathBuf,
    pub abs_target: PathBuf,
}

impl ResolvedLink {
    pub fn new(source: &PathBuf, target: &PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            abs_source: expand_path(source)?,
            abs_target: expand_path(target)?,
        })
    }
}

pub fn expand_path(path: &PathBuf) -> anyhow::Result<PathBuf> {
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
            println!("Error: {:?}", err);
            println!("Falling back to default config");
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
    if target.symlink_metadata().is_ok() {
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
