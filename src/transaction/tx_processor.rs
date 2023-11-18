use anyhow::Context;

use super::Transaction;

#[derive(Clone, Debug)]
pub struct TxProcessor {
    name: String,
    verbose: bool,
    processed: Vec<Transaction>,
}

impl TxProcessor {
    pub fn new<S: Into<String>>(name: S, verbose: bool) -> Self {
        Self {
            verbose,
            name: name.into(),
            processed: Default::default(),
        }
    }

    /// Runs the given transaction such that the failure of it won't affect the overall progress.
    pub fn run_optional(&mut self, tx: Transaction) -> anyhow::Result<()> {
        let tx_result = tx.run_atomic(self.verbose);
        if !tx_result.is_success() {
            tx_result.display_report();
        }
        if tx_result.is_fatal_failure() {
            panic!("Fatal failure: Filesystem could not be restored.");
        }
        let undo_tx = tx_result.as_tx_result()?;
        self.processed.push(undo_tx);
        Ok(())
    }

    /// Runs the given transaction such that the failure of it will cause all the previous transactions executed by this processor to be reversed.
    pub fn run_required(&mut self, tx: Transaction) -> anyhow::Result<()> {
        let run_res = self.run_optional(tx);
        if let Err(err) = run_res {
            println!("Rolling {} back due to error", self.name);
            self.rollback()?;
            Err(err)
        } else {
            Ok(())
        }
    }

    fn rollback(&mut self) -> anyhow::Result<()> {
        for prev_tx in self.processed.drain(..).rev() {
            prev_tx
                .run_haphazard(self.verbose)
                .context("could not undo the previous transaction")
                .expect("Fatal failure! Filesystem could not be restored.");
        }
        Ok(())
    }
}
