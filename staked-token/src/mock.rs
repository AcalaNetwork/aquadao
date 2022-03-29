//! Mocks for Aqua Staked Token module.

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

mod aqua_staked_token {
	pub use super::super::*;
}

pub type AccountId = AccountId32;
pub type BlockNumber = u64;

pub const ALICE: AccountId = AccountId32::new([1u8; 32]);
pub const BOB: AccountId = AccountId32::new([2u8; 32]);
pub const DAO_ACCOUNT: AccountId = AccountId32::new([10u8; 32]);
pub const TREASURY_ACCOUNT: AccountId = AccountId32::new([20u8; 32]);

pub const ADAO_CURRENCY: CurrencyId = Token(TokenSymbol::ADAO);
pub const SDAO_CURRENCY: CurrencyId = Token(TokenSymbol::SDAO);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaxLocks: u32 = 10;
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
	type MaxLocks = MaxLocks;
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
}

parameter_types!(
	pub AquaDaoPalletId: PalletId = PalletId(*b"aqua/dao");
	pub AquaStakedTokenPalletId: PalletId = PalletId(*b"aqua/stt");
	pub InflationRatePerNBlock: (BlockNumber, Rate) = (100, Rate::one());
	pub TreasuryShare: Ratio = Ratio::saturating_from_rational(1, 10);
	pub DaoShare: Ratio = Ratio::saturating_from_rational(1, 10);
	pub DaoDefaultExchangeRate: Rate = Rate::one();
	pub DaoAccount: AccountId = DAO_ACCOUNT;
	pub TreasuryAccount: AccountId = TREASURY_ACCOUNT;
	pub StakedTokenLockIdentifier: LockIdentifier = *b"aqu/vest";
	pub MaxVestingChunks: u32 = 5;
);

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

impl Config for Runtime {
	type Event = Event;
	type Currency = Currencies;
	type UpdateParamsOrigin = EnsureRoot<AccountId>;
	type BlockNumberProvider = MockBlockNumberProvider;
	type InflationRatePerNBlock = InflationRatePerNBlock;
	type TreasuryShare = TreasuryShare;
	type DaoShare = DaoShare;
	type DefaultExchangeRate = DaoDefaultExchangeRate;
	type PalletId = AquaStakedTokenPalletId;
	type TreasuryAccount = TreasuryAccount;
	type DaoAccount = DaoAccount;
	type LockIdentifier = StakedTokenLockIdentifier;
	type MaxVestingChunks = MaxVestingChunks;
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
		AquaStakedToken: aqua_staked_token::{Pallet, Call, Event<T>},
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
