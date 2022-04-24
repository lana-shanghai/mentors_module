#![cfg_attr(not(feature = "std"), no_std)]

mod vault;
pub use vault::{Vault, VaultDetails};

pub mod vault_manager {
	pub use super::*;
}
