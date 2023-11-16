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
    name: String,
    mods: Vec<FsPrimitive>,
}

impl FsTransaction {
    pub fn empty<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            mods: Default::default(),
        }
    }

    pub fn append(&mut self, other: FsTransaction) {
        self.mods.extend(other.mods)
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

    pub fn remove_empty_dir<P: Into<PathBuf>>(&mut self, target: P) {
        self.mods.push(FsPrimitive::RemoveEmptyDir(target.into()))
    }

    pub fn create_dirs<P: Into<PathBuf>>(&mut self, target: P) {
        self.mods.push(FsPrimitive::CreateDirs(target.into()));
    }

    pub fn try_create_dirs<P: Into<PathBuf>>(&mut self, target: P) {
        let path = target.into();
        if path.try_exists().unwrap_or(true) || self.summarize().creates_dirs(&path) {
            return;
        }
        self.mods.push(FsPrimitive::CreateDirs(path));
    }

    pub fn summarize(&self) -> TxSummary {
        TxSummary::from(self)
    }
}

pub struct TxSummary {
    files_to_create: HashSet<PathBuf>,
    files_to_remove: HashSet<PathBuf>,
    dirs_to_create: HashSet<PathBuf>,
    dirs_to_remove: HashSet<PathBuf>,
}

impl From<&FsTransaction> for TxSummary {
    fn from(tx: &FsTransaction) -> Self {
        let mut files_to_create = HashSet::new();
        let mut files_to_remove = HashSet::new();
        let mut dirs_to_create = HashSet::new();
        let mut dirs_to_remove = HashSet::new();
        for m in &tx.mods {
            match m {
                FsPrimitive::RemoveFile(p) => {
                    files_to_remove.insert(p.to_path_buf());
                    files_to_create.remove(p);
                }
                FsPrimitive::CopyFile { target: p, .. } => {
                    files_to_create.insert(p.to_path_buf());
                    files_to_remove.remove(p);
                }
                FsPrimitive::Link { target: p, .. } => {
                    files_to_create.insert(p.to_path_buf());
                    files_to_remove.remove(p);
                }
                FsPrimitive::RemoveEmptyDir(p) => {
                    dirs_to_remove.insert(p.to_path_buf());
                    // TODO: raise an error if there exists a `TryCreateDirs` command
                    // with `p` is a strict prefix.
                    dirs_to_create.remove(p);
                }
                FsPrimitive::CreateDirs(p) => {
                    dirs_to_create.insert(p.to_path_buf());
                    // The directories that were removed previously that are prefixes of `p`
                    // are recreated.
                    dirs_to_remove = dirs_to_remove
                        .drain()
                        .filter(|rm_dir| !p.starts_with(rm_dir))
                        .collect();
                }
                FsPrimitive::Nop => {}
            }
        }
        Self {
            files_to_create,
            files_to_remove,
            dirs_to_create,
            dirs_to_remove,
        }
    }
}

impl TxSummary {
    pub fn creates_file(&self, p: &PathBuf) -> bool {
        self.files_to_create.contains(p)
    }

    pub fn removes_file(&self, p: &PathBuf) -> bool {
        self.files_to_remove.contains(p)
    }

    pub fn creates_dirs(&self, p: &PathBuf) -> bool {
        self.dirs_to_create.contains(p)
    }

    pub fn removes_empty_dir(&self, p: &PathBuf) -> bool {
        self.dirs_to_remove.contains(p)
    }
}
