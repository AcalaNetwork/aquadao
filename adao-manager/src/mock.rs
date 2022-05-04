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

//! Mocks for Aqua DAO manager module.

#![cfg(test)]

use super::*;
use acala_primitives::DexShare;
use frame_support::{
	construct_runtime, ord_parameter_types, parameter_types,
	traits::{ConstU32, ConstU64, Everything, Nothing},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use module_support::{
	mocks::{MockAddressMapping, MockStableAsset},
	Price,
};
use orml_traits::parameter_type_with_key;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{IdentityLookup, One},
	AccountId32,
};
use sp_std::cell::RefCell;

pub type AccountId = AccountId32;
pub type BlockNumber = u64;
pub const ALICE: AccountId = AccountId32::new([0; 32]);
pub const BOB: AccountId = AccountId32::new([1; 32]);
pub const DAO: AccountId = AccountId32::new([2; 32]);
pub const AUSD: CurrencyId = CurrencyId::Token(TokenSymbol::AUSD);
pub const ACA: CurrencyId = CurrencyId::Token(TokenSymbol::ACA);
pub const ADAO: CurrencyId = CurrencyId::Token(TokenSymbol::ADAO);
pub const DOT: CurrencyId = CurrencyId::Token(TokenSymbol::DOT);
pub const ACA_AUSD_LP: CurrencyId =
	CurrencyId::DexShare(DexShare::Token(TokenSymbol::ACA), DexShare::Token(TokenSymbol::AUSD));
pub const ADAO_AUSD_LP: CurrencyId =
	CurrencyId::DexShare(DexShare::Token(TokenSymbol::AUSD), DexShare::Token(TokenSymbol::ADAO));

impl frame_system::Config for Runtime {
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = BlockNumber;
	type Call = Call;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = ConstU64<250>;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type Event = Event;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = frame_system::Pallet<Runtime>;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
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

pub type AdaptedBasicCurrency = module_currencies::BasicCurrencyAdapter<Runtime, Balances, Amount, BlockNumber>;

parameter_types! {
	pub const GetNativeCurrencyId: CurrencyId = ACA;
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
}

parameter_types! {
	pub const DEXPalletId: PalletId = PalletId(*b"aca/dexm");
	pub const GetExchangeFee: (u32, u32) = (0, 100);
	pub const TradingPathLimit: u32 = 4;
	pub EnabledTradingPairs: Vec<TradingPair> = vec![
		TradingPair::from_currency_ids(ACA, AUSD).unwrap(),
		TradingPair::from_currency_ids(ADAO, AUSD).unwrap(),
	];
	pub const ExtendedProvisioningBlocks: BlockNumber = 0;
}

ord_parameter_types! {
	pub const Alice: AccountId = ALICE;
}

impl module_dex::Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type GetExchangeFee = GetExchangeFee;
	type TradingPathLimit = TradingPathLimit;
	type PalletId = DEXPalletId;
	type Erc20InfoMapping = ();
	type DEXIncentives = ();
	type WeightInfo = ();
	type ListingOrigin = EnsureSignedBy<Alice, AccountId>;
	type ExtendedProvisioningBlocks = ExtendedProvisioningBlocks;
	type OnLiquidityPoolUpdated = ();
	type StableAsset = MockStableAsset<CurrencyId, Balance, AccountId, BlockNumber>;
}

thread_local! {
	static ACA_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
	static AUSD_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
	static ADAO_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
	static ACA_AUSD_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
	static ADAO_AUSD_PRICE: RefCell<Option<Price>> = RefCell::new(Some(Price::one()));
}

pub struct MockPriceSource;
impl MockPriceSource {
	pub fn set_price(currency_id: CurrencyId, price: Option<Price>) {
		match currency_id {
			ACA => ACA_PRICE.with(|v| *v.borrow_mut() = price),
			AUSD => AUSD_PRICE.with(|v| *v.borrow_mut() = price),
			ADAO => ADAO_PRICE.with(|v| *v.borrow_mut() = price),
			ACA_AUSD_LP => ACA_AUSD_PRICE.with(|v| *v.borrow_mut() = price),
			ADAO_AUSD_LP => ADAO_AUSD_PRICE.with(|v| *v.borrow_mut() = price),
			_ => {}
		}
	}
}
impl PriceProvider<CurrencyId> for MockPriceSource {
	fn get_price(currency_id: CurrencyId) -> Option<Price> {
		match currency_id {
			ACA => ACA_PRICE.with(|v| *v.borrow()),
			AUSD => AUSD_PRICE.with(|v| *v.borrow()),
			ACA_AUSD_LP => ACA_AUSD_PRICE.with(|v| *v.borrow()),
			ADAO_AUSD_LP => ADAO_AUSD_PRICE.with(|v| *v.borrow()),
			_ => None,
		}
	}
}
impl DEXPriceProvider<CurrencyId> for MockPriceSource {
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<Price> {
		if quote != AUSD {
			return None;
		}

		match base {
			ADAO => ADAO_PRICE.with(|v| *v.borrow()),
			_ => None,
		}
	}
}

parameter_types! {
	pub const GetStableCurrency: CurrencyId = AUSD;
	pub const GetDaoAccount: AccountId = DAO;
	pub const AquaDaoPalletId: PalletId = PalletId(*b"aca/adao");
}

impl module::Config for Runtime {
	type Event = Event;
	type StableCurrencyId = GetStableCurrency;
	type RebalancePeriod = ConstU64<2>;
	type RebalanceOffset = ConstU64<1>;
	type DaoAccount = GetDaoAccount;
	type PalletId = AquaDaoPalletId;
	type DEX = DexModule;
	type Currency = Currencies;
	type UpdateOrigin = EnsureSignedBy<Alice, AccountId>;
	type AssetPriceProvider = MockPriceSource;
	type AdaoPriceProvider = MockPriceSource;
	type WeightInfo = ();
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system,
			Balances: pallet_balances,
			DexModule: module_dex,
			Tokens: orml_tokens,
			Currencies: module_currencies,
			AquaDAO: module,
		}
);

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![(ALICE, AUSD, 1_000_000)],
		}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Runtime>()
			.unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![(ALICE, 1_000_000)],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		orml_tokens::GenesisConfig::<Runtime> {
			balances: self.balances,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		module_dex::GenesisConfig::<Runtime> {
			initial_listing_trading_pairs: vec![],
			initial_enabled_trading_pairs: EnabledTradingPairs::get(),
			initial_added_liquidity_pools: vec![],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
