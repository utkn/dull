use std::path::PathBuf;

use anyhow::Context;

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct IncludeConfig {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ModuleConfig {
    pub source: PathBuf,
    pub target: PathBuf,
    pub linkthis: Vec<PathBuf>,
    pub linkthese: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub include: Vec<IncludeConfig>,
    pub module: Vec<ModuleConfig>,
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedConfig {
    pub modules: Vec<ModuleConfig>,
}

impl From<Config> for ResolvedConfig {
    fn from(config: Config) -> Self {
        ResolvedConfig {
            modules: config.module,
        }
    }
}

impl ResolvedConfig {
    pub fn merged(mut self, mut parent_config: ResolvedConfig) -> Self {
        self.modules.extend(parent_config.modules.drain(..));
        self
    }
}

pub fn read_config<P: Into<PathBuf>>(p: P) -> anyhow::Result<ResolvedConfig> {
    let config_file_path = p.into();
    let config: Config = std::fs::read_to_string(&config_file_path)
        .context(format!("could not read config file {:?}", config_file_path))
        .and_then(|file_contents| {
            toml::from_str(&file_contents).context(format!(
                "could not parse config file {:?}",
                config_file_path
            ))
        })?;
    if config.include.is_empty() {
        return Ok(ResolvedConfig::from(config));
    }
    let resolved_children = config
        .include
        .iter()
        .map(|include_config| (read_config(&include_config.path), &include_config.path))
        .flat_map(|(result, target_path)| {
            match &result {
                Err(err) => {
                    println!(
                        "Skipping including {:?} from {:?} due to error: {:?}",
                        target_path, config_file_path, err
                    );
                }
                Ok(_) => {}
            };
            result
        })
        .reduce(|acc, e| acc.merged(e))
        .unwrap();
    Ok(resolved_children.merged(ResolvedConfig::from(config)))
}
