use std::path::PathBuf;

use anyhow::Context;
use rand::Rng;

use super::{FsPrimitive, FsTransaction, FsTransactionResult};

impl FsTransaction {
    fn rollback_tx(
        history: Vec<FsPrimitive>,
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
                    return FsTransactionResult::rb_fail(tx_err, ext_rb_err);
                }
            }
            println!("✓ rollback succeeded, filesystem restored");
            return FsTransactionResult::rb_success(tx_err);
        }
        return FsTransactionResult::success();
    }

    /// Interprets the transaction as a list of primitives and applies them sequentially until an error occurs.
    pub fn run_haphazard(self, verbose: bool) -> anyhow::Result<FsTransactionResult> {
        println!(
            "Running filesystem modifications ({})",
            self.name.unwrap_or(String::from("nameless"))
        );
        for m in self.mods.into_iter() {
            if verbose {
                println!("? {}", m);
            }
            match m.apply(None) {
                Err(err) => {
                    println!("✗ modification error");
                    return Ok(FsTransactionResult::tx_fail_only(err));
                }
                Ok(_) => {}
            }
        }
        println!("✓ transaction succeeded");
        return Ok(FsTransactionResult::success());
    }

    /// Runs the transaction in an atomic manner. This means if an error occurs, we try to rollback.
    pub fn run_atomic(self, verbose: bool) -> anyhow::Result<FsTransactionResult> {
        // Create a random transaction id.
        let tx_id = format!("{}", rand::thread_rng().gen::<u32>());
        println!(
            "Running atomic transaction ({})",
            self.name.as_ref().unwrap_or(&String::from("nameless")),
        );
        // Create a backup directory for the transaction.
        let tx_backup_dir = PathBuf::from("backups").join(tx_id);
        std::fs::create_dir_all(&tx_backup_dir)
            .context(format!("could not create the backup directory"))?;
        // Apply the primitives and keep track of their inverses.
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
                    println!("✗ transaction error, trying to roll back");
                    tx_err = Some(err.context(format!(
                        "{} failed",
                        self.name.as_ref().unwrap_or(&String::from("transaction"))
                    )));
                    break;
                }
            }
        }
        // Try to rollback (if required) and acquire the final transaction result.
        let res = Self::rollback_tx(history, tx_err, verbose);
        // Remove the backups only if a fatal failure has not occurred.
        if res.is_fatal_failure() {
            println!(
                "! backed up files remain at {:?}, good luck =)",
                tx_backup_dir
            );
        } else {
            _ = std::fs::remove_dir_all(&tx_backup_dir);
        }
        if res.is_success() {
            println!("✓ transaction succeeded");
        }
        return Ok(res);
    }
}
