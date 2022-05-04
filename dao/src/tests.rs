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

//! Unit tests for Aqua DAO module.

#![cfg(test)]

use super::*;
use mock::{Event, *};

use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_runtime::traits::BadOrigin;

fn dollar(currency_id: CurrencyId) -> Balance {
	10u128.saturating_pow(currency_id.decimals().expect("Not support Non-Token decimals").into())
}

const UNITS: Balance = 1_000_000;

fn create_default_subscription() -> DispatchResult {
	AquaDao::create_subscription(
		RawOrigin::Root.into(),
		AUSD_CURRENCY,
		1_000,
		dollar(ADAO_CURRENCY) * 10,
		Ratio::saturating_from_rational(1, 10),
		dollar(CurrencyId::Token(ADAO)) * UNITS,
		Discount {
			max: DiscountRate::saturating_from_rational(2, 10),
			interval: 1,
			inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
			dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
		},
	)
}

#[test]
fn create_subscription_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		assert_ok!(create_default_subscription());
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionCreated {
			id: 0,
			subscription: Subscription {
				currency_id: AUSD_CURRENCY,
				vesting_period: 1_000,
				min_amount: dollar(ADAO_CURRENCY) * 10,
				min_ratio: Ratio::saturating_from_rational(1, 10),
				amount: dollar(CurrencyId::Token(ADAO)) * UNITS,
				discount: Discount {
					max: DiscountRate::saturating_from_rational(2, 10),
					interval: 1,
					inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
					dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
				},
				state: SubscriptionState {
					total_sold: Zero::zero(),
					last_sold_at: 1,
					last_discount: Zero::zero(),
				},
			},
		}));
		assert_eq!(AquaDao::subscription_index(), 1);
	});
}

#[test]
fn create_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			AquaDao::create_subscription(
				RawOrigin::Signed(ALICE).into(),
				AUSD_CURRENCY,
				1_000,
				dollar(ADAO_CURRENCY) * 10,
				Ratio::saturating_from_rational(1, 10),
				dollar(CurrencyId::Token(ADAO)) * UNITS,
				Discount {
					max: DiscountRate::saturating_from_rational(2, 10),
					interval: 1,
					inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
					dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
				},
			),
			BadOrigin
		);
	});
}

#[test]
pub fn update_subscription_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(create_default_subscription());

		let new_discount = Discount {
			max: DiscountRate::one(),
			interval: 1,
			inc_on_idle: DiscountRate::one(),
			dec_per_unit: DiscountRate::one(),
		};
		assert_ok!(AquaDao::update_subscription(
			RawOrigin::Root.into(),
			0,
			Some(1),
			Some(1),
			Some(Ratio::one()),
			Some(0),
			Some(new_discount),
		));
		assert_eq!(
			AquaDao::subscriptions(0),
			Some(Subscription {
				currency_id: AUSD_CURRENCY,
				vesting_period: 1,
				min_amount: 1,
				min_ratio: Ratio::one(),
				amount: 0,
				discount: new_discount,
				state: SubscriptionState {
					total_sold: 0,
					last_sold_at: 1,
					last_discount: Zero::zero(),
				},
			})
		);
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionUpdated { id: 0 }));
	});
}

#[test]
fn update_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(create_default_subscription());
		assert_noop!(
			AquaDao::update_subscription(RawOrigin::Signed(ALICE).into(), 0, Some(1), None, None, None, None),
			BadOrigin
		);
	});
}

#[test]
fn close_subscription_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		assert_ok!(create_default_subscription());
		assert_ok!(AquaDao::close_subscription(RawOrigin::Root.into(), 0));
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionClosed { id: 0 }));

		assert_eq!(AquaDao::subscriptions(0), None);
	});
}

#[test]
fn close_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(create_default_subscription());
		assert_noop!(
			AquaDao::close_subscription(RawOrigin::Signed(ALICE).into(), 0),
			BadOrigin
		);
	});
}

#[test]
fn subscribe_works() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(create_default_subscription());
			Subscriptions::<Runtime>::mutate(0, |maybe_subscription| {
				if let Some(subscription) = maybe_subscription {
					subscription.state.last_discount = FixedI128::saturating_from_rational(5, 100);
				}
			});

			let payment_amount = dollar(AUSD_CURRENCY) * 100;
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));

			let new_subscription = AquaDao::subscriptions(0).unwrap();
			assert_eq!(new_subscription.state.total_sold, 105_260_000_000_000);
			assert_eq!(new_subscription.state.last_sold_at, 1);
			assert_eq!(
				new_subscription.state.last_discount,
				DiscountRate::saturating_from_rational(5, 100)
			);
			assert_eq!(
				Currencies::free_balance(AUSD_CURRENCY, &ALICE),
				1_999_900 * dollar(AUSD_CURRENCY)
			);
			assert_eq!(MockStakedToken::minted(), (105_260_000_000_000, 1_000));

			System::assert_has_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: 105_260_000_000_000,
			}));
		});
}

#[test]
fn no_discount_increase_on_subscribe_within_interval() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(AquaDao::create_subscription(
				RawOrigin::Root.into(),
				AUSD_CURRENCY,
				1_000,
				dollar(ADAO_CURRENCY) * 10,
				Ratio::saturating_from_rational(1, 10),
				dollar(CurrencyId::Token(ADAO)) * UNITS,
				Discount {
					max: DiscountRate::saturating_from_rational(1, 2),
					interval: 1_000,
					inc_on_idle: DiscountRate::saturating_from_rational(1, 2),
					dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
				},
			));

			let payment_amount = dollar(AUSD_CURRENCY) * 100;
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));
			System::assert_last_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: 99_995_000_000_000,
			}));

			// no discount on new subscription within interval
			MockBlockNumberProvider::set_block_number(998);
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));
			System::assert_last_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: 99_995_000_000_000,
			}));

			// discount increases
			MockBlockNumberProvider::set_block_number(2000);
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));
			System::assert_last_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: 199_965_000_000_000,
			}));
		});
}

#[test]
fn subscribe_with_below_min_ratio_works() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			// min_ratio is 1
			assert_ok!(AquaDao::create_subscription(
				RawOrigin::Root.into(),
				AUSD_CURRENCY,
				1_000,
				dollar(ADAO_CURRENCY) * 10,
				Ratio::one(),
				dollar(CurrencyId::Token(ADAO)) * UNITS,
				Discount {
					max: DiscountRate::saturating_from_rational(2, 10),
					interval: 1,
					inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
					dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
				}
			));
			Subscriptions::<Runtime>::mutate(0, |maybe_subscription| {
				if let Some(subscription) = maybe_subscription {
					subscription.state.last_discount = FixedI128::saturating_from_rational(5, 100);
				}
			});

			let payment_amount = dollar(AUSD_CURRENCY) * 100;
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));
			System::assert_has_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: dollar(ADAO_CURRENCY) * 100,
			}));
		});
}

#[test]
fn subscribe_fails_if_below_min_amount() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(create_default_subscription());

			let payment_amount = dollar(AUSD_CURRENCY) * 1;
			assert_noop!(
				AquaDao::subscribe(RawOrigin::Signed(ALICE).into(), 0, payment_amount, 0),
				Error::<Runtime>::BelowMinSubscriptionAmount
			);
		});
}

#[test]
fn subscribe_fails_if_full() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(AquaDao::create_subscription(
				RawOrigin::Root.into(),
				AUSD_CURRENCY,
				1_000,
				dollar(ADAO_CURRENCY) * 10,
				Ratio::saturating_from_rational(1, 10),
				dollar(CurrencyId::Token(ADAO)) * UNITS,
				Discount {
					max: DiscountRate::saturating_from_rational(2, 10),
					interval: 1,
					inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
					dec_per_unit: DiscountRate::saturating_from_rational(20, UNITS * 100),
				}
			));

			Subscriptions::<Runtime>::mutate(0, |maybe_subscription| {
				if let Some(subscription) = maybe_subscription {
					subscription.state.total_sold = dollar(CurrencyId::Token(ADAO)) * UNITS;
				}
			});

			let payment_amount = dollar(AUSD_CURRENCY) * 100;
			assert_noop!(
				AquaDao::subscribe(RawOrigin::Signed(ALICE).into(), 0, payment_amount, 0),
				Error::<Runtime>::SubscriptionIsFull
			);
		});
}

#[test]
fn subscribe_fails_if_below_target_amount() {
	ExtBuilder::default()
		.balances(vec![(
			AccountId::from(ALICE),
			AUSD_CURRENCY,
			2_000_000 * dollar(AUSD_CURRENCY),
		)])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(create_default_subscription());
			Subscriptions::<Runtime>::mutate(0, |maybe_subscription| {
				if let Some(subscription) = maybe_subscription {
					subscription.state.last_discount = FixedI128::saturating_from_rational(5, 100);
				}
			});

			let payment_amount = dollar(AUSD_CURRENCY) * 10;
			assert_noop!(
				AquaDao::subscribe(
					RawOrigin::Signed(ALICE).into(),
					0,
					payment_amount,
					dollar(ADAO_CURRENCY) * 100
				),
				Error::<Runtime>::BelowMinTargetAmount
			);
		});
}
