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

impl ResolvedConfig {
    fn root(config: Config) -> Self {
        ResolvedConfig {
            modules: config.module,
        }
    }
    /// Merges this configuration with the given `parent_config` and returns the result.
    fn merged(mut self, mut parent_config: ResolvedConfig) -> Self {
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
    let inclusions = config
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
        .reduce(|acc, e| acc.merged(e));
    let parent = ResolvedConfig::root(config);
    match inclusions {
        Some(inclusions) => Ok(inclusions.merged(parent)),
        None => Ok(parent),
    }
}
