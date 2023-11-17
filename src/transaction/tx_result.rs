use anyhow::Context;

use super::{Concrete, Transaction};

#[derive(Debug)]
pub struct TxResult {
    tx_result: anyhow::Result<Transaction<Concrete>>,
    rb_result: Option<anyhow::Result<()>>,
}

impl TxResult {
    /// Returns a transaction result that denotes a successful execution.
    pub fn success(tx_inv: Transaction<Concrete>) -> Self {
        Self {
            tx_result: Ok(tx_inv),
            rb_result: None,
        }
    }

    /// Returns a transaction result that denotes a failure during transaction execution with no rollback.
    pub fn tx_fail_only(tx_err: anyhow::Error) -> Self {
        Self {
            tx_result: Err(tx_err),
            rb_result: None,
        }
    }

    /// Returns a transaction result that denotes a failure during transaction execution with successful rollback.
    pub fn rb_success(tx_err: anyhow::Error) -> Self {
        Self {
            tx_result: Err(tx_err),
            rb_result: Some(Ok(())),
        }
    }

    /// Returns a transaction result that denotes a failure during transaction execution with failed rollback.
    pub fn rb_fail(tx_err: anyhow::Error, rb_err: anyhow::Error) -> Self {
        Self {
            tx_result: Err(tx_err),
            rb_result: Some(Err(rb_err)),
        }
    }

    /// Returns `true` if the result denotes a successful transaction.
    pub fn is_success(&self) -> bool {
        matches!(self.tx_result, Ok(_))
    }

    /// Returns `true` if the result denotes a failed transaction and failed rollback.
    pub fn is_fatal_failure(&self) -> bool {
        matches!(self.rb_result, Some(Err(_)))
    }

    /// Consumes self and returns the included transaction result, discarding the rollback result.
    pub fn as_tx_result(self) -> anyhow::Result<Transaction<Concrete>> {
        self.tx_result.context("transaction failed")
    }

    /// Prints a transaction report on the standard output.
    pub fn display_report(&self) {
        if let Err(tx_err) = &self.tx_result {
            println!("-------");
            println!("Transaction error: {:?}", tx_err);
            match &self.rb_result {
                Some(Err(rb_err)) => {
                    println!("-------");
                    println!("Rollback error: {:?}", rb_err);
                }
                _ => (),
            }
            println!("-------");
            return;
        }
    }
}
