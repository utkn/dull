use std::path::PathBuf;

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
    pub build_file: String,
    pub state_file: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            linkthis_file: String::from(".dull-linkthis"),
            linkthese_file: String::from(".dull-linkthese"),
            build_file: String::from(".dull-build"),
            state_file: String::from(".dull-state"),
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
