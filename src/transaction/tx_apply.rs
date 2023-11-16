use std::path::PathBuf;

use anyhow::Context;
use rand::Rng;

use super::{FsPrimitive, FsTransaction, TxResult};

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
    Ok(())
}

impl FsTransaction {
    /// Interprets the transaction as a list of primitives and applies them sequentially until an error occurs.
    pub fn run_haphazard(self, verbose: bool) -> anyhow::Result<()> {
        println!(
            "Running filesystem modifications ({})",
            self.name.unwrap_or(String::from("nameless"))
        );
        run_sequentially(
            self.mods,
            None,
            None,
            if verbose { Some(".") } else { None },
        )?;
        println!(" ✓ Execution succeeded");
        return Ok(());
    }

    /// Runs the transaction in an atomic manner. This means if an error occurs, we try to rollback.
    pub fn run_atomic(self, verbose: bool) -> anyhow::Result<TxResult> {
        println!(
            "Running transaction ({})",
            self.name.as_ref().unwrap_or(&String::from("nameless")),
        );
        // Create a random transaction id.
        let tx_id = format!("{}", rand::thread_rng().gen::<u32>());
        // Create a backup directory for the transaction.
        let tx_backup_dir = PathBuf::from("backups").join(tx_id);
        std::fs::create_dir_all(&tx_backup_dir)
            .context(format!("could not create the backup directory"))?;
        // Run the transaction atomically and get the result.
        let res = {
            // Apply the primitives and keep track of their inverses.
            let mut inv_mods = vec![];
            // Run the included primitives sequentially.
            if let Err(tx_err) = run_sequentially(
                self.mods,
                Some(&mut inv_mods),
                Some(&tx_backup_dir),
                if verbose { Some("→") } else { None },
            ) {
                println!(" ✗ Transaction error, trying to roll back");
                inv_mods.reverse();
                // Run the history (inverted) to rollback.
                if let Err(rb_err) =
                    run_sequentially(inv_mods, None, None, if verbose { Some("←") } else { None })
                {
                    println!(" ✗ Rollback failed");
                    TxResult::rb_fail(tx_err, rb_err)
                } else {
                    println!(" ✓ Rollback succeeded");
                    TxResult::rb_success(tx_err)
                }
            } else {
                println!(" ✓ Transaction succeeded");
                inv_mods.reverse();
                TxResult::success(FsTransaction {
                    name: self.name.map(|old_name| format!("undo {}", old_name)),
                    mods: inv_mods,
                })
            }
        };
        if res.is_fatal_failure() {
            println!(
                " ! Backed up files remain at {:?}, good luck =)",
                tx_backup_dir
            );
        }
        return Ok(res);
    }
}
