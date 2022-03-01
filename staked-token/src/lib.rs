//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
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

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, Default)]
pub struct Vesting<BlockNumber> {
	pub unlock_at: BlockNumber,
	pub amount: Balance,
}
pub type VestingOf<T> = Vesting<<T as frame_system::Config>::BlockNumber>;

const VESTING_LOCK_ID: LockIdentifier = *b"aquavest";

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

		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	#[pallet::getter(fn unstake_fee_rate)]
	pub type UnstakeFeeRate<T> = StorageValue<_, Rate, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn vestings)]
	pub type Vestings<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, VestingOf<T>, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// No vesting,
		VestingNotFound,
		/// Vesting is can't be claimed yet
		NotClaimable,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
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
		InflationRatePerBlockUpdated {
			rate: Rate,
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

			let received = Self::to_staked(amount)?;
			T::Currency::transfer(Token(ADAO), &who, &Self::account_id(), amount)?;
			T::Currency::deposit(Token(SADAO), &who, received)?;

			Self::deposit_event(Event::<T>::Staked { who, amount, received });
			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::unstake())]
		#[transactional]
		pub fn unstake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let redeem = Self::from_staked(amount)?;
			let fee = Self::unstake_fee_rate()
				.checked_mul_int(redeem)
				.ok_or(ArithmeticError::Overflow)?;
			let received = redeem.checked_sub(fee).ok_or(ArithmeticError::Underflow)?;

			// destroy SADAO
			T::Currency::withdraw(Token(SADAO), &who, amount)?;
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

			let claimed_amount = Vestings::<T>::try_mutate(&who, |vesting| -> BalanceResult {
				ensure!(!vesting.amount.is_zero(), Error::<T>::VestingNotFound);
				let now = T::BlockNumberProvider::current_block_number();
				ensure!(vesting.unlock_at <= now, Error::<T>::NotClaimable);

				T::Currency::remove_lock(VESTING_LOCK_ID, Token(SADAO), &who)?;

				let amount = vesting.amount;
				vesting.amount = Zero::zero();

				Ok(amount)
			})?;

			Self::deposit_event(Event::<T>::Claimed {
				who,
				amount: claimed_amount,
			});
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
		// mint = amount * (1 - fixed_share)
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
		T::Currency::deposit(Token(SADAO), &T::TreasuryAccount::get(), treasury_staked)?;
		T::Currency::deposit(Token(SADAO), &T::DaoAccount::get(), dao_staked)?;

		//TODO: add treasury principle

		Ok(())
	}

	fn exchange_rate() -> Rate {
		let total = T::Currency::total_balance(Token(ADAO), &Self::account_id());
		let supply = T::Currency::total_issuance(Token(SADAO));
		if supply.is_zero() {
			T::DefaultExchangeRate::get()
		} else {
			Rate::checked_from_rational(total, supply).unwrap_or_else(T::DefaultExchangeRate::get)
		}
	}

	/// Get SADAO token amount from given ADAO `amount`, based on exchange rate.
	fn to_staked(amount: Balance) -> BalanceResult {
		Self::exchange_rate()
			.reciprocal()
			.unwrap_or_else(|| T::DefaultExchangeRate::get().reciprocal().unwrap())
			.checked_mul_int(amount)
			.ok_or(ArithmeticError::Overflow.into())
	}

	/// Get ADAO token amount from given SADAO `amount`, based on exchange rate.
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
		T::Currency::deposit(Token(SADAO), who, staked)?;
		T::Currency::deposit(Token(SADAO), &T::TreasuryAccount::get(), treasury_staked)?;
		T::Currency::deposit(Token(SADAO), &T::DaoAccount::get(), dao_staked)?;

		// SDAO token vesting, extend the existing vesting period if not claimable.
		Vestings::<T>::try_mutate(&who, |vesting| -> DispatchResult {
			let now = T::BlockNumberProvider::current_block_number();
			let existing_locked = if !vesting.amount.is_zero() && vesting.unlock_at > now {
				vesting.amount
			} else {
				Zero::zero()
			};
			let to_lock = staked.saturating_add(existing_locked);
			T::Currency::set_lock(VESTING_LOCK_ID, Token(SADAO), who, to_lock)?;

			vesting.amount = to_lock;
			vesting.unlock_at = now.saturating_add(vesting_period);

			Ok(())
		})?;

		//TODO: add treasury principle

		Ok(())
	}
}
