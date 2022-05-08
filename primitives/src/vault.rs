use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
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
}
