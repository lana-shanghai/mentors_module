use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use scale_info::TypeInfo;

#[derive(Copy, Clone, Encode, Decode, Default, Debug, PartialEq, TypeInfo, MaxEncodedLen)]
pub struct VaultDetails<AccountId, Balance> {
	/// The mentor's AccountId
	pub mentor: AccountId,
	/// The student's AccountId
	pub student: AccountId,
	/// The Vault's id
	pub vault_id: u64,
	/// Amount locked
	pub locked_amount: Balance,
}

pub trait Vault {
	type AccountId;
	type Balance;

	fn account_id(vault_id: u64) -> Self::AccountId;

	// Create a new vault for the mentor and student pair.
	//fn create_vault(mentor: Self::AccountId, student: Self::AccountId) -> Result<(u64,
	// VaultDetails<Self::AccountId, Self::Balance>), DispatchError>;

	// Withdraw deposit from the vault.
	fn withdraw(from: &Self::AccountId, vault_id: u64) -> Result<Self::Balance, DispatchError>;
}
