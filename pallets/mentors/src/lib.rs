#![cfg_attr(not(feature = "std"), no_std)]

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
	
	use frame_support::{
		dispatch::{Vec, DispatchError, DispatchResult,},
		traits::{
			Currency,
			tokens::ExistenceRequirement,
		},
		transactional,
		PalletId,
	};
	use frame_system::{
        offchain::{
            SubmitTransaction,
            SendTransactionTypes,
        },
	};
	use frame_support::sp_runtime::traits::AccountIdConversion;

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
	pub trait Config: frame_system::Config + timestamp::Config + SendTransactionTypes<Call<Self>> {
		type Event: From<Event<Self>> 
			+ IsType<<Self as frame_system::Config>::Event>
			+ Into<<Self as frame_system::Config>::Event>;

		type Call: From<Call<Self>> + Into<<Self as frame_system::Config>::Call>;

		type Currency: Currency<Self::AccountId>;

		type Balance: Default
			+ Parameter
			+ Encode
			+ Decode
			+ MaxEncodedLen
			+ Copy
			+ Ord
			+ From<u32>;
		
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
	pub(super) type NewMentors<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;
	
	// The storage item mapping the mentor AccountId to a Status enum, Verified if credentials have been verified.
	// Currently a mentor can verify credentials by submitting a correct response to the provided challenge. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_credentials)]
	pub(super) type MentorCredentials<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Status, ValueQuery>;

	// The storage item mapping the mentor AccountId to a vector of timeslot availabilities.
	// Currently the timeslot lengths are all taken equal to X. A mentor can provide up to 32 availabilities. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_availabilities)]
	pub(super) type MentorAvailabilities<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Availabilities<T>, ValueQuery>;

	// The storage item mapping the mentor AccountId to a vector of timeslot availabilities.
	// Currently the timeslot lengths are all taken equal to X. A mentor can provide up to 32 availabilities. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_pricing)]
	pub(super) type MentorPricing<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, OptionQuery>;

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
		T::Moment, 
		ValueQuery
	>;

	// The storage item mapping the mentor AccountId to a student AccountId to the vault id.
	// The vault id increments by one for each new session and corresponding vault. 
	#[pallet::storage]
	#[pallet::getter(fn past_sessions)]
	pub(super) type PastSessions<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat, 
		T::AccountId, 
		Blake2_128Concat, 
		T::AccountId, 
		BalanceOf<T>, 
		ValueQuery
	>;

	/// The storage item needed to identify past session ids and vault accounts.
	#[pallet::storage]
	#[pallet::getter(fn vault_count)]
	pub type VaultTracker<T: Config> = StorageValue<_, u64, ValueQuery>;

	// The storage item mapping the mentor AccountId to a student AccountId to the booked timeslot.
	// Currently it's possible to book only one session upfront. 
	#[pallet::storage]
	#[pallet::getter(fn mentor_student_vaults)]
	pub(super) type MentorStudentVaults<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat, 
		T::AccountId, 
		Blake2_128Concat, 
		T::AccountId, 
		VaultDetails<T::AccountId, BalanceOf<T>>, 
		OptionQuery
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emitted when the caller started the mentor registration process.
		NewMentorRegistered(T::AccountId),
		/// Emitted when the offchain worker sends the challenge to the mentor.
		MentorInVerificationProcess(T::AccountId),
		/// Emitted when a student make a deposit into the vault.
		DepositSuccessful(T::AccountId),
		/// Emitted when the mentor withdraws from the vault.
		WithdrawalSuccessful,
		/// Emitted when a student gets a refund from the vault.
		RefundSuccessful,
		/// Emitted when a mentor sets a session price.
		PriceSet,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Emitted if a mentor is already in the MentorCredentials storage.
		MentorAlreadyRegistered,
		/// Emitted if a student cancels less than 24 hours in advance.
		CancellationNotPossible,
		/// Emitted if a student tries to book a timeslot that is not available.
		TimeslotNotAvailable,
		/// Emitted when
		WithdrawalNotPermitted,
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

		/// Allows a mentor to set their session price. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn set_session_price(origin: OriginFor<T>, price: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			<MentorPricing<T>>::insert(who.clone(), price);
			Self::deposit_event(Event::PriceSet);
			Ok(())
		}

		/// Allows a mentor to provide an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn add_availability(origin: OriginFor<T>, timeslot: T::Moment) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let now = <timestamp::Pallet<T>>::get();
			let mut current_availabilities = <MentorAvailabilities<T>>::get(&who);
			if !current_availabilities.contains(&(now + timeslot)) {
				current_availabilities.try_push(now + timeslot);
				<MentorAvailabilities<T>>::insert(&who, current_availabilities)
			} else {}
			Ok(())
		}

		/// Allows a mentor to remove an open timeslot.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn remove_availability(origin: OriginFor<T>, timestamp: T::Moment) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let current_availabilities = <MentorAvailabilities<T>>::get(&who);
			let index = current_availabilities.iter().position(|&t| t == timestamp).unwrap(); // TODO fix unwrap call on None
			<MentorAvailabilities<T>>::try_mutate(&who, |current_availabilities| {
				current_availabilities.remove(index);
				Ok(())
			})
		}

		/// Allows a student to book a session with a mentor.
		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn book_session(origin: OriginFor<T>, mentor: T::AccountId, timestamp: T::Moment) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let current_availabilities = <MentorAvailabilities<T>>::get(&mentor);
			if current_availabilities.contains(&timestamp) {
				<UpcomingSessions<T>>::insert(&mentor, &who, timestamp);
				let _new_vault = Self::new_vault(mentor.clone(), who.clone()).map(|(vault_id, _)| vault_id);
				Self::make_deposit(&mentor, &who);
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
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn reject_session(origin: OriginFor<T>, student: T::AccountId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			<UpcomingSessions<T>>::remove(&who, &student);
			Ok(())
		}

		/// Allows a student to cancel a session 24 hours in advance. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn cancel_session(origin: OriginFor<T>, mentor: T::AccountId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let upcoming_timestamp = <UpcomingSessions<T>>::get(&mentor, &who);
			let now = <timestamp::Pallet<T>>::get();
			let cancellation_period = T::CancellationPeriod::get();
			if upcoming_timestamp - now > cancellation_period {
				<UpcomingSessions<T>>::remove(&mentor, &who);
				Ok(())
			} else {
				Err(Error::<T>::CancellationNotPossible)?
			}
		}

		/// The offchain worker uses this dispatchable to remove viewed mentors from storage,
		/// and to send them a challenge for verification. Sets Status to ChallengeSent. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(2))]
		pub fn process_new_mentor(origin: OriginFor<T>, mentor: T::AccountId) -> DispatchResult {
			let _who = ensure_none(origin)?;
			<NewMentors<T>>::remove(&mentor);
			Self::send_challenge();
			<MentorCredentials<T>>::insert(&mentor, Status::ChallengeSent);
			Self::deposit_event(Event::MentorInVerificationProcess(mentor));
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {

		/// The offchain worker tracks new mentor applications and sends out a challenge for verification.
		/// It also tracks upcoming sessions to move them to past sessions storage. 
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
            match SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into()) {
				Err(e) => log::info!("{:?}", e),
   				 _ => ()
			};
		}

		pub fn new_vault(mentor: T::AccountId, student: T::AccountId) -> Result<(u64, VaultDetails<T::AccountId, BalanceOf<T>>), DispatchError> {
			<VaultTracker<T>>::try_mutate(|id| {
				let new_vault_id = {
					*id += 1;
					*id
				};
				let vault_details = VaultDetails {
					mentor: mentor.clone(), 
					student: student.clone(),
					vault_id: new_vault_id, 
					locked_amount: 1000u32.into(),
				};
				<MentorStudentVaults<T>>::insert(mentor, student, vault_details.clone());
				Ok((new_vault_id, vault_details))
			})	
		}

		pub fn make_deposit(mentor: &T::AccountId, student: &T::AccountId) -> Result<BalanceOf<T>, DispatchError> {
			let vault_id = <VaultTracker<T>>::get();

			let vault_address = <Self as Vault>::account_id(vault_id);
			log::info!("VAULT ADDRESS: {:?}", vault_address);
			let price = <MentorPricing<T>>::get(&mentor).unwrap();

			T::Currency::transfer(
				student,
				&vault_address,
				price,
				ExistenceRequirement::KeepAlive,
			);
			Ok(price)
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

	impl<T: Config> Vault for Pallet<T> {
		type AccountId = T::AccountId;
		type Balance = BalanceOf<T>;

		fn account_id(vault_id: u64) -> Self::AccountId {
			T::PalletId::get().into_sub_account(vault_id)
		}

		// fn create_vault(mentor: Self::AccountId, student: Self::AccountId) -> Result<(u64, VaultDetails<Self::AccountId, Self::Balance>), DispatchError> {
		// 	Self::new_vault(mentor, student)
		// }

		// fn deposit(mentor: T::AccountId, student: T::AccountId, vault_id: Self::VaultId) -> Result<Self::Balance, DispatchError> {
		// 	Self::make_deposit(mentor, student)
		// }

		// fn withdraw_as_mentor(
		// 	mentor: &Self::AccountId,
		// 	amount: Self::Balance,
		// ) -> Result<Self::Balance, DispatchError> {
		// 	Self::refund_deposit(from, amount)
		// }

		// fn withdraw_as_student(
		// 	student: &Self::AccountId,
		// 	amount: Self::Balance,
		// ) -> Result<Self::Balance, DispatchError> {
		// 	Self::withdraw_deposit(from, amount)
		// }
	}

	impl Default for Status {
		fn default() -> Self { Status::New }
	}

	impl MaxEncodedLen for Status {
		fn max_encoded_len() -> usize { 256 }
	}
}
