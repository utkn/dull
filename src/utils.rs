use std::path::PathBuf;

use anyhow::Context;

use crate::config_parser::{Config, GlobalConfig};

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

pub fn read_state<P: Into<PathBuf>>(p: P, global_config: &GlobalConfig) -> anyhow::Result<String> {
    let root_path = p.into();
    let state_file = root_path.join(&global_config.build_file);
    std::fs::read_to_string(&state_file)
        .context(format!("could not read the state file {:?}", state_file))
}

pub fn set_state<P: Into<PathBuf>>(
    p: P,
    global_config: &GlobalConfig,
    contents: &str,
) -> anyhow::Result<()> {
    let root_path = p.into();
    let state_file = root_path.join(&global_config.build_file);
    std::fs::write(&state_file, contents)
        .context(format!("could not write the state file {:?}", state_file))
}
