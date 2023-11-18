use std::path::PathBuf;

use anyhow::Context;

use crate::transaction::TxBuilder;

use super::{FsPrimitive, Transaction, TxResult};

fn run_sequentially(
    mods: Vec<FsPrimitive>,
    mut inv_mods: Option<&mut Vec<FsPrimitive>>,
    backup_dir: Option<&PathBuf>,
    info_icon: Option<&'static str>,
) -> anyhow::Result<()> {
    for m in mods.into_iter() {
        if let Some(info_icon) = info_icon {
            println!(" {} {}", info_icon, m);
        }
        let m_inv = m.apply(backup_dir)?;
        if let Some(inv_mods) = &mut inv_mods {
            inv_mods.push(m_inv);
        }
    }
    inv_mods.map(|inv_mods| inv_mods.reverse());
    Ok(())
}

impl Transaction {
    /// Interprets the transaction as a list of primitives and applies them sequentially until an error occurs.
    pub fn run_haphazard(self, verbose: bool) -> anyhow::Result<()> {
        println!("Running filesystem modifications ({})", self.name);
        println!("Directory: {:?}", self.backup_dir);
        if let Err(err) = run_sequentially(
            self.primitives,
            None,
            None,
            if verbose { Some(".") } else { None },
        ) {
            println!(" ✗ Execution failed");
            Err(err)
        } else {
            println!(" ✓ Execution succeeded");
            Ok(())
        }
    }

    /// Runs the transaction in an atomic manner. This means if an error occurs, we try to rollback.
    pub fn run_atomic(self, verbose: bool) -> TxResult {
        println!("Running transaction ({})", self.name);
        // Run the transaction sequentially while keeping track of its inverse.
        let mut inv_mods = vec![];
        let run_res = run_sequentially(
            self.primitives,
            Some(&mut inv_mods),
            Some(&self.backup_dir),
            if verbose { Some("→") } else { None },
        )
        // Then try to generate the undo transaction from the inverted primitives.
        .and_then(|_| {
            // TODO: fix the unecessary clone
            let mut txb = TxBuilder::empty();
            // Clone is kind of unnecessary, but I want to make the compiler happy.
            inv_mods.clone().into_iter().for_each(|p| txb.push(p));
            let undo_tx = txb.build(format!("Undo{}", self.name))?;
            Ok(undo_tx)
        });
        match run_res {
            Ok(undo_tx) => {
                println!(" ✓ Transaction succeeded");
                TxResult::Success(undo_tx)
            }
            Err(tx_err) => {
                println!(" ✗ Transaction failed, trying to roll back");
                // Run the history (inverted) to rollback.
                if let Err(rb_err) =
                    run_sequentially(inv_mods, None, None, if verbose { Some("←") } else { None })
                {
                    println!(" ✗ Transaction rollback failed");
                    println!(
                        " ✗ Backed up files remain at {:?}, good luck =)",
                        self.backup_dir
                    );
                    TxResult::FatalFailure { tx_err, rb_err }
                } else {
                    println!(" ✓ Transaction rollback succeeded");
                    TxResult::TxFailure(tx_err)
                }
            }
        }
    }
}
