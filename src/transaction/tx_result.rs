use anyhow::Context;

use super::{Concrete, Transaction};

#[derive(Debug)]
pub enum TxResult {
    /// Returns a transaction result that denotes a successful execution.
    Success(Transaction<Concrete>),
    /// Returns a transaction result that denotes a failure during transaction execution with successful rollback.
    TxFailure(anyhow::Error),
    /// Returns a transaction result that denotes a failure during transaction execution with failed rollback.
    FatalFailure {
        tx_err: anyhow::Error,
        rb_err: anyhow::Error,
    },
}

impl TxResult {
    /// Returns `true` if the result denotes a successful transaction.
    pub fn is_success(&self) -> bool {
        matches!(self, &TxResult::Success(_))
    }

    /// Returns `true` if the result denotes a failed transaction and failed rollback.
    pub fn is_fatal_failure(&self) -> bool {
        matches!(
            self,
            &TxResult::FatalFailure {
                tx_err: _,
                rb_err: _
            }
        )
    }

    /// Consumes self and returns the included transaction result, discarding the rollback result.
    pub fn as_tx_result(self) -> anyhow::Result<Transaction<Concrete>> {
        match self {
            TxResult::Success(undo_tx) => Ok(undo_tx),
            TxResult::TxFailure(tx_err) => Err(tx_err).context("transaction failed"),
            TxResult::FatalFailure { tx_err, .. } => Err(tx_err).context("transaction failed"),
        }
    }

    /// Prints a transaction report on the standard output.
    pub fn display_report(&self) {
        match self {
            TxResult::TxFailure(tx_err) => {
                println!("-------");
                println!("Transaction error: {:?}", tx_err);
                println!("-------");
            }
            TxResult::FatalFailure { tx_err, rb_err } => {
                println!("-------");
                println!("Transaction error: {:?}", tx_err);
                println!("-------");
                println!("Rollback error: {:?}", rb_err);
                println!("-------");
            }
            TxResult::Success(_) => {}
        };
    }
}
