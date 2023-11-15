use std::path::PathBuf;

use anyhow::Context;
use rand::Rng;

use crate::utils;

#[derive(Clone, Debug)]
pub enum FsMod {
    Link { original: PathBuf, target: PathBuf },
    CopyFile { source: PathBuf, target: PathBuf },
    RemoveAll(PathBuf),
    CreateDirs(PathBuf),
    Nop,
}

impl FsMod {
    pub fn apply(self, backup_dir: &PathBuf) -> anyhow::Result<FsMod> {
        match self {
            FsMod::Link { original, target } => {
                std::os::unix::fs::symlink(&original, &target).context(format!(
                    "could not create the symlink {:?} => {:?}",
                    original, target
                ))?;
                Ok(Self::RemoveAll(target))
            }
            FsMod::CopyFile { source, target } => {
                if let Ok(_) = std::fs::symlink_metadata(&target) {
                    anyhow::bail!("file at {:?} already exists", target);
                }
                std::fs::copy(&source, &target)
                    .context(format!("could not copy {:?} to {:?}", source, target))?;
                Ok(Self::RemoveAll(target))
            }
            FsMod::RemoveAll(path) => {
                return Ok(Self::Nop);
                let backup_file_name = format!("{}", rand::thread_rng().gen::<u32>());
                let backup = backup_dir.join(backup_file_name);
                utils::copy_recursively(&path, &backup, &[])
                    .and_then(|tx| tx.run_haphazard(false))
                    .context(format!("could not backup {:?}", path))?;
                std::fs::remove_dir_all(&path).context(format!("could not remove {:?}", path))?;
                Ok(Self::CopyFile {
                    source: backup,
                    target: path,
                })
            }
            FsMod::CreateDirs(path) => {
                let mut curr_subdir = path.clone();
                let mut subdirs = vec![];
                loop {
                    subdirs.push(curr_subdir.clone());
                    if !curr_subdir.pop() {
                        break;
                    }
                }
                let first_created_dir = subdirs
                    .into_iter()
                    .rev()
                    .find(|subdir| !subdir.try_exists().unwrap_or(true));
                std::fs::create_dir_all(&path)
                    .context(format!("could not create the dirs {:?}", path))?;
                if let Some(first_created_dir) = first_created_dir {
                    Ok(Self::RemoveAll(first_created_dir))
                } else {
                    Ok(FsMod::Nop)
                }
            }
            FsMod::Nop => Ok(FsMod::Nop),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FsTransaction {
    mods: Vec<FsMod>,
}

impl FsTransaction {
    pub fn empty() -> Self {
        Self {
            mods: Default::default(),
        }
    }

    pub fn push(&mut self, fs_mod: FsMod) {
        self.mods.push(fs_mod);
    }

    pub fn append(&mut self, other: FsTransaction) {
        self.mods.extend(other.mods)
    }

    pub fn run_haphazard(self, verbose: bool) -> anyhow::Result<()> {
        for m in self.mods.into_iter() {
            println!("=> {:?}", m);
            m.apply(&PathBuf::from("/dev/null"))?;
        }
        Ok(())
    }

    pub fn run_atomic(self, verbose: bool) -> anyhow::Result<()> {
        println!("Running transaction...");
        let mut history = vec![];
        let mut rollback = false;
        for m in self.mods.into_iter() {
            println!("=> {:?}", m);
            match m.apply(&PathBuf::from("backups")) {
                Ok(undo_mod) => {
                    history.push(undo_mod);
                }
                Err(err) => {
                    println!("! error while running the transaction: {:?}", err);
                    rollback = true;
                    break;
                }
            }
        }
        if rollback {
            println!("trying to rollback...");
            for m_inv in history.into_iter().rev() {
                println!("=> {:?}", m_inv);
                if let Err(err) = m_inv.apply(&PathBuf::from("/dev/null")) {
                    println!("! could not rollback: {:?}", err);
                    println!("! i am out, sorry i messed up your system =(");
                    anyhow::bail!("transaction failed destructively");
                }
            }
            anyhow::bail!("transaction failed gracefully");
        } else {
            println!("Transaction ran successfully");
            Ok(())
        }
    }
}
