use std::path::PathBuf;

use anyhow::Context;
use path_absolutize::Absolutize;

use crate::globals;

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

pub fn get_state() -> anyhow::Result<String> {
    let state_file = PathBuf::from(".").join(globals::STATE_FILE_NAME);
    std::fs::read_to_string(&state_file)
        .context(format!("could not get the state file {:?}", state_file))
}

pub fn set_state(contents: &str) -> anyhow::Result<()> {
    let state_file = PathBuf::from(".").join(globals::STATE_FILE_NAME);
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
