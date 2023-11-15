mod primitives;
mod tx_apply;
pub mod tx_gen;
mod tx_result;

pub use primitives::*;
pub use tx_apply::*;
pub use tx_result::*;

#[derive(Clone, Debug)]
pub struct FsTransaction {
    pub(super) name: Option<String>,
    pub(super) mods: Vec<FsPrimitive>,
}

impl FsTransaction {
    pub fn empty() -> Self {
        Self {
            name: None,
            mods: Default::default(),
        }
    }

    pub fn push(&mut self, fs_mod: FsPrimitive) {
        self.mods.push(fs_mod);
    }

    pub fn append(&mut self, other: FsTransaction) {
        self.mods.extend(other.mods)
    }

    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }
}
