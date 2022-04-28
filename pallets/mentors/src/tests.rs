use crate::*;
use crate::{mock::*, Error, Event};
use vault_primitives::Vault;
use frame_support::{
	assert_noop, assert_ok,
};

fn events() -> Vec<mock::Event> {
	let evt = System::events().into_iter().map(|evt| evt.event).collect::<Vec<_>>();
	System::reset_events();
	evt
}

#[test]
fn new_mentor_can_register() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(2u64).into()));
	});
}

#[test]
fn mentor_status_changes_to_challenge_sent() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(2u64).into()));
		assert_ok!(MentorsModule::process_new_mentor(Origin::none(), 2u64));
		assert_eq!(MentorCredentials::<Test>::get(2u64), Status::ChallengeSent);
	});
}

#[test]
fn mentor_sets_price_correctly() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::set_session_price(Origin::signed(2u64).into(), 1200));
		assert_eq!(MentorPricing::<Test>::get(2u64), Some(1200));
	});
}

#[test]
fn mentor_can_submit_availability() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::add_availability(Origin::signed(2u64).into(), 86_400_000 + 100_000));
		assert_eq!(MentorAvailabilities::<Test>::get(2u64).len(), 1);
	});
}

#[test]
fn student_can_book_session() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(2u64).into()));
		assert_ok!(MentorsModule::process_new_mentor(Origin::none(), 2u64));
		assert_ok!(MentorsModule::set_session_price(Origin::signed(2u64).into(), 1200));
		assert_ok!(MentorsModule::add_availability(Origin::signed(2u64).into(), 86_400_000 + 100_000));
		assert_ok!(MentorsModule::book_session(Origin::signed(1u64).into(), 2u64, MentorAvailabilities::<Test>::get(2u64)[0]));
		assert_eq!(VaultTracker::<Test>::get(), 1);
	});
}

#[test]
fn student_cannot_book_unavailable_timeslot() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(2u64).into()));
		assert_ok!(MentorsModule::process_new_mentor(Origin::none(), 2u64));
		assert_ok!(MentorsModule::add_availability(Origin::signed(2u64).into(), 86_400_000 + 100_000));
		assert_noop!(MentorsModule::book_session(Origin::signed(1u64).into(), 2u64, MentorAvailabilities::<Test>::get(2u64)[0] + 1000), Error::<Test>::TimeslotNotAvailable);
	});
}

// #[test]
// fn booking_session_moves_correct_amount_to_vault() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }

// #[test]
// fn new_mentor_can_register() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }

// #[test]
// fn new_mentor_can_register() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }

// #[test]
// fn new_mentor_can_register() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }

// #[test]
// fn new_mentor_can_register() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }

// #[test]
// fn new_mentor_can_register() {
// 	new_test_ext().execute_with(|| {
// 		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
// 	});
// }
