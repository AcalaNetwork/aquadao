//! Unit tests for Aqua Staked Token module.

#![cfg(test)]

use super::*;
use mock::{Event, *};

use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_runtime::traits::BadOrigin;

#[test]
fn stake_works() {
	ExtBuilder::default()
		// exchange rate: 1 SDAO = 10 ADAO
		.balances(vec![
			(AccountId::from(ALICE), ADAO_CURRENCY, 100),
			(AccountId::from(BOB), SDAO_CURRENCY, 10),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 100),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(AquaStakedToken::stake(RawOrigin::Signed(ALICE).into(), 0));
			assert_eq!(System::events().len(), 0);

			assert_ok!(AquaStakedToken::stake(RawOrigin::Signed(ALICE).into(), 20));
			assert_eq!(Currencies::free_balance(ADAO_CURRENCY, &ALICE), 80);
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &ALICE), 2);
			assert_eq!(
				Currencies::free_balance(ADAO_CURRENCY, &AquaStakedToken::account_id()),
				120
			);
			assert_eq!(Currencies::total_issuance(SDAO_CURRENCY), 12);
			System::assert_has_event(Event::AquaStakedToken(crate::Event::Staked {
				who: ALICE,
				amount: 20,
				received: 2,
			}));
		});
}

#[test]
fn unstake_works() {
	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), SDAO_CURRENCY, 20),
			(AccountId::from(BOB), SDAO_CURRENCY, 30),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 500),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(AquaStakedToken::update_unstake_fee_rate(
				RawOrigin::Root.into(),
				Rate::saturating_from_rational(1, 10)
			));

			assert_ok!(AquaStakedToken::unstake(RawOrigin::Signed(ALICE).into(), 10));
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &ALICE), 10);
			assert_eq!(Currencies::free_balance(ADAO_CURRENCY, &ALICE), 90);
			assert_eq!(Currencies::free_balance(ADAO_CURRENCY, &FeeDestAccount::get()), 10);
			System::assert_has_event(Event::AquaStakedToken(crate::Event::Unstaked {
				who: ALICE,
				amount: 10,
				received: 90,
			}));
		});
}

#[test]
fn claim_works() {
	ExtBuilder::default()
		// exchange rate: 1 SDAO = 10 ADAO
		.balances(vec![
			(AccountId::from(BOB), SDAO_CURRENCY, 10),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 100),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(AquaStakedToken::mint_for_subscription(&ALICE, 100, 10));

			MockBlockNumberProvider::set_block_number(11);
			assert_ok!(AquaStakedToken::claim(RawOrigin::Signed(ALICE).into()));
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &ALICE), 10);
			System::assert_has_event(Event::AquaStakedToken(crate::Event::Claimed { who: ALICE, amount: 10 }));

			assert_noop!(
				AquaStakedToken::claim(RawOrigin::Signed(ALICE).into()),
				Error::<Runtime>::VestingNotFound
			);
		});
}

#[test]
fn cannot_claim_if_no_vesting() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			AquaStakedToken::claim(RawOrigin::Signed(ALICE).into()),
			Error::<Runtime>::VestingNotFound
		);
	});
}

#[test]
fn update_unstake_fee_rate_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(AquaStakedToken::update_unstake_fee_rate(
			RawOrigin::Root.into(),
			Rate::one(),
		));
		assert_eq!(AquaStakedToken::unstake_fee_rate(), Rate::one());
	});
}

#[test]
fn update_unstaked_fee_rate_fails_if_bad_origin() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			AquaStakedToken::update_unstake_fee_rate(RawOrigin::Signed(ALICE).into(), Rate::one(),),
			BadOrigin
		);
	});
}

#[test]
fn inflation_works() {
	ExtBuilder::default()
		.balances(vec![
			(AccountId::from(ALICE), ADAO_CURRENCY, 50),
			(AccountId::from(BOB), SDAO_CURRENCY, 10),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 30),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			AquaStakedToken::on_initialize(99);
			// no inflation yet
			assert_eq!(Currencies::total_issuance(ADAO_CURRENCY), 80);

			AquaStakedToken::on_initialize(100);
			// inflation happened
			// mint: 80 / 0.8 = 100
			assert_eq!(
				Currencies::free_balance(ADAO_CURRENCY, &AquaStakedToken::account_id()),
				130
			);
			// treasury, dao shares: 100 * share / exchange_rate = 100 * 0.1 / 3
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &RewardDestAccount::get()), 3);
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &DaoAccount::get()), 3);
			assert_eq!(MockOnDepositReward::deposit_reward(), (SDAO_CURRENCY, 3));
		});
}

#[test]
fn mint_for_subscription_works() {
	ExtBuilder::default()
		// exchange rate: 1 SDAO = 8 ADAO
		.balances(vec![
			(AccountId::from(BOB), SDAO_CURRENCY, 10),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 80),
		])
		.build()
		.execute_with(|| {
			assert_ok!(AquaStakedToken::mint_for_subscription(&ALICE, 800, 10));
			// mint: 800 / 0.8 = 1_000
			assert_eq!(
				Currencies::free_balance(ADAO_CURRENCY, &AquaStakedToken::account_id()),
				1_080
			);
			// alice SDAO: += 800 / 8
			assert_eq!(Currencies::total_balance(SDAO_CURRENCY, &ALICE), 100);
			// vested, not transferrable
			assert_noop!(
				Currencies::transfer(RawOrigin::Signed(ALICE).into(), BOB, SDAO_CURRENCY, 1),
				orml_tokens::Error::<Runtime>::LiquidityRestrictions
			);
			// treasury, dao shares: 1_000 * share / exchange_rate = 1000 * 0.1 / 8
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &RewardDestAccount::get()), 12);
			assert_eq!(Currencies::free_balance(SDAO_CURRENCY, &DaoAccount::get()), 12);
			assert_eq!(MockOnDepositReward::deposit_reward(), (SDAO_CURRENCY, 12));
		});
}

#[test]
fn vesting_over_max_chunks_fails() {
	ExtBuilder::default()
		// exchange rate: 1 SDAO = 10 ADAO
		.balances(vec![
			(AccountId::from(BOB), SDAO_CURRENCY, 10),
			(AquaStakedToken::account_id(), ADAO_CURRENCY, 100),
		])
		.build()
		.execute_with(|| {
			for i in 0..5 {
				MockBlockNumberProvider::set_block_number(i + 1);
				assert_ok!(AquaStakedToken::mint_for_subscription(&ALICE, 100, 10));
			}

			MockBlockNumberProvider::set_block_number(6);
			assert_noop!(
				AquaStakedToken::mint_for_subscription(&ALICE, 100, 10),
				Error::<Runtime>::MaxVestingChunkExceeded,
			);
		});
}
