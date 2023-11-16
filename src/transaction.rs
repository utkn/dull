mod primitives;
mod tx_apply;
pub mod tx_gen;
mod tx_processor;
mod tx_result;

use std::{collections::HashSet, path::PathBuf};

use primitives::*;
pub use tx_apply::*;
pub use tx_processor::*;
pub use tx_result::*;

#[derive(Clone, Debug)]
pub struct FsTransaction {
    name: Option<String>,
    mods: Vec<FsPrimitive>,
}

impl FsTransaction {
    pub fn empty() -> Self {
        Self {
            name: None,
            mods: Default::default(),
        }
    }

    pub fn append(&mut self, other: FsTransaction) {
        self.mods.extend(other.mods)
    }

    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl FsTransaction {
    pub fn link<P1, P2>(&mut self, original: P1, target: P2)
    where
        P1: Into<PathBuf>,
        P2: Into<PathBuf>,
    {
        self.mods.push(FsPrimitive::Link {
            original: original.into(),
            target: target.into(),
        })
    }

    pub fn copy_file<P1, P2>(&mut self, source: P1, target: P2)
    where
        P1: Into<PathBuf>,
        P2: Into<PathBuf>,
    {
        self.mods.push(FsPrimitive::CopyFile {
            source: source.into(),
            target: target.into(),
        })
    }

    pub fn remove_file<P: Into<PathBuf>>(&mut self, target: P) {
        self.mods.push(FsPrimitive::RemoveFile(target.into()))
    }

    pub fn remove_dir<P: Into<PathBuf>>(&mut self, target: P) {
        self.mods.push(FsPrimitive::RemoveDir(target.into()))
    }

    pub fn try_create_dirs<P: Into<PathBuf>>(&mut self, path: P) {
        self.mods.push(FsPrimitive::TryCreateDirs(path.into()));
    }

    /// Returns true if this transaction creates the given directory `path`.
    pub fn has_dir<P: Into<PathBuf>>(&self, path: P) -> bool {
        let mut created_dirs = HashSet::new();
        for m in &self.mods {
            match m {
                FsPrimitive::RemoveDir(p) => {
                    created_dirs.remove(p);
                }
                FsPrimitive::TryCreateDirs(p) => {
                    created_dirs.insert(p);
                }
                _ => {}
            }
        }
        created_dirs.contains(&path.into())
    }

    /// Returns true if this transaction creates the given file or symlink `path`.
    pub fn has_file<P: Into<PathBuf>>(&self, path: P) -> bool {
        let mut created_files = HashSet::new();
        for m in &self.mods {
            match m {
                FsPrimitive::RemoveFile(p) => {
                    created_files.remove(p);
                }
                FsPrimitive::CopyFile { target: p, .. } => {
                    created_files.insert(p);
                }
                FsPrimitive::Link { target: p, .. } => {
                    created_files.insert(p);
                }
                _ => {}
            }
        }
        created_files.contains(&path.into())
    }
}
