mod primitives;
mod tx_apply;
pub mod tx_gen;
mod tx_processor;
mod tx_result;

use std::{collections::HashSet, path::PathBuf};

use anyhow::Context;
use primitives::*;
use rand::Rng;
pub use tx_apply::*;
pub use tx_processor::*;
pub use tx_result::*;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ephemeral;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Concrete {
    id: String,
    backup_dir: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Transaction<T> {
    name: String,
    mods: Vec<FsPrimitive>,
    data: T,
}

impl Transaction<Ephemeral> {
    pub fn empty<S: Into<String>>(name: S) -> Self {
        let name = name.into();
        Self {
            name,
            mods: Default::default(),
            data: Ephemeral,
        }
    }

    fn from_primitives<S, V>(name: S, mods: V) -> Self
    where
        S: Into<String>,
        V: IntoIterator<Item = FsPrimitive>,
    {
        let mut tx = Transaction::empty(name);
        tx.mods = mods.into_iter().collect();
        tx
    }

    pub fn append(&mut self, other: Self) {
        self.mods.extend(other.mods)
    }

    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = name.into();
        self
    }

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
        let self_summary = TxSummary::from(&*self);
        if path.try_exists().unwrap_or(true) || self_summary.creates_dirs(&path) {
            return;
        }
        self.mods.push(FsPrimitive::CreateDirs(path));
    }

    /// Finalizes the transaction.
    pub fn finalize(self) -> anyhow::Result<Transaction<Concrete>> {
        Transaction::finalized(self)
    }
}

impl Transaction<Concrete> {
    /// Constructs a concrete transaction that can be executed.
    pub fn finalized(tx: Transaction<Ephemeral>) -> anyhow::Result<Self> {
        // Create a random transaction id.
        let id = format!("{}-{}", tx.name, rand::thread_rng().gen::<u32>());
        // Create a backup directory for the transaction.
        let backup_dir = PathBuf::from("transactions").join(&id);
        std::fs::create_dir_all(&backup_dir)
            .context(format!("could not create the backup directory"))?;
        let tx_file_path = backup_dir.join("tx");
        // Construct the concrete transaction.
        let concrete_tx = Transaction {
            name: tx.name,
            mods: tx.mods,
            data: Concrete { id, backup_dir },
        };
        // Write it into a file.
        let tx_file = std::fs::File::create(&tx_file_path).context(format!(
            "could not write the transaction file at {:?}",
            tx_file_path
        ))?;
        let tx_wr = std::io::BufWriter::new(tx_file);
        serde_json::to_writer(tx_wr, &concrete_tx).context(format!(
            "could not serialize the transaction into {:?}",
            tx_file_path
        ))?;
        // Return the concretized transaction.
        Ok(concrete_tx)
    }

    /// Reads a concrete transaction from a file.
    pub fn read(path: PathBuf) -> anyhow::Result<Self> {
        let tx_file = std::fs::File::open(&path)
            .context(format!("could not read the transaction file at {:?}", path))?;
        let tx_rd = std::io::BufReader::new(tx_file);
        serde_json::from_reader(tx_rd).context(format!(
            "could not deserialzie the transaction from {:?}",
            path
        ))
    }
}

pub struct TxSummary {
    files_to_create: HashSet<PathBuf>,
    files_to_remove: HashSet<PathBuf>,
    dirs_to_create: HashSet<PathBuf>,
    dirs_to_remove: HashSet<PathBuf>,
}

impl<T> From<&Transaction<T>> for TxSummary {
    fn from(tx: &Transaction<T>) -> Self {
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
