#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::sp_runtime::transaction_validity::{
	InvalidTransaction, TransactionValidity, ValidTransaction,
};

use pallet_timestamp::{self as timestamp};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	
	use frame_support::dispatch::Vec;
	use frame_system::{
        offchain::{
            SubmitTransaction,
            SendTransactionTypes,
        },
	};

	#[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
	pub enum Status {
		New,
		ChallengeSent,
		ChallengeResponseSent,
		Verified,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config + timestamp::Config + SendTransactionTypes<Call<Self>> {
		type Event: From<Event<Self>> 
			+ IsType<<Self as frame_system::Config>::Event>
			+ Into<<Self as frame_system::Config>::Event>;

		type Call: From<Call<Self>> + Into<<Self as frame_system::Config>::Call>;
		
		#[pallet::constant]
		type MaxLength: Get<u32>;

		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	pub type CancellationPeriod<T: Config> = T::Moment;
	pub type Availabilities<T: Config> = BoundedVec<T::Moment, T::MaxLength>;

	// The storage item with the new Mentors. The offchain worker removes them every block.
	#[pallet::storage]
	#[pallet::getter(fn new_mentors)]
	pub(super) type NewMentors<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;
	
	// The storage item mapping the mentor AccountId to a boolean value, true if credentials have been verified.
	// Currently a Mentor can verify credentials by submitting a correct response to the provided challenge. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_credentials)]
	pub(super) type MentorCredentials<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Status, ValueQuery>;

	// The storage item mapping the mentor AccountId to a vector of timeslot availabilities.
	// Currently the timeslot lengths are all taken equal to X. A mentor can provide up to 32 availabilities. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_availabilities)]
	pub(super) type MentorAvailabilities<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Availabilities<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emitted when the caller started
		/// the mentor registration process.
		NewMentorRegistered(T::AccountId),
		/// Emitted when the offchain worker
		/// sends the challenge to the mentor.
		MentorInVerificationProcess(T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// If a Mentor is already in the MentorCredentials storage, they cannot register again.
		MentorAlreadyRegistered,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(_block_number: T::BlockNumber) {
			log::info!("Entering offchain worker...");
			Self::offchain_process();
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		/// Validate unsigned call to this module.
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::process_new_mentor { mentor } = call {
				Self::validate_transaction_parameters()
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Allows a new mentor to initiate the registration process. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn register_as_mentor(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			<NewMentors<T>>::insert(who.clone(), true);
			log::info!("Who {:?}", who.clone());
			Self::deposit_event(Event::NewMentorRegistered(who));
			Ok(())
		}

		/// Allows a mentor to provide an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn add_availability(origin: OriginFor<T>, timeslot: T::Moment) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let current_availabilities = <MentorAvailabilities<T>>::get(&who);
			if current_availabilities.contains(&timeslot) {
				let updated_availabilities = current_availabilities.push(timeslot);
				<MentorAvailabilities<T>>::insert(&who, updated_availabilities)
			} else {}
			Ok(())
		}

		/// Allows a mentor to remove an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn remove_availability(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Ok(())
		}

		/// Allows a student to book a session with a mentor.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn book_session(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Ok(())
		}

		/// Allows a mentor to reject a session. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn reject_session(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Ok(())
		}

		/// Allows a student to cancel a session 24 hours in advance. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn cancel_session(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Ok(())
		}


		#[pallet::weight(10_000 + T::DbWeight::get().writes(2))]
		pub fn process_new_mentor(origin: OriginFor<T>, mentor: T::AccountId) -> DispatchResult {
			let who = ensure_none(origin)?;
			<NewMentors<T>>::remove(&mentor);
			Self::send_challenge();
			<MentorCredentials<T>>::insert(&mentor, Status::ChallengeSent);
			Self::deposit_event(Event::MentorInVerificationProcess(mentor));
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {

		fn offchain_process() {
			log::info!("Started offchain process...");

			let mut new_mentors = Vec::<T::AccountId>::new();

			for (mentor, val) in <NewMentors<T>>::iter() {
				log::info!("New mentor: {:?}", &mentor);
				match val {
					true => {
						new_mentors.push(mentor);
					},
					false => {log::info!("On false branch")},
				}
			}

			for mentor in new_mentors {
				Self::offchain_unsigned_response_new_mentor(mentor);
			}
		}

		pub fn offchain_unsigned_response_new_mentor(mentor: T::AccountId) {
			let call: Call<T> = Call::process_new_mentor{ mentor: mentor };
            SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
		}

		pub fn validate_transaction_parameters() -> TransactionValidity {
            ValidTransaction::with_tag_prefix("OffchainWorker")
                .priority(T::UnsignedPriority::get())
                .longevity(5)
                .propagate(true)
                .build()
        }

		// The offchain worker sends a challenge to a new mentor. 
		pub fn send_challenge() {}
	}

	impl Default for Status {
		fn default() -> Self { Status::New }
	}

	impl MaxEncodedLen for Status {
		fn max_encoded_len() -> usize { 256 }
	}
}