//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	parameter_types,
	traits::{EnsureOrigin, Get, LockIdentifier},
	transactional, PalletId,
};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{AccountIdConversion, BlockNumberProvider, CheckedAdd, CheckedSub, One, Saturating, Zero},
	ArithmeticError, FixedPointNumber,
};
use sp_std::result::Result;

use orml_traits::{MultiCurrency, MultiLockableCurrency};

use acala_primitives::{
	bonding::{self, BondingController},
	Balance,
	CurrencyId::{self, Token},
	TokenSymbol::*,
};
use ecosystem_aqua_dao::StakedTokenManager;
use module_support::{Rate, Ratio};

mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;

pub use module::*;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiLockableCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		/// Origin required to update financial parameters, like unstake fee rate, inflation rate
		/// per block etc.
		type UpdateParamsOrigin: EnsureOrigin<Self::Origin>;

		/// The block number provider
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = Self::BlockNumber>;

		/// Inflate rate per `n` block: (n, rate)
		type InflationRatePerNBlock: Get<(Self::BlockNumber, Rate)>;

		#[pallet::constant]
		type TreasuryShare: Get<Ratio>;

		#[pallet::constant]
		type DaoShare: Get<Ratio>;

		#[pallet::constant]
		type DefaultExchangeRate: Get<Rate>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

		#[pallet::constant]
		type TreasuryAccount: Get<Self::AccountId>;

		#[pallet::constant]
		type DaoAccount: Get<Self::AccountId>;

		#[pallet::constant]
		type LockIdentifier: Get<LockIdentifier>;

		#[pallet::constant]
		type MaxVestingChunks: Get<u32>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	#[pallet::getter(fn unstake_fee_rate)]
	pub type UnstakeFeeRate<T> = StorageValue<_, Rate, ValueQuery>;

	pub type BondingLedgerOf<T> = bonding::BondingLedgerOf<Pallet<T>>;

	/// Vesting ledger
	///
	/// Ledger: map AccountId => Option<BondingLedger>
	#[pallet::storage]
	#[pallet::getter(fn ledger)]
	pub type VestingLedger<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, BondingLedgerOf<T>, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// No vesting.
		VestingNotFound,
		/// Max vesting chunk exceeded.
		MaxVestingChunkExceeded,
		/// Below min Vesting amount.
		BelowMinVestingAmount,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		Staked {
			who: T::AccountId,
			amount: Balance,
			received: Balance,
		},
		Unstaked {
			who: T::AccountId,
			amount: Balance,
			received: Balance,
		},
		Claimed {
			who: T::AccountId,
			amount: Balance,
		},
		UnstakeFeeRateUpdated {
			rate: Rate,
		},
		VestingAdded {
			who: T::AccountId,
			amount: Balance,
		},
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		fn on_initialize(now: T::BlockNumber) -> Weight {
			let (n, rate) = T::InflationRatePerNBlock::get();
			// `rem_euclid` should be preferred but not supported by `BlockNumber`. `n`
			// can't be zero in runtime config so it's safe to use modulo `%`.
			if (now % n).is_zero() {
				let total = T::Currency::total_issuance(Token(ADAO));
				if let Some(inflation_amount) = rate.checked_mul_int(total) {
					let _ = Self::inflate(inflation_amount);
				}
				<T as Config>::WeightInfo::on_initialize()
			} else {
				<T as Config>::WeightInfo::on_initialize_without_inflation()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(<T as Config>::WeightInfo::stake())]
		#[transactional]
		pub fn stake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			if amount == Zero::zero() {
				return Ok(());
			}

			let received = Self::to_staked(amount)?;
			T::Currency::transfer(Token(ADAO), &who, &Self::account_id(), amount)?;
			T::Currency::deposit(Token(SDAO), &who, received)?;

			Self::deposit_event(Event::<T>::Staked { who, amount, received });
			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::unstake())]
		#[transactional]
		pub fn unstake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			if amount == Zero::zero() {
				return Ok(());
			}

			let redeem = Self::from_staked(amount)?;
			let fee = Self::unstake_fee_rate()
				.checked_mul_int(redeem)
				.ok_or(ArithmeticError::Overflow)?;
			let received = redeem.checked_sub(fee).ok_or(ArithmeticError::Underflow)?;

			// destroy SDAO
			T::Currency::withdraw(Token(SDAO), &who, amount)?;
			// payback ADAO
			T::Currency::transfer(Token(ADAO), &Self::account_id(), &who, received)?;
			// fee goes to treasury
			T::Currency::transfer(Token(ADAO), &Self::account_id(), &T::TreasuryAccount::get(), fee)?;

			Self::deposit_event(Event::<T>::Unstaked { who, amount, received });
			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::claim())]
		#[transactional]
		pub fn claim(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let now = T::BlockNumberProvider::current_block_number();
			let maybe_change = <Self as BondingController>::withdraw_unbonded(&who, now)?;
			if let Some(change) = maybe_change {
				Self::deposit_event(Event::<T>::Claimed {
					who,
					amount: change.change,
				});
			}
			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::update_unstake_fee_rate())]
		#[transactional]
		pub fn update_unstake_fee_rate(origin: OriginFor<T>, rate: Rate) -> DispatchResult {
			T::UpdateParamsOrigin::ensure_origin(origin)?;
			UnstakeFeeRate::<T>::put(rate);
			Self::deposit_event(Event::<T>::UnstakeFeeRateUpdated { rate });
			Ok(())
		}
	}
}

type BalanceResult = Result<Balance, DispatchError>;

impl<T: Config> Pallet<T> {
	/// Inflate DAO token.
	fn inflate(amount: Balance) -> DispatchResult {
		// fixed_share = treasury_share + dao_share
		let fixed_share = T::TreasuryShare::get()
			.checked_add(&T::DaoShare::get())
			.ok_or(ArithmeticError::Overflow)?;
		// mint = amount / (1 - fixed_share)
		let mint = Rate::one()
			.checked_sub(&fixed_share)
			.ok_or(ArithmeticError::Underflow)?
			.reciprocal()
			.ok_or(ArithmeticError::DivisionByZero)?
			.checked_mul_int(amount)
			.ok_or(ArithmeticError::Overflow)?;

		let treasury_mint = T::TreasuryShare::get()
			.checked_mul_int(mint)
			.ok_or(ArithmeticError::Overflow)?;
		let dao_mint = T::DaoShare::get()
			.checked_mul_int(mint)
			.ok_or(ArithmeticError::Overflow)?;
		let treasury_staked = Self::to_staked(treasury_mint)?;
		let dao_staked = Self::to_staked(dao_mint)?;

		// mint
		T::Currency::deposit(Token(ADAO), &Self::account_id(), mint)?;

		// stake the treasury and DAO share
		T::Currency::deposit(Token(SDAO), &T::TreasuryAccount::get(), treasury_staked)?;
		T::Currency::deposit(Token(SDAO), &T::DaoAccount::get(), dao_staked)?;

		//TODO: add treasury principle

		Ok(())
	}

	pub fn exchange_rate() -> Rate {
		let total = T::Currency::total_balance(Token(ADAO), &Self::account_id());
		let supply = T::Currency::total_issuance(Token(SDAO));
		if supply.is_zero() {
			T::DefaultExchangeRate::get()
		} else {
			Rate::checked_from_rational(total, supply).unwrap_or_else(T::DefaultExchangeRate::get)
		}
	}

	/// Get SDAO token amount from given ADAO `amount`, based on exchange rate.
	fn to_staked(amount: Balance) -> BalanceResult {
		Self::exchange_rate()
			.reciprocal()
			.unwrap_or_else(|| T::DefaultExchangeRate::get().reciprocal().unwrap())
			.checked_mul_int(amount)
			.ok_or(ArithmeticError::Overflow.into())
	}

	/// Get ADAO token amount from given SDAO `amount`, based on exchange rate.
	fn from_staked(amount: Balance) -> BalanceResult {
		Self::exchange_rate()
			.checked_mul_int(amount)
			.ok_or(ArithmeticError::Overflow.into())
	}

	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}
}

impl<T: Config> StakedTokenManager<T::AccountId, T::BlockNumber> for Pallet<T> {
	#[transactional]
	fn mint_for_subscription(who: &T::AccountId, amount: Balance, vesting_period: T::BlockNumber) -> DispatchResult {
		// fixed_share = treasury_share + dao_share
		let fixed_share = T::TreasuryShare::get()
			.checked_add(&T::DaoShare::get())
			.ok_or(ArithmeticError::Overflow)?;
		// mint = amount / (1 - fixed_share)
		let mint = Rate::one()
			.checked_sub(&fixed_share)
			.ok_or(ArithmeticError::Underflow)?
			.reciprocal()
			.ok_or(ArithmeticError::DivisionByZero)?
			.checked_mul_int(amount)
			.ok_or(ArithmeticError::Overflow)?;

		let treasury_mint = T::TreasuryShare::get()
			.checked_mul_int(mint)
			.ok_or(ArithmeticError::Overflow)?;
		let dao_mint = T::DaoShare::get()
			.checked_mul_int(mint)
			.ok_or(ArithmeticError::Overflow)?;
		let treasury_staked = Self::to_staked(treasury_mint)?;
		let dao_staked = Self::to_staked(dao_mint)?;
		let staked = Self::to_staked(amount)?;

		T::Currency::deposit(Token(ADAO), &Self::account_id(), mint)?;

		// mint & stake the treasury and DAO share
		T::Currency::deposit(Token(SDAO), who, staked)?;
		T::Currency::deposit(Token(SDAO), &T::TreasuryAccount::get(), treasury_staked)?;
		T::Currency::deposit(Token(SDAO), &T::DaoAccount::get(), dao_staked)?;

		// SDAO token vesting
		let change = <Self as BondingController>::bond(&who, staked)?;
		let unlock_at = T::BlockNumberProvider::current_block_number().saturating_add(vesting_period);
		let _ = <Self as BondingController>::unbond(&who, staked, unlock_at)?;
		if let Some(change) = change {
			Self::deposit_event(Event::VestingAdded {
				who: who.clone(),
				amount: change.change,
			});
		}

		//TODO: add treasury principle

		Ok(())
	}
}

parameter_types! {
	pub const ZeroMinVesting: Balance = 0;
}

impl<T: Config> BondingController for Pallet<T> {
	// min subscription was checked on minting, so we don't need to check it here.
	type MinBond = ZeroMinVesting;
	type MaxUnbondingChunks = T::MaxVestingChunks;
	type Moment = T::BlockNumber;
	type AccountId = T::AccountId;

	type Ledger = VestingLedger<T>;

	fn available_balance(who: &Self::AccountId, ledger: &BondingLedgerOf<T>) -> Balance {
		let free_balance = T::Currency::free_balance(Token(SDAO), who);
		free_balance.saturating_sub(ledger.total())
	}

	fn apply_ledger(who: &Self::AccountId, ledger: &BondingLedgerOf<T>) -> DispatchResult {
		if ledger.is_empty() {
			T::Currency::remove_lock(T::LockIdentifier::get(), Token(SDAO), who)
		} else {
			T::Currency::set_lock(T::LockIdentifier::get(), Token(SDAO), who, ledger.total())
		}
	}

	fn convert_error(err: bonding::Error) -> DispatchError {
		match err {
			bonding::Error::BelowMinBondThreshold => Error::<T>::BelowMinVestingAmount.into(),
			bonding::Error::MaxUnlockChunksExceeded => Error::<T>::MaxVestingChunkExceeded.into(),
			bonding::Error::NotBonded => Error::<T>::VestingNotFound.into(),
		}
	}
}
