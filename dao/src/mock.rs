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

//! Mocks for Aqua DAO module.

#![cfg(test)]

use super::*;
use acala_primitives::{Amount, TokenSymbol};
use frame_support::{
	parameter_types,
	traits::{Everything, Nothing},
};
use frame_system::EnsureRoot;
use module_support::mocks::MockAddressMapping;
use orml_traits::parameter_type_with_key;
use sp_core::H256;
use sp_runtime::{testing::Header, traits::IdentityLookup, AccountId32};
use sp_std::cell::RefCell;

mod aqua_dao {
	pub use super::super::*;
}

pub type AccountId = AccountId32;
pub type BlockNumber = u64;

pub const ALICE: AccountId = AccountId32::new([1u8; 32]);

pub const AUSD_CURRENCY: CurrencyId = Token(TokenSymbol::AUSD);
pub const ADAO_CURRENCY: CurrencyId = Token(TokenSymbol::ADAO);
pub const DOT_CURRENCY: CurrencyId = Token(TokenSymbol::DOT);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Runtime {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = BlockNumber;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_type_with_key! {
	pub ExistentialDeposits: |_currency_id: CurrencyId| -> Balance {
		Default::default()
	};
}

impl orml_tokens::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type Amount = Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type OnDust = ();
	type MaxLocks = ();
	type DustRemovalWhitelist = Nothing;
}

parameter_types! {
	pub const NativeTokenExistentialDeposit: Balance = 0;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type Event = Event;
	type ExistentialDeposit = NativeTokenExistentialDeposit;
	type AccountStore = frame_system::Pallet<Runtime>;
	type MaxLocks = ();
	type WeightInfo = ();
	type MaxReserves = ();
	type ReserveIdentifier = ();
}

pub type AdaptedBasicCurrency = module_currencies::BasicCurrencyAdapter<Runtime, Balances, Amount, BlockNumber>;

parameter_types! {
	pub const GetNativeCurrencyId: CurrencyId = CurrencyId::Token(TokenSymbol::ACA);
}

impl module_currencies::Config for Runtime {
	type Event = Event;
	type MultiCurrency = Tokens;
	type NativeCurrency = AdaptedBasicCurrency;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type WeightInfo = ();
	type AddressMapping = MockAddressMapping;
	type EVMBridge = ();
	type SweepOrigin = EnsureRoot<AccountId>;
	type OnDust = ();
	type GasToWeight = ();
}

thread_local! {
	static DOT_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
	static ADAO_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
}

pub struct MockPriceProvider;
impl MockPriceProvider {
	pub fn set_price(currency_id: CurrencyId, price: Option<Price>) {
		match currency_id {
			DOT_CURRENCY => DOT_PRICE.with(|v| *v.borrow_mut() = price),
			_ => {}
		}
	}
}
impl PriceProvider<CurrencyId> for MockPriceProvider {
	fn get_price(currency_id: CurrencyId) -> Option<Price> {
		match currency_id {
			AUSD_CURRENCY => Some(Price::one()),
			DOT_CURRENCY => DOT_PRICE.with(|v| *v.borrow()),
			_ => None,
		}
	}
}
impl DEXPriceProvider<CurrencyId> for MockPriceProvider {
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<Price> {
		if quote != AUSD_CURRENCY {
			return None;
		}

		match base {
			DOT_CURRENCY => DOT_PRICE.with(|v| *v.borrow()),
			ADAO_CURRENCY => ADAO_PRICE.with(|v| *v.borrow()),
			_ => None,
		}
	}
}

thread_local! {
	static MINT_INFO: RefCell<(Balance, BlockNumber)> = RefCell::new((0, 0));
}

pub struct MockStakedToken;
impl MockStakedToken {
	pub fn minted() -> (Balance, BlockNumber) {
		MINT_INFO.with(|v| *v.borrow())
	}
}
impl StakedTokenManager<AccountId, BlockNumber> for MockStakedToken {
	fn mint_for_subscription(
		_who: &AccountId,
		subscription_amount: Balance,
		vesting_period: BlockNumber,
	) -> DispatchResult {
		MINT_INFO.with(|v| *v.borrow_mut() = (subscription_amount, vesting_period));
		Ok(())
	}
}

thread_local! {
	static CURRENT_BLOCK_NUMBER: RefCell<BlockNumber> = RefCell::new(1);
}

pub struct MockBlockNumberProvider;
impl MockBlockNumberProvider {
	pub fn set_block_number(n: BlockNumber) {
		CURRENT_BLOCK_NUMBER.with(|v| *v.borrow_mut() = n);
	}
}
impl BlockNumberProvider for MockBlockNumberProvider {
	type BlockNumber = BlockNumber;

	fn current_block_number() -> Self::BlockNumber {
		CURRENT_BLOCK_NUMBER.with(|v| *v.borrow())
	}
}

parameter_types!(
	pub const StableCurrencyId: CurrencyId = AUSD_CURRENCY;
	pub AquaDaoPalletId: PalletId = PalletId(*b"aqua/dao");
);

impl Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type StableCurrencyId = StableCurrencyId;
	type UpdateOrigin = EnsureRoot<AccountId>;
	type AssetPriceProvider = MockPriceProvider;
	type AdaoPriceProvider = MockPriceProvider;
	type BlockNumberProvider = MockBlockNumberProvider;
	type StakedToken = MockStakedToken;
	type PalletId = AquaDaoPalletId;
	type WeightInfo = ();
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
type Block = frame_system::mocking::MockBlock<Runtime>;

frame_support::construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Tokens: orml_tokens::{Pallet, Storage, Event<T>, Config<T>},
		Currencies: module_currencies::{Pallet, Call, Event<T>},
		AquaDao: aqua_dao::{Pallet, Call, Event<T>},
	}
);

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { balances: vec![] }
	}
}

impl ExtBuilder {
	pub fn balances(mut self, balances: Vec<(AccountId, CurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap();

		orml_tokens::GenesisConfig::<Runtime> {
			balances: self.balances,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}
