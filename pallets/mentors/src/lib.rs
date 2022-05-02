// GNU GENERAL PUBLIC LICENSE
// Version 3, 29 June 2007

//  Copyright (C) 2007 Free Software Foundation, Inc. <http://fsf.org/>
//  Everyone is permitted to copy and distribute verbatim copies
//  of this license document, but changing it is not allowed.

//! # A pallet that serves as a booking platform for a service provider.
//!
//! ## Overview
//!
//! ### User Types
//!
//! * Mentor - A user that solves necessary challenges in order to prove their credentials as a
//!   mentor.
//! * Student - A user that can book sessions with mentors according to their needs.
//!
//! ### Mechanics
//!
//! #### Registering as a mentor
//!
//! A user can register as a mentor, which creates a new entry in the NewMentors storage.
//! Every block, the NewMentors storage is iterated over to see if anyone expressed interest.
//! They are sent a challenge and removed from NewMentors. An entry appears in MentorCredentials
//! and their status in the storage changes to ChallengeSent.
//!
//! #### Setting the session price
//!
//! The mentor should then set a price for their session. A session can't be booked without a price.
//! 10**12 units correspond to 1 unit deducted from the prefunded user account.
//!
//! #### Adding and removing available timeslots
//!
//! The mentor can now offer time availabilities by submitting timeslots (taken in milliseconds from
//! now). For example, submitting a timeslot 24*60*60*1000 = 86_400_000 would book a session 24
//! hours from now. The corresponding timestamp will appear in the MentorAvailabilities storage. If
//! something changes, the mentor can remove a timeslot by submitting the `remove_availability`
//! extrinsic.
//!
//! #### Booking a session
//!
//! The user can book a session with a mentor by picking a timestamp from MentorAvailabilities
//! storage and sending an extrinsic to book a session. The timestamp has to be available, the user
//! must have enough funds to pay for the session. When someone books a session, the vault ID
//! tracker updates incrementally. Each vault ID has a corresponding vault address, derived via
//! PalletId's `into_sub_account` method. The value equivalent to the price per one session is
//! transferred to the newly generated vault address.
//!
//! #### Cancelling a session
//!
//! A user can cancel a session they booked earlier if there are 24 hours until the booked timeslot.
//! The funds return to their account, and the transfer is made with
//! `ExistenceRequirement::AllowDeath`.
//!
//! #### Rejecting a session
//!
//! A mentor can reject a session if they are unable to attend. The funds return to the student's
//! address.
//!
//! #### On initialize
//!
//! Every block the UpcomingSessions storage is iterated over to check if the timestamp has passed.
//! If so, the funds are moved from the vault address to the mentor's address, and the session
//! information is moved to PastSessions storage.
//!
//! #### On finalize
//!
//! At the end of every block after every new mentor was sent a challenge, they are removed from 
//! the NewMentors storage to MentorCredentials, where their Status is set to ChallengeSent.


#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused_variables)]
#![allow(type_alias_bounds)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::sp_runtime::{
	traits::Zero,
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
};

use pallet_timestamp::{self as timestamp};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	use frame_support::{
		dispatch::{DispatchError, DispatchResult, Vec},
		sp_runtime::traits::AccountIdConversion,
		traits::{tokens::ExistenceRequirement, Currency},
		transactional, PalletId,
	};
	use frame_system::offchain::{SendTransactionTypes, SubmitTransaction};

	use vault_primitives::vault_manager::{Vault, VaultDetails};

	pub type Availabilities<T: Config> = BoundedVec<T::Moment, T::MaxLength>;
	pub type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
	pub enum Status {
		New,
		ChallengeSent,
		ChallengeResponseSent,
		Verified,
	}

	#[pallet::config]
	pub trait Config:
		frame_system::Config + timestamp::Config + SendTransactionTypes<Call<Self>>
	{
		type Event: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::Event>
			+ Into<<Self as frame_system::Config>::Event>;

		type Call: From<Call<Self>> + Into<<Self as frame_system::Config>::Call>;

		type Currency: Currency<Self::AccountId>;

		#[pallet::constant]
		type MaxLength: Get<u32>;

		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;

		#[pallet::constant]
		type CancellationPeriod: Get<Self::Moment>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	// The storage item with the new Mentors. The offchain worker removes them every block.
	#[pallet::storage]
	#[pallet::getter(fn new_mentors)]
	pub(super) type NewMentors<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

	// The storage item mapping the mentor AccountId to a Status enum, Verified if credentials have
	// been verified. Currently a mentor can verify credentials by submitting a correct response to
	// the provided challenge.
	#[pallet::storage]
	#[pallet::getter(fn mentor_credentials)]
	pub(super) type MentorCredentials<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, Status, ValueQuery>;

	// The storage item mapping the mentor AccountId to a vector of timeslot availabilities.
	// Currently the timeslot lengths are all taken equal to X. A mentor can provide up to 32
	// availabilities.
	#[pallet::storage]
	#[pallet::getter(fn mentor_availabilities)]
	pub(super) type MentorAvailabilities<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, Availabilities<T>, ValueQuery>;

	// The storage item mapping the mentor AccountId to a vector of timeslot availabilities.
	// Currently the timeslot lengths are all taken equal to X. A mentor can provide up to 32
	// availabilities.
	#[pallet::storage]
	#[pallet::getter(fn mentor_pricing)]
	pub(super) type MentorPricing<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, OptionQuery>;

	// The storage item mapping the mentor AccountId to a student AccountId to the booked timeslot.
	// Currently it's possible to book only one session upfront.
	#[pallet::storage]
	#[pallet::getter(fn upcoming_sessions)]
	pub(super) type UpcomingSessions<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::AccountId,
		(T::Moment, u64),
		ValueQuery,
	>;

	/// The storage item mapping the mentor AccountId to a student AccountId to the vault id.
	/// The vault id increments by one for each new session and corresponding vault.
	#[pallet::storage]
	#[pallet::getter(fn past_sessions)]
	pub(super) type PastSessions<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::AccountId,
		(T::Moment, u64),
		ValueQuery,
	>;

	/// The storage item needed to identify past session ids and vault accounts.
	#[pallet::storage]
	#[pallet::getter(fn vault_count)]
	pub type VaultTracker<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// The storage item mapping the vault id to the corresponding vault details.
	#[pallet::storage]
	#[pallet::getter(fn mentor_student_vaults)]
	pub(super) type MentorStudentVaults<T: Config> =
		StorageMap<_, Blake2_128Concat, u64, VaultDetails<T::AccountId, BalanceOf<T>>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emitted when the caller started the mentor registration process.
		NewMentorRegistered(T::AccountId),
		/// Emitted when the offchain worker sends the challenge to the mentor.
		MentorInVerificationProcess(T::AccountId),
		/// Emitted when a student make a deposit into the vault.
		DepositSuccessful,
		/// Emitted when the mentor withdraws from the vault.
		WithdrawalSuccessful,
		/// Emitted when a student gets a refund from the vault.
		RefundSuccessful,
		/// Emitted when a mentor sets a session price.
		PriceSet,
		/// Emitted when a vault is successfully created.
		VaultCreated(u64),
		/// Emitted when a transfer has been successfully made.
		TransferSuccessful(BalanceOf<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Emitted if a mentor is already in the MentorCredentials storage.
		MentorAlreadyRegistered,
		/// Emitted if a student cancels less than 24 hours in advance.
		CancellationNotPossible,
		/// Emitted when a student tries to book a timeslot that is not available.
		TimeslotNotAvailable,
		/// Emitted when a student tries to book a timeslot that is in the future.
		BookingMustBeInTheFuture,
		/// Emitted when a withdrawal does not fulfill necessary requirements.
		WithdrawalNotPermitted,
		/// Emitted when a student tries to book a session with a non-registered mentor.
		MentorNotRegistered,
		/// Emitted when a mentor's price has not been set.
		MentorDidNotSetPrice,
		/// Emitted when a balance transfer fails.
		TransferFailed,
		/// Emitted when a student tries to book a session when there is already a scheduled
		/// session.
		CannotBookTwoSessionsUpfront,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block_number: BlockNumberFor<T>) -> Weight {
			Self::process_past_sessions();
			0 // TODO: Return the non-negotiable weight consumed in the block.
		}

		fn offchain_worker(_block_number: BlockNumberFor<T>) {
			log::info!("Entering offchain worker...");
			Self::offchain_process();
		}

		fn on_finalize(_block_number: BlockNumberFor<T>) {
			Self::process_new_mentors();
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		/// Validate unsigned call to this module.
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::send_challenge { mentor } = call {
				Self::validate_transaction_parameters()
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Allows a new mentor to initiate the registration process.
		#[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1, 1))]
		pub fn register_as_mentor(origin: OriginFor<T>) -> DispatchResult {
			let mentor = ensure_signed(origin)?;
			ensure!(<MentorCredentials<T>>::get(&mentor) == Status::New, Error::<T>::MentorAlreadyRegistered);
			<NewMentors<T>>::insert(mentor.clone(), true);
			Self::deposit_event(Event::NewMentorRegistered(mentor));
			Ok(())
		}

		/// Allows a mentor to set their session price.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn set_session_price(origin: OriginFor<T>, price: BalanceOf<T>) -> DispatchResult {
			let mentor = ensure_signed(origin)?;
			<MentorPricing<T>>::insert(mentor.clone(), price);
			Self::deposit_event(Event::PriceSet);
			Ok(())
		}

		/// Allows a mentor to provide an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().reads_writes(2, 1))]
		pub fn add_availability(origin: OriginFor<T>, timeslot: T::Moment) -> DispatchResult {
			let mentor = ensure_signed(origin)?;
			let now = <timestamp::Pallet<T>>::get();
			let mut current_availabilities = <MentorAvailabilities<T>>::get(&mentor);
			if !current_availabilities.contains(&(now + timeslot)) {
				match current_availabilities.try_push(now + timeslot) {
					Err(e) => log::info!("{:?}", e),
					_ => (),
				};
				<MentorAvailabilities<T>>::insert(&mentor, current_availabilities)
			} else {
			}
			Ok(())
		}

		/// Allows a mentor to remove an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1, 1))]
		pub fn remove_availability(origin: OriginFor<T>, timestamp: T::Moment) -> DispatchResult {
			let mentor = ensure_signed(origin)?;
			let current_availabilities = <MentorAvailabilities<T>>::get(&mentor);
			let index = current_availabilities.iter().position(|&t| t == timestamp).unwrap(); // TODO fix unwrap call on None
			<MentorAvailabilities<T>>::try_mutate(&mentor, |current_availabilities| {
				current_availabilities.remove(index);
				Ok(())
			})
		}

		/// Allows a student to book a session with a mentor.
		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn book_session(
			origin: OriginFor<T>,
			mentor: T::AccountId,
			timestamp: T::Moment,
		) -> DispatchResult {
			let student = ensure_signed(origin)?;
			ensure!(<MentorCredentials<T>>::get(&mentor) != Status::New, Error::<T>::MentorNotRegistered);
			// TODO: add check that the mentor's status has been set to Verified.
			let now = <timestamp::Pallet<T>>::get();
			ensure!(
				<UpcomingSessions<T>>::get(&mentor, &student).0 == T::Moment::zero(),
				Error::<T>::CannotBookTwoSessionsUpfront
			);
			ensure!(timestamp > now, Error::<T>::BookingMustBeInTheFuture);
			let current_availabilities = <MentorAvailabilities<T>>::get(&mentor);
			if current_availabilities.contains(&timestamp) {
				let _new_vault =
					Self::new_vault(mentor.clone(), student.clone()).map(|(vault_id, _)| vault_id);
				<UpcomingSessions<T>>::insert(
					&mentor,
					&student,
					(timestamp, <VaultTracker<T>>::get()),
				);
				Self::make_deposit(&mentor, &student)?;
				Self::deposit_event(Event::DepositSuccessful);
				let index = current_availabilities.iter().position(|&t| t == timestamp).unwrap(); // TODO fix unwrap call on None
				<MentorAvailabilities<T>>::try_mutate(&mentor, |current_availabilities| {
					current_availabilities.remove(index);
					Ok(())
				})
			} else {
				Err(Error::<T>::TimeslotNotAvailable)?
			}
		}

		/// Allows a mentor to reject a session.
		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1, 1))]
		pub fn reject_session(origin: OriginFor<T>, student: T::AccountId) -> DispatchResult {
			let mentor = ensure_signed(origin)?;
			let vault_id = <UpcomingSessions<T>>::get(&mentor, &student).1;
			Self::withdraw_deposit(&student, vault_id)?;
			<UpcomingSessions<T>>::remove(&mentor, &student);
			Self::deposit_event(Event::RefundSuccessful);
			Ok(())
		}

		/// Allows a student to cancel a session 24 hours in advance.
		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1, 1))]
		pub fn cancel_session(origin: OriginFor<T>, mentor: T::AccountId) -> DispatchResult {
			let student = ensure_signed(origin)?;
			let upcoming_timestamp = <UpcomingSessions<T>>::get(&mentor, &student).0;
			let now = <timestamp::Pallet<T>>::get();
			let cancellation_period = T::CancellationPeriod::get();
			if upcoming_timestamp - now > cancellation_period {
				let vault_id = <UpcomingSessions<T>>::get(&mentor, &student).1;
				Self::withdraw_deposit(&student, vault_id)?;
				<UpcomingSessions<T>>::remove(&mentor, &student);
				Self::deposit_event(Event::RefundSuccessful);
				Ok(())
			} else {
				Err(Error::<T>::CancellationNotPossible)?
			}
		}

		// The offchain worker sends a challenge to a new mentor.
		#[pallet::weight(10_000 + T::DbWeight::get().reads(1))]
		pub fn send_challenge(origin: OriginFor<T>, mentor: T::AccountId) -> DispatchResult {
			ensure_none(origin)?;
			Ok(())
		}	
	}

	impl<T: Config> Pallet<T> {
		/// The offchain worker tracks new mentor applications and sends out a challenge for
		/// verification.
		fn offchain_process() {
			log::info!("Started offchain process...");

			let mut new_mentors = Vec::<T::AccountId>::new();

			for (mentor, val) in <NewMentors<T>>::iter() {
				log::info!("New mentor: {:?}", &mentor);
				match val {
					true => {
						new_mentors.push(mentor);
					},
					false => {
						log::info!("On false branch") // TODO: remove logging
					},
				}
			}

			for mentor in new_mentors {
				Self::offchain_unsigned_response_new_mentor(mentor);
			}
		}

		pub fn offchain_unsigned_response_new_mentor(mentor: T::AccountId) {
			let call: Call<T> = Call::send_challenge { mentor };
			SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
		}

		/// Move funds from a vault to a mentor's address and update storage accordingly.
		pub fn process_past_sessions() -> DispatchResult {
			for (mentor, student, session) in <UpcomingSessions<T>>::iter() {
				let now = <timestamp::Pallet<T>>::get();
				if session.0 < now {
					match Self::withdraw_deposit(&mentor, session.1) {
						Err(e) => return Err(e),
						Ok(amount) => {
							<UpcomingSessions<T>>::remove(&mentor, &student);
							<PastSessions<T>>::insert(&mentor, &student, session);
							()
						},
					};
				}
			}
			Ok(())
		}

		/// Remove new mentors from storage at the end of the block. OCW sends
		/// them a challenge for verification. Status is set to ChallengeSent.
		pub fn process_new_mentors() -> DispatchResult {
			for (mentor, _) in <NewMentors<T>>::iter() {
				<NewMentors<T>>::remove(&mentor);
				<MentorCredentials<T>>::insert(&mentor, Status::ChallengeSent);
				Self::deposit_event(Event::MentorInVerificationProcess(mentor));
			}
			Ok(())
		}

		/// Helper function to create a new vault from the new vault id.
		pub fn new_vault(
			mentor: T::AccountId,
			student: T::AccountId,
		) -> Result<(u64, VaultDetails<T::AccountId, BalanceOf<T>>), DispatchError> {
			<VaultTracker<T>>::try_mutate(|id| {
				let new_vault_id = {
					*id += 1;
					*id
				};
				let vault_details = VaultDetails {
					mentor: mentor.clone(),
					student: student.clone(),
					vault_id: new_vault_id,
					locked_amount: 0u32.into(),
				};
				<MentorStudentVaults<T>>::insert(new_vault_id, vault_details.clone());
				Self::deposit_event(Event::VaultCreated(new_vault_id));
				Ok((new_vault_id, vault_details))
			})
		}

		/// Helper function to move funds from a student to a vault.
		pub fn make_deposit(
			mentor: &T::AccountId,
			student: &T::AccountId,
		) -> Result<BalanceOf<T>, DispatchError> {
			let vault_id = <VaultTracker<T>>::get();
			let vault_address = <Self as Vault>::account_id(vault_id);
			ensure!(<MentorPricing<T>>::get(&mentor).is_some(), Error::<T>::MentorDidNotSetPrice);
			let price = <MentorPricing<T>>::get(&mentor).unwrap();

			T::Currency::transfer(student, &vault_address, price, ExistenceRequirement::KeepAlive)?;
			<MentorStudentVaults<T>>::try_mutate(vault_id, |vault_details| {
				vault_details.as_mut().unwrap().locked_amount += price;
				Self::deposit_event(Event::TransferSuccessful(price));
				Ok(price)
			})
		}

		/// Helper function to move funds from a vault to an account.
		pub fn withdraw_deposit(
			account: &T::AccountId,
			vault_id: u64,
		) -> Result<BalanceOf<T>, DispatchError> {
			let vault_address = <Self as Vault>::account_id(vault_id);
			let amount = <MentorStudentVaults<T>>::get(vault_id).unwrap().locked_amount;

			T::Currency::transfer(
				&vault_address,
				&account,
				amount,
				ExistenceRequirement::AllowDeath,
			)?;
			<MentorStudentVaults<T>>::try_mutate(vault_id, |vault_details| {
				vault_details.as_mut().unwrap().locked_amount -= amount;
				Self::deposit_event(Event::TransferSuccessful(amount));
				Ok(amount)
			})
		}

		pub fn validate_transaction_parameters() -> TransactionValidity {
			ValidTransaction::with_tag_prefix("OffchainWorker")
				.priority(T::UnsignedPriority::get())
				.longevity(5)
				.propagate(true)
				.build()
		}
	}

	impl<T: Config> Vault for Pallet<T> {
		type AccountId = T::AccountId;
		type Balance = BalanceOf<T>;

		fn account_id(vault_id: u64) -> Self::AccountId {
			T::PalletId::get().into_sub_account(vault_id)
		}

		fn withdraw(
			account: &Self::AccountId,
			vault_id: u64,
		) -> Result<Self::Balance, DispatchError> {
			Self::withdraw_deposit(account, vault_id)
		}
	}

	impl Default for Status {
		fn default() -> Self {
			Status::New
		}
	}

	impl MaxEncodedLen for Status {
		fn max_encoded_len() -> usize {
			256
		}
	}
}
