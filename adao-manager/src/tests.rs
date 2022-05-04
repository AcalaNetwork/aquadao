// This file is part of Acala.

// Copyright (C) 2022 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Tests for Aqua DAO manager module.

#![cfg(test)]

use super::*;
use mock::{Event, ACA, AUSD, DOT, *};

use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use module_support::dex::DEXManager;
use orml_traits::MultiCurrencyExtended;

fn run_to_block(n: BlockNumber) {
	let mut block = System::block_number();
	while block < n {
		System::set_block_number(block + 1);
		AquaDAO::on_initialize(block + 1);
		block += 1;
	}
}

// Sets AUSD/ADAO and AUSD/ACA for liquidity provision
fn set_test_strategies() {
	// Set strategies
	let strategy = Strategy {
		kind: StrategyKind::LiquidityProvisionAusdAdao,
		percent_per_trade: FixedU128::saturating_from_rational(1, 2),
		max_amount_per_trade: 1_000_000,
		min_amount_per_trade: -1_000_000,
	};
	let strategy2 = Strategy {
		kind: StrategyKind::LiquidityProvisionAusdOther(TokenSymbol::ACA),
		percent_per_trade: FixedU128::saturating_from_rational(1, 2),
		max_amount_per_trade: 1_000_000,
		min_amount_per_trade: -1_000_000,
	};
	assert_ok!(AquaDAO::set_strategies(
		Origin::signed(ALICE),
		vec![strategy, strategy2]
	));
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
		assert_eq!(
			TargetAllocationPercents::<Runtime>::get().get(&AUSD).unwrap(),
			&AllocationPercent {
				value: FixedU128::saturating_from_rational(5, 10),
				min: FixedU128::saturating_from_rational(9, 20),
				max: FixedU128::saturating_from_rational(11, 20)
			}
		);

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
fn set_strategies_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(AquaDAO::set_strategies(Origin::signed(BOB), vec![]), BadOrigin);
		assert_eq!(Strategies::<Runtime>::get(), vec![]);

		let strategy = Strategy {
			kind: StrategyKind::LiquidityProvisionAusdAdao,
			percent_per_trade: FixedU128::default(),
			max_amount_per_trade: 0,
			min_amount_per_trade: 0,
		};
		assert_ok!(AquaDAO::set_strategies(Origin::signed(ALICE), vec![strategy]));
	});
}

#[test]
fn test_current_allocations() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			ACA, &DAO, 1_000_000
		));

		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc)), (AUSD, Some(alloc))]
		));

		let curr_allocations = AquaDAO::current_allocations().unwrap();
		assert_eq!(curr_allocations.1, 2_000_000);
		assert_eq!(
			curr_allocations.0.get(&ACA).unwrap(),
			&CurrentAllocation {
				amount: 1_000_000,
				value: 1_000_000,
				percent: FixedU128::saturating_from_rational(1, 2)
			}
		);
	});
}

#[test]
fn test_allocation_diff() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			ACA, &DAO, 1_000_000
		));

		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, Some(alloc)), (AUSD, Some(alloc))]
		));

		// There is no difference as allocations are the same
		let diff = AquaDAO::allocation_diff().unwrap();
		assert_eq!(
			diff.get(&ACA).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(1, 2),
				target: FixedU128::saturating_from_rational(1, 2),
				diff: FixedI128::default(),
				range_diff: FixedI128::default(),
				diff_amount: 0
			}
		);

		// Adjust allocation to make difference in target and current allocation amounts
		let adjustment = AllocationAdjustment { value: -50, range: -5 };
		assert_ok!(AquaDAO::adjust_target_allocations(
			Origin::signed(ALICE),
			vec![(ACA, adjustment)]
		));

		// Now there is a difference in values
		let diff = AquaDAO::allocation_diff().unwrap();
		// slight loss of percision due to fixed integer, should not be major issue as system is always
		// rebalancing, can only be off by 1
		assert_eq!(
			diff.get(&ACA).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(1, 2),
				target: FixedU128::saturating_from_rational(1, 3),
				diff: FixedI128::from_inner(166666666666666667),
				range_diff: FixedI128::from_inner(133333333333333334),
				diff_amount: 333_334
			}
		);
		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(1, 2),
				target: FixedU128::saturating_from_rational(2, 3),
				diff: FixedI128::from_inner(-166666666666666666),
				range_diff: FixedI128::saturating_from_rational(-1, 10),
				diff_amount: -333_333
			}
		);
	});
}

#[test]
fn on_initialize_no_allocations() {
	ExtBuilder::default().build().execute_with(|| {
		// Set Balances for DaoAccount
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(DexModule::add_liquidity(
			Origin::signed(ALICE),
			AUSD,
			ACA,
			10000,
			10000,
			0,
			false
		));
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 1_000_000);

		// Nothing happens when no allocations are set and no strategies are set
		run_to_block(4);
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 1_000_000);

		set_test_strategies();
		// Check strategies are set
		assert_eq!(Strategies::<Runtime>::get().len(), 2);

		// Nothing happens when no allocations are set
		run_to_block(8);
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 1_000_000);
	});
}

#[test]
fn rebalance_ausd_other_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			ACA, &DAO, 1_000_000
		));

		let alloc = Allocation { value: 100, range: 10 };
		let alloc2 = Allocation { value: 50, range: 5 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(AUSD, Some(alloc)), (ACA, Some(alloc2)), (ACA_AUSD_LP, Some(alloc))]
		));

		let diff = AquaDAO::allocation_diff().unwrap();
		let strategy = Strategy {
			kind: StrategyKind::LiquidityProvisionAusdOther(TokenSymbol::ACA),
			percent_per_trade: FixedU128::saturating_from_rational(1, 2),
			max_amount_per_trade: 1_000_000,
			min_amount_per_trade: -1_000_000,
		};

		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(1, 2),
				target: FixedU128::saturating_from_rational(2, 5),
				diff: FixedI128::saturating_from_rational(1, 10),
				range_diff: FixedI128::saturating_from_rational(6, 100),
				diff_amount: 200_000
			}
		);
		assert_ok!(AquaDAO::rebalance(&strategy, diff.clone()));

		assert_eq!(Currencies::free_balance(AUSD, &DAO), 900_000);
		let diff = AquaDAO::allocation_diff().unwrap();
		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(9, 20),
				target: FixedU128::saturating_from_rational(2, 5),
				diff: FixedI128::saturating_from_rational(1, 20),
				range_diff: FixedI128::saturating_from_rational(1, 100),
				diff_amount: 100_000
			}
		);

		assert_ok!(AquaDAO::rebalance(&strategy, diff.clone()));

		// will recursively rebalance 50%
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 850_000);
		let diff = AquaDAO::allocation_diff().unwrap();
		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(17, 40),
				target: FixedU128::saturating_from_rational(2, 5),
				diff: FixedI128::saturating_from_rational(1, 40),
				range_diff: FixedI128::saturating_from_rational(0, 1),
				diff_amount: 50_000
			}
		);
	});
}

#[test]
fn rebalance_ausd_adao_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));

		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![(AUSD, Some(alloc)), (ADAO_AUSD_LP, Some(alloc))]
		));

		let diff = AquaDAO::allocation_diff().unwrap();
		let strategy = Strategy {
			kind: StrategyKind::LiquidityProvisionAusdAdao,
			percent_per_trade: FixedU128::saturating_from_rational(1, 2),
			max_amount_per_trade: 1_000_000,
			min_amount_per_trade: -1_000_000,
		};

		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(1, 1),
				target: FixedU128::saturating_from_rational(1, 2),
				diff: FixedI128::saturating_from_rational(1, 2),
				range_diff: FixedI128::saturating_from_rational(45, 100),
				diff_amount: 500_000
			}
		);
		assert_ok!(AquaDAO::rebalance(&strategy, diff.clone()));

		let diff = AquaDAO::allocation_diff().unwrap();
		assert_eq!(
			diff.get(&AUSD).unwrap(),
			&AllocationDiff {
				current: FixedU128::saturating_from_rational(7, 9),
				target: FixedU128::saturating_from_rational(1, 2),
				diff: FixedI128::saturating_from_rational(25, 90),
				range_diff: FixedI128::saturating_from_rational(41, 180),
				diff_amount: 312_500
			}
		);

		assert_ok!(AquaDAO::rebalance(&strategy, diff.clone()));
		// LP token deposited into dao account
		assert_eq!(Currencies::free_balance(ADAO_AUSD_LP, &DAO), 406_250);
	});
}

#[test]
fn alternates_strategies_correctly() {
	ExtBuilder::default().build().execute_with(|| {
		// Set Balances for DaoAccount
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			ACA, &DAO, 1_000_000
		));
		set_test_strategies();

		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![
				(AUSD, Some(alloc)),
				(ACA, Some(alloc)),
				(ACA_AUSD_LP, Some(alloc)),
				(ADAO_AUSD_LP, Some(alloc))
			]
		));
		run_to_block(2);

		// Nothing happens as offset is 1 so only will rebalance on odd blocks
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 1_000_000);
		assert_eq!(Currencies::free_balance(ACA, &DAO), 1_000_000);
		run_to_block(3);

		// rebalance with ausd and other token (ACA in this case)
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 875_000);
		assert_eq!(Currencies::free_balance(ACA, &DAO), 875_000);
		run_to_block(5);

		// rebalance with ausd and adao
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 750_000);
		assert_eq!(Currencies::free_balance(ACA, &DAO), 875_000);
	});
}

#[test]
fn allocate_with_no_funds() {
	ExtBuilder::default().build().execute_with(|| {
		set_test_strategies();

		let alloc = Allocation { value: 100, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![
				(AUSD, Some(alloc)),
				(ACA, Some(alloc)),
				(ACA_AUSD_LP, Some(alloc)),
				(ADAO_AUSD_LP, Some(alloc))
			]
		));
		System::reset_events();

		run_to_block(5);

		// rebalance will error out and no liquidity is added to pools
		assert_eq!(System::events(), vec![]);
		assert_eq!(
			DexModule::get_liquidity_pool(
				CurrencyId::Token(TokenSymbol::ADAO),
				CurrencyId::Token(TokenSymbol::AUSD)
			),
			(0, 0)
		);
		assert_eq!(
			DexModule::get_liquidity_pool(
				CurrencyId::Token(TokenSymbol::ACA),
				CurrencyId::Token(TokenSymbol::AUSD)
			),
			(0, 0)
		);
	});
}

#[test]
fn zero_amount_allocations_test() {
	ExtBuilder::default().build().execute_with(|| {
		set_test_strategies();

		let alloc = Allocation { value: 0, range: 10 };
		let alloc2 = Allocation { value: 1, range: 10 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![
				(AUSD, Some(alloc)),
				(ACA, Some(alloc)),
				(DOT, Some(alloc2)),
				(ACA_AUSD_LP, Some(alloc)),
				(ADAO_AUSD_LP, Some(alloc))
			]
		));

		System::reset_events();
		run_to_block(5);

		// rebalance will error out and no liquidity is added to pools
		assert_eq!(System::events(), vec![]);
		assert_eq!(
			DexModule::get_liquidity_pool(
				CurrencyId::Token(TokenSymbol::ADAO),
				CurrencyId::Token(TokenSymbol::AUSD)
			),
			(0, 0)
		);
		assert_eq!(
			DexModule::get_liquidity_pool(
				CurrencyId::Token(TokenSymbol::ACA),
				CurrencyId::Token(TokenSymbol::AUSD)
			),
			(0, 0)
		)
	});
}

#[test]
fn on_initialize_max_greater_than_one() {
	ExtBuilder::default().build().execute_with(|| {
		// Set Balances for DaoAccount
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			AUSD, &DAO, 1_000_000
		));
		assert_ok!(<Currencies as MultiCurrencyExtended<AccountId>>::update_balance(
			ACA, &DAO, 1_000_000
		));
		set_test_strategies();

		let alloc = Allocation { value: 100, range: 200 };
		assert_ok!(AquaDAO::set_target_allocations(
			Origin::signed(ALICE),
			vec![
				(ACA, Some(alloc)),
				(AUSD, Some(alloc)),
				(ACA_AUSD_LP, Some(alloc)),
				(ADAO_AUSD_LP, Some(alloc))
			]
		));

		// Nothing happens due to range being larger than value in allocation
		run_to_block(5);
		assert_eq!(Currencies::free_balance(AUSD, &DAO), 1_000_000);
		assert_eq!(Currencies::free_balance(ACA, &DAO), 1_000_000);
	});
}
