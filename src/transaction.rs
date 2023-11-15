use std::path::PathBuf;

use anyhow::Context;
use rand::Rng;

use crate::utils;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FsMod {
    Link { original: PathBuf, target: PathBuf },
    CopyFile { source: PathBuf, target: PathBuf },
    RemoveFile(PathBuf),
    RemoveDir(PathBuf),
    CreateDirs(PathBuf),
    Nop,
}

impl std::fmt::Display for FsMod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsMod::Link { original, target } => f.write_fmt(format_args!(
                "Link {} <= {}",
                original.display(),
                target.display()
            )),
            FsMod::CopyFile { source, target } => f.write_fmt(format_args!(
                "CopyFile {} => {}",
                source.display(),
                target.display()
            )),
            FsMod::RemoveFile(path) => f.write_fmt(format_args!("RemoveFile {}", path.display())),
            FsMod::RemoveDir(path) => f.write_fmt(format_args!("RemoveDir {}", path.display())),
            FsMod::CreateDirs(path) => f.write_fmt(format_args!("CreateDirs {}", path.display())),
            FsMod::Nop => f.write_fmt(format_args!("Nop")),
        }
    }
}

impl FsMod {
    pub fn apply(self, backup_dir: Option<&PathBuf>) -> anyhow::Result<FsMod> {
        let backup_name = format!("{}", rand::thread_rng().gen::<u32>());
        match self {
            FsMod::Link { original, target } => {
                std::os::unix::fs::symlink(&original, &target)
                    .context(format!("could not link {:?} to {:?}", target, original,))?;
                Ok(Self::RemoveFile(target))
            }
            FsMod::CopyFile { source, target } => {
                if let Ok(_) = std::fs::symlink_metadata(&target) {
                    anyhow::bail!("file at {:?} already exists", target);
                }
                utils::copy_file_or_symlink(&source, &target)
                    .context(format!("could not copy {:?} to {:?}", source, target))?;
                Ok(Self::RemoveFile(target))
            }
            FsMod::RemoveFile(path) => {
                let undo_mod = if let Some(backup_dir) = backup_dir {
                    let backup = backup_dir.join(backup_name);
                    utils::copy_file_or_symlink(&path, &backup)
                        .context(format!("could not backup {:?} to {:?}", path, backup))?;
                    Self::CopyFile {
                        source: backup,
                        target: path.clone(),
                    }
                } else {
                    Self::Nop
                };
                std::fs::remove_file(&path).context("could not remove file {:?}")?;
                Ok(undo_mod)
            }
            FsMod::RemoveDir(path) => {
                std::fs::remove_dir(&path).context(format!("could not remove dir {:?}", path))?;
                Ok(Self::CreateDirs(path))
            }
            FsMod::CreateDirs(path) => {
                // No need to create a directory if it already exists.
                if path.symlink_metadata().is_ok() {
                    return Ok(FsMod::Nop);
                }
                let first_created_dir = path.ancestors().find(|ancestor| {
                    let exists_or_unreachable = ancestor.try_exists().unwrap_or(true);
                    !exists_or_unreachable // definitely doesn't exist
                });
                std::fs::create_dir_all(&path).context(format!("could not create {:?}", path))?;
                if let Some(first_created_dir) = first_created_dir {
                    Ok(Self::RemoveDir(first_created_dir.to_path_buf()))
                } else {
                    Ok(FsMod::Nop)
                }
            }
            FsMod::Nop => Ok(FsMod::Nop),
        }
    }
}

#[derive(Debug)]
pub struct FsTransactionResult {
    pub tx_result: anyhow::Result<()>,
    pub rb_result: Option<anyhow::Result<()>>,
}

impl FsTransactionResult {
    pub fn fatal_failure(&self) -> bool {
        matches!(self.rb_result, Some(Err(_)))
    }

    pub fn display_report(&self) {
        println!("-------");
        if let Err(tx_err) = &self.tx_result {
            println!("Transaction error: {:?}", tx_err);
            match &self.rb_result {
                Some(Err(rb_err)) => {
                    println!("-------");
                    println!("Rollback error: {:?}", rb_err);
                }
                Some(Ok(())) => {
                    println!("-------");
                    println!("Rollback succeeded.");
                }
                _ => (),
            }
            return;
        }
        println!("Transaction succeeded.");
    }
}

#[derive(Clone, Debug)]
pub struct FsTransaction {
    pub mods: Vec<FsMod>,
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

    fn rollback_tx(
        history: Vec<FsMod>,
        tx_err: Option<anyhow::Error>,
        verbose: bool,
    ) -> FsTransactionResult {
        if let Some(tx_err) = tx_err {
            for m_inv in history.into_iter().rev() {
                if verbose {
                    println!("<= {}", m_inv);
                }
                if let Err(rollback_err) = m_inv.apply(None) {
                    println!("✗ rollback error");
                    let ext_rb_err = rollback_err.context("rollback failed");
                    return FsTransactionResult {
                        tx_result: Err(tx_err),
                        rb_result: Some(Err(ext_rb_err)),
                    };
                }
            }
            println!("✓ rollback succeeded, filesystem restored");
            return FsTransactionResult {
                tx_result: Err(tx_err),
                rb_result: Some(Ok(())),
            };
        }
        return FsTransactionResult {
            tx_result: Ok(()),
            rb_result: None,
        };
    }

    pub fn run_haphazard(self, verbose: bool) -> anyhow::Result<FsTransactionResult> {
        println!("Running haphazard transaction...");
        for m in self.mods.into_iter() {
            if verbose {
                println!("? {}", m);
            }
            match m.apply(None) {
                Err(err) => {
                    return Ok(FsTransactionResult {
                        tx_result: Err(err),
                        rb_result: None,
                    });
                }
                Ok(_) => {}
            }
        }
        return Ok(FsTransactionResult {
            tx_result: Ok(()),
            rb_result: None,
        });
    }

    pub fn run_atomic(self, verbose: bool) -> anyhow::Result<FsTransactionResult> {
        let tx_id = format!("{}", rand::thread_rng().gen::<u32>());
        println!("Running atomic transaction {:?}...", tx_id);
        let tx_backup_dir = PathBuf::from("backups").join(tx_id);
        std::fs::create_dir_all(&tx_backup_dir)
            .context(format!("could not create the backup directory"))?;
        let mut history = vec![];
        let mut tx_err = None;
        for m in self.mods.into_iter() {
            if verbose {
                println!("=> {}", m);
            }
            match m.apply(Some(&tx_backup_dir)) {
                Ok(m_inv) => {
                    history.push(m_inv);
                }
                Err(err) => {
                    println!("✗ atomic transaction error, trying to roll back");
                    tx_err = Some(err.context("transaction failed"));
                    break;
                }
            }
        }
        let tx_result = Self::rollback_tx(history, tx_err, verbose);
        // Remove the backups only if a fatal failure has not occurred.
        if tx_result.fatal_failure() {
            println!(
                "! backed up files remain at {:?}, good luck =)",
                tx_backup_dir
            );
        } else {
            _ = std::fs::remove_dir_all(&tx_backup_dir);
        }
        return Ok(tx_result);
    }
}
