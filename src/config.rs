use std::path::PathBuf;

use anyhow::Context;

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub global: GlobalConfig,
    pub module: Vec<ModuleConfig>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct GlobalConfig {
    pub linkthis_file: String,
    pub linkthese_file: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            linkthis_file: String::from(".dull-linkthis"),
            linkthese_file: String::from(".dull-linkthese"),
        }
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ModuleConfig {
    pub source: PathBuf,
    pub target: PathBuf,
    pub linkthis: Vec<PathBuf>,
    pub linkthese: Vec<PathBuf>,
}

pub fn read_config<P: Into<PathBuf>>(p: P) -> Config {
    let config_file_path = p.into();
    let config: Config = std::fs::read_to_string(config_file_path.clone())
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
