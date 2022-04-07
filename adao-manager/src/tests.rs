//! Tests for Aqua DAO manager module.

#![cfg(test)]

use super::*;
use mock::{Event, ACA, AUSD, *};

use frame_support::{assert_noop, assert_ok, error::BadOrigin};

fn run_to_block(n: BlockNumber) {
	System::set_block_number(n);
	AquaDAO::on_initialize(n);
}

#[test]
fn set_target_allocations_fails() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(AquaDAO::set_target_allocations(Origin::signed(BOB), vec![]), BadOrigin);
		assert_noop!(
			AquaDAO::set_target_allocations(Origin::signed(ALICE), vec![]),
			Error::<Runtime>::ZeroTargetAllocation
		);
	});
}

#[test]
fn set_target_allocations_works() {
	ExtBuilder::default().build().execute_with(|| {
		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc)), (AUSD, Some(alloc))]
		));
		System::assert_last_event(Event::AquaDAO(crate::Event::TargetAllocationSet {
			currency_id: AUSD,
			allocation: alloc,
		}));

		let alloc2 = Allocation { value: 50, range: 5 };
		// Will overwrite existing allocation
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc2))]
		));
		System::assert_last_event(Event::AquaDAO(crate::Event::TargetAllocationSet {
			currency_id: ACA,
			allocation: alloc2,
		}));

		// State is correct for target allocations
		assert_eq!(TargetAllocations::<Runtime>::get().get(&ACA).unwrap(), &alloc2);
		assert_eq!(TargetAllocations::<Runtime>::get().get(&AUSD).unwrap(), &alloc);

		// Totally remove an allocation of a token
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, None)]
		));
		System::assert_last_event(Event::AquaDAO(crate::Event::TargetAllocationRemoved {
			currency_id: ACA,
		}));
		// Percents are correct
		assert_eq!(
			TargetAllocationPercents::<Runtime>::get().get(&AUSD).unwrap(),
			&AllocationPercent {
				value: FixedU128::saturating_from_rational(1, 1),
				min: FixedU128::saturating_from_rational(9, 10),
				max: FixedU128::saturating_from_rational(11, 10)
			}
		);
	});
}

#[test]
fn adjust_target_allocations_fails() {
	ExtBuilder::default().build().execute_with(|| {
		// Errors out correctly
		assert_noop!(
			AquaDAO::adjust_target_allocations(Origin::signed(BOB), vec![]),
			BadOrigin
		);
		assert_noop!(
			AquaDAO::adjust_target_allocations(Origin::signed(ALICE), vec![]),
			Error::<Runtime>::ZeroTargetAllocation
		);
		assert_noop!(
			AquaDAO::adjust_target_allocations(
				Origin::signed(ALICE),
				vec![(ACA, AllocationAdjustment { value: 10, range: 5 })]
			),
			Error::<Runtime>::TargetAllocationNotFound
		);
	});
}

#[test]
fn adjust_target_allocations_works() {
	ExtBuilder::default().build().execute_with(|| {
		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc)), (AUSD, Some(alloc))]
		));
		assert_eq!(
			TargetAllocationPercents::<Runtime>::get().get(&ACA).unwrap(),
			&AllocationPercent {
				value: FixedU128::saturating_from_rational(1, 2),
				min: FixedU128::saturating_from_rational(45, 100),
				max: FixedU128::saturating_from_rational(55, 100)
			}
		);

		let adjustment = AllocationAdjustment { value: -50, range: -5 };
		assert_ok!(AquaDAO::adjust_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, adjustment)]
		));
		System::assert_last_event(Event::AquaDAO(crate::Event::TargetAllocationAdjusted {
			currency_id: ACA,
			adjustment,
		}));

		// Target allocation is adjusted
		assert_eq!(
			TargetAllocations::<Runtime>::get().get(&ACA).unwrap(),
			&Allocation { value: 50, range: 5 }
		);
		assert_eq!(TargetAllocations::<Runtime>::get().get(&AUSD).unwrap(), &alloc);
		assert_eq!(
			TargetAllocationPercents::<Runtime>::get().get(&ACA).unwrap(),
			&AllocationPercent {
				value: FixedU128::saturating_from_rational(1, 3),
				min: FixedU128::saturating_from_rational(3, 10),
				max: FixedU128::saturating_from_rational(11, 30)
			}
		);
	});
}

#[test]
fn on_initialize_max_greater_than_one() {
	ExtBuilder::default().build().execute_with(|| {
		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc))]
		));

		run_to_block(2);
	});
}
