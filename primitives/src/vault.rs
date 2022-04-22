use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use codec::{Decode, Encode};
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
	//fn create_vault(mentor: Self::AccountId, student: Self::AccountId) -> Result<(u64, VaultDetails<Self::AccountId, Self::Balance>), DispatchError>;

	// Deposit into the vault.
	// fn deposit(
	// 	mentor: Self::AccountId,
    //     student: Self::AccountId,
	// 	vault_id: Self::VaultId,
	// ) -> Result<Self::Balance, DispatchError>;

	// Withdraw collateral from the vault.
	// fn withdraw_as_mentor(
	// 	from: &Self::AccountId,
	// 	amount: Self::Balance,
	// ) -> Result<Self::Balance, DispatchError>;

    // Withdraw collateral from the vault.
	// fn withdraw_as_student(
	// 	from: &Self::AccountId,
	// 	amount: Self::Balance,
	// ) -> Result<Self::Balance, DispatchError>;
}