use std::path::PathBuf;

use anyhow::{bail, Context};
use itertools::Itertools;
use walkdir::WalkDir;

use crate::{
    config_parser::{GlobalConfig, ModuleConfig},
    utils,
    virtual_system::ResolvedLink,
};

#[derive(Default, Debug, Clone)]
pub struct Module {
    module_path: PathBuf,
    sources: Vec<PathBuf>,
}

impl Module {
    pub fn emplace(self, target_path: PathBuf) -> Vec<ResolvedLink> {
        self.sources
            .into_iter()
            .map(|source| {
                source
                    .strip_prefix(&self.module_path)
                    .map(|stripped| stripped.to_owned())
                    .map(|stripped| (source, stripped))
            })
            .flatten()
            .flat_map(|(source, source_stripped)| {
                let mut resolved_target = target_path.clone();
                resolved_target.push(source_stripped);
                ResolvedLink::new(source, resolved_target)
            })
            .collect_vec()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum TraversalDirective<'a> {
    LinkThis(&'a PathBuf),
    LinkThese(&'a PathBuf),
}

#[derive(Clone, Debug)]
pub enum TraversalStrategy {
    LinkThis(PathBuf),
    LinkThese(Vec<PathBuf>),
    Recurse(Vec<PathBuf>),
    Skip,
}

impl TraversalStrategy {
    /// Consumes the given path and returns the traversal strategy associated with it.
    pub fn try_determine(
        path: PathBuf,
        directives: &[TraversalDirective],
        ignore_filenames: &[&str],
    ) -> anyhow::Result<Self> {
        // If the path doesn't exists or is it unreachable, return `None`.
        if !path.try_exists().is_ok_and(|exists| exists) {
            bail!("unreachable path {:?}", path);
        }
        if ignore_filenames.contains(
            &path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        ) {
            return Ok(TraversalStrategy::Skip);
        }
        // A file must always be linked directly.
        if path.is_file() {
            return Ok(TraversalStrategy::LinkThis(path));
        }
        // A directory that should be treated as a file.
        let linkthis_directive = directives.contains(&TraversalDirective::LinkThis(&path));
        if linkthis_directive {
            return Ok(TraversalStrategy::LinkThis(path));
        }
        // Get the directory contents.
        let dir_contents = path
            .read_dir()
            .context(format!("could not read the directory contents {:?}", path))?
            .flatten()
            .map(|f| f.path())
            .filter(|p| {
                !ignore_filenames
                    .contains(&p.file_name().unwrap_or_default().to_string_lossy().as_ref())
            })
            .collect_vec();
        // A directory can be either traversed recursively or not.
        let linkthese_directive = directives.contains(&TraversalDirective::LinkThese(&path));
        if linkthese_directive {
            return Ok(TraversalStrategy::LinkThese(dir_contents));
        }
        return Ok(TraversalStrategy::Recurse(dir_contents));
    }
}

#[derive(Debug)]
pub struct ModuleParser<'a> {
    module_config: &'a ModuleConfig,
    global_config: &'a GlobalConfig,
}

impl<'a> ModuleParser<'a> {
    pub fn from_config(module_config: &'a ModuleConfig, global_config: &'a GlobalConfig) -> Self {
        Self {
            module_config,
            global_config,
        }
    }

    pub fn parse(self) -> anyhow::Result<Module> {
        let source = &self.module_config.source;
        println!("* parsing module {:?}", source);
        if !source.is_dir() {
            bail!(
                "module path {:?} is not a directory",
                self.module_config.source
            );
        }
        // Read the directives from all the files under the module source root.
        let all_files = WalkDir::new(source)
            .into_iter()
            .flatten()
            .flat_map(|dir_entry| {
                let file_path = dir_entry.into_path();
                let parent_path = file_path.parent()?.to_path_buf();
                Some((parent_path, file_path))
            })
            .collect_vec();
        let mut directives = all_files
            .iter()
            .flat_map(|(parent, file)| {
                let file_name = file.file_name()?.to_string_lossy();
                if file_name == self.global_config.linkthis_file {
                    Some(TraversalDirective::LinkThis(parent))
                } else if file_name == self.global_config.linkthese_file {
                    Some(TraversalDirective::LinkThese(parent))
                } else {
                    None
                }
            })
            .collect_vec();
        // Read directives from the configuration.
        directives.extend(
            self.module_config
                .linkthis
                .iter()
                .map(|p| TraversalDirective::LinkThis(p)),
        );
        directives.extend(
            self.module_config
                .linkthese
                .iter()
                .map(|p| TraversalDirective::LinkThese(p)),
        );
        println!("collected directives {:?}", directives);
        let mut collected_paths = vec![];
        let mut frontier = vec![source.clone()];
        while frontier.len() > 0 {
            let curr_path = frontier.pop().expect("could not pop from the frontier");
            match TraversalStrategy::try_determine(
                curr_path,
                &directives,
                &utils::ignore_filenames(self.global_config),
            ) {
                Ok(strategy) => match strategy {
                    TraversalStrategy::LinkThis(path) => {
                        collected_paths.push(path);
                    }
                    TraversalStrategy::LinkThese(paths) => {
                        collected_paths.extend(paths);
                    }
                    TraversalStrategy::Recurse(paths) => {
                        let inner_dirs = paths.clone().into_iter().filter(|path| path.is_dir());
                        let inner_files = paths.into_iter().filter(|path| path.is_file());
                        collected_paths.extend(inner_files);
                        frontier.extend(inner_dirs);
                    }
                    TraversalStrategy::Skip => {
                        continue;
                    }
                },
                Err(err) => {
                    println!("skipping due to error: {:?}", err)
                }
            }
        }
        Ok(Module {
            module_path: source.clone(),
            sources: collected_paths,
        })
    }
}