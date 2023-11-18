use anyhow::Context;
use primitives::*;
use rand::Rng;
use std::path::PathBuf;

mod primitives;
mod tx_apply;
mod tx_builder;
mod tx_gen;
mod tx_processor;
mod tx_result;

pub use tx_apply::*;
pub use tx_builder::*;
pub use tx_gen::*;
pub use tx_processor::*;
pub use tx_result::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Transaction {
    id: String,
    name: String,
    backup_dir: PathBuf,
    primitives: Vec<FsPrimitive>,
}

impl Transaction {
    /// Constructs a concrete transaction that can be executed directly.
    fn generate(name: String, primitives: Vec<FsPrimitive>) -> anyhow::Result<Self> {
        // Create a random transaction id.
        let id = format!("{}-{}", name, rand::thread_rng().gen::<u32>());
        // Create a backup directory for the transaction.
        let backup_dir = PathBuf::from("transactions").join(&id);
        std::fs::create_dir_all(&backup_dir)
            .context(format!("could not create the backup directory"))?;
        let tx_file_path = backup_dir.join("tx");
        // Construct the concrete transaction.
        let concrete_tx = Transaction {
            id,
            backup_dir,
            name,
            primitives,
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
            "could not deserialize the transaction from {:?}",
            path
        ))
    }
}
