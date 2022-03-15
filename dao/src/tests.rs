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

fn default_subscription() -> SubscriptionOf<Runtime> {
	let units = 1_000_000;
	let amount = dollar(CurrencyId::Token(ADAO)) * units;
	Subscription {
		currency_id: AUSD_CURRENCY,
		vesting_period: 1_000,
		min_amount: dollar(ADAO_CURRENCY) * 10,
		min_ratio: Ratio::saturating_from_rational(1, 10),
		amount,
		discount: Discount {
			max: DiscountRate::saturating_from_rational(2, 10),
			inc_on_idle: DiscountRate::saturating_from_rational(1, 1_000),
			dec_per_unit: DiscountRate::saturating_from_rational(20, units * 100),
		},
		state: SubscriptionState {
			total_sold: 0,
			last_sold_at: 0,
			last_discount: DiscountRate::saturating_from_rational(5, 100),
		},
	}
}

#[test]
fn create_subscription_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let subscription = default_subscription();
		assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionCreated {
			id: 0,
			subscription,
		}));
		assert_eq!(AquaDao::subscription_index(), 1);
	});
}

#[test]
fn create_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let subscription = default_subscription();
		assert_noop!(
			AquaDao::create_subscription(RawOrigin::Signed(ALICE).into(), subscription),
			BadOrigin
		);
	});
}

#[test]
pub fn update_subscription_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let subscription = default_subscription();
		assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));

		let new_discount = Discount {
			max: DiscountRate::one(),
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
					last_sold_at: 0,
					last_discount: DiscountRate::saturating_from_rational(5, 100),
				},
			})
		);
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionUpdated { id: 0 }));
	});
}

#[test]
fn update_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let subscription = default_subscription();
		assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));
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

		let subscription = default_subscription();
		assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));
		assert_ok!(AquaDao::close_subscription(RawOrigin::Root.into(), 0));
		System::assert_has_event(Event::AquaDao(crate::Event::SubscriptionClosed { id: 0 }));

		assert_eq!(AquaDao::subscriptions(0), None);
	});
}

#[test]
fn close_subscription_fails_if_not_required_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let subscription = default_subscription();
		assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));
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

			let subscription = default_subscription();
			assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));

			let payment_amount = dollar(AUSD_CURRENCY) * 100;
			assert_ok!(AquaDao::subscribe(
				RawOrigin::Signed(ALICE).into(),
				0,
				payment_amount,
				0
			));

			let new_subscription = AquaDao::subscriptions(0).unwrap();
			assert_eq!(new_subscription.state.total_sold, 105_370_000_000_000);
			assert_eq!(new_subscription.state.last_sold_at, 1);
			assert_eq!(
				new_subscription.state.last_discount,
				DiscountRate::saturating_from_rational(51, 1000)
			);
			assert_eq!(
				Currencies::free_balance(AUSD_CURRENCY, &ALICE),
				1_999_900 * dollar(AUSD_CURRENCY)
			);
			assert_eq!(MockStakedToken::minted(), (105_370_000_000_000, 1_000));

			System::assert_has_event(Event::AquaDao(crate::Event::Subscribed {
				who: ALICE,
				subscription_id: 0,
				payment_amount,
				subscription_amount: 105_370_000_000_000,
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

			let subscription = default_subscription();
			assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));

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

			let mut subscription = default_subscription();
			subscription.state.total_sold = subscription.amount - 1;
			assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));

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

			let subscription = default_subscription();
			assert_ok!(AquaDao::create_subscription(RawOrigin::Root.into(), subscription));

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
