use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use itertools::Itertools;

use super::{primitives::FsPrimitive, Transaction};

/// A type that can be used to build transactions.
pub struct TxBuilder {
    files_to_create: HashMap<PathBuf, FsPrimitive>,
    files_to_remove: HashMap<PathBuf, FsPrimitive>,
    dirs_to_create: HashMap<PathBuf, FsPrimitive>,
    dirs_to_remove: HashMap<PathBuf, FsPrimitive>,
}

impl TxBuilder {
    /// Creates an empty transaction builder.
    pub fn empty() -> Self {
        Self {
            files_to_create: Default::default(),
            files_to_remove: Default::default(),
            dirs_to_create: Default::default(),
            dirs_to_remove: Default::default(),
        }
    }

    /// Returns true iff the transaction generated at the current state of the builder would create the given directory.
    pub fn will_create_dir(&self, p: &PathBuf) -> bool {
        self.dirs_to_create.contains_key(p)
    }

    /// Returns true iff the transaction generated at the current state of the builder would remove the given directory.
    pub fn _will_remove_dir(&self, p: &PathBuf) -> bool {
        self.dirs_to_remove.contains_key(p)
    }

    /// Returns true iff the transaction generated at the current state of the builder would create the given file.
    pub fn _will_create_file(&self, p: &PathBuf) -> bool {
        self.files_to_create.contains_key(p)
    }

    /// Returns true iff the transaction generated at the current state of the builder would remove the given file.
    pub fn _will_remove_file(&self, p: &PathBuf) -> bool {
        self.files_to_remove.contains_key(p)
    }

    pub(super) fn push(&mut self, p: FsPrimitive) {
        match &p {
            FsPrimitive::Link {
                original: _,
                target,
            } => {
                self.files_to_remove.remove(target);
                self.files_to_create.insert(target.clone(), p.clone());
            }
            FsPrimitive::CopyFile { source: _, target } => {
                self.files_to_remove.remove(target);
                self.files_to_create.insert(target.clone(), p.clone());
            }
            FsPrimitive::RemoveFile(target) => {
                self.files_to_create.remove(target);
                self.files_to_remove.insert(target.clone(), p.clone());
            }
            FsPrimitive::RemoveDir(target) => {
                self.dirs_to_create.remove(target);
                self.dirs_to_remove.insert(target.clone(), p.clone());
            }
            FsPrimitive::CreateDir(target) => {
                self.dirs_to_remove.remove(target);
                self.dirs_to_create.insert(target.clone(), p.clone());
            }
            FsPrimitive::Nop => {}
        }
    }

    /// Appends an instruction to create a symlink at `target` to the file exists at `source` .
    /// Note that `target` must not exist, but it's parent directories must exist.
    pub fn link<P1, P2>(&mut self, original: P1, target: P2)
    where
        P1: Into<PathBuf>,
        P2: Into<PathBuf>,
    {
        self.push(FsPrimitive::Link {
            original: original.into(),
            target: target.into(),
        });
    }

    /// Appends an instruction to copy the file that exists at `source` to `target`.
    /// Note that `target` must not exist, but it's parent directories must exist.
    pub fn copy_file<P1, P2>(&mut self, source: P1, target: P2)
    where
        P1: Into<PathBuf>,
        P2: Into<PathBuf>,
    {
        self.push(FsPrimitive::CopyFile {
            source: source.into(),
            target: target.into(),
        });
    }

    /// Appends an instruction to remove the file/symlink at the given path.
    pub fn remove_file<P: Into<PathBuf>>(&mut self, target: P) {
        self.push(FsPrimitive::RemoveFile(target.into()));
    }

    /// Appends an instruction to create an empty directory at the given free path.
    pub fn create_dir<P: Into<PathBuf>>(&mut self, target: P) {
        self.push(FsPrimitive::CreateDir(target.into()));
    }

    /// Appends an instruction to remove the free directory at the given path.
    pub fn remove_dir<P: Into<PathBuf>>(&mut self, target: P) {
        self.push(FsPrimitive::RemoveDir(target.into()));
    }

    /// Returns the number of final instructions this builder would generate.
    pub fn len(&self) -> usize {
        self.dirs_to_create.len()
            + self.dirs_to_remove.len()
            + self.files_to_create.len()
            + self.files_to_remove.len()
    }

    /// Builds an actual transaction that can be executed.
    pub fn build<S: Into<String>>(self, name: S) -> anyhow::Result<Transaction> {
        let name = name.into();
        let mut primitives = Vec::with_capacity(self.len());
        primitives.extend(
            self.dirs_to_create
                .into_iter()
                .sorted_by_key(|(dir, _)| dir.components().count())
                .map(|(_, prm)| prm),
        );
        primitives.extend(
            self.files_to_create
                .into_iter()
                .sorted_by_key(|(dir, _)| dir.components().count())
                .map(|(_, prm)| prm),
        );
        primitives.extend(
            self.files_to_remove
                .into_iter()
                .sorted_by_key(|(dir, _)| dir.components().count())
                .rev()
                .map(|(_, prm)| prm),
        );
        primitives.extend(
            self.dirs_to_remove
                .into_iter()
                .sorted_by_key(|(dir, _)| dir.components().count())
                .rev()
                .map(|(_, prm)| prm),
        );
        Transaction::generate(name.clone(), primitives)
            .context(format!("could not build the transaction {:?}", name))
    }
}
