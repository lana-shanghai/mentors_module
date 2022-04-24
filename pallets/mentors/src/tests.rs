use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};

#[test]
fn new_mentor_can_register() {
	new_test_ext().execute_with(|| {
		assert_ok!(MentorsModule::register_as_mentor(Origin::signed(1u64).into()));
	});
}

