//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::{Get, EnsureOrigin}, transactional, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{AccountIdConversion, CheckedAdd, CheckedSub, One, Zero},
	ArithmeticError, FixedPointNumber,
};
use sp_std::result::Result;

use orml_traits::MultiCurrency;

use acala_primitives::{
	Balance,
	CurrencyId::{self, Token},
	TokenSymbol::*,
};
use module_support::{Rate, Ratio};

mod mock;
mod tests;

pub use module::*;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		/// Origin required to update financial parameters, like unstake fee rate, inflation rate per block etc.
		type UpdateParamsOrigin: EnsureOrigin<Self::Origin>;

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
	}

	#[pallet::storage]
	#[pallet::getter(fn unstake_fee_rate)]
	pub type UnstakeFeeRate<T> = StorageValue<_, Rate, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn inflation_rate_per_block)]
	pub type InflationRatePerBlock<T> = StorageValue<_, Rate, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		Dummy,
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
		UnstakeFeeRateUpdated {
			rate: Rate,
		},
		InflationRatePerBlockUpdated {
			rate: Rate,
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		fn on_finalize(_now: T::BlockNumber) {
			let total = T::Currency::total_issuance(Token(ADAO));
			let maybe_inflation_amount = Self::inflation_rate_per_block().checked_mul_int(total);
			if let Some(inflation_amount) = maybe_inflation_amount {
				let _ = Self::inflate(inflation_amount);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[transactional]
		pub fn stake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let received = Self::to_staked(amount)?;
			T::Currency::transfer(Token(ADAO), &who, &Self::account_id(), amount)?;
			T::Currency::deposit(Token(SADAO), &who, received)?;

			Self::deposit_event(Event::<T>::Staked { who, amount, received });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn unstake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let redeem = Self::from_staked(amount)?;
			let fee = Self::unstake_fee_rate()
				.checked_mul_int(redeem)
				.ok_or(ArithmeticError::Overflow)?;
			let received = redeem.checked_sub(fee).ok_or(ArithmeticError::Underflow)?;

			T::Currency::withdraw(Token(SADAO), &who, amount)?;
			T::Currency::transfer(Token(ADAO), &Self::account_id(), &who, received)?;

			Self::deposit_event(Event::<T>::Unstaked { who, amount, received });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn update_unstake_fee_rate(origin: OriginFor<T>, rate: Rate) -> DispatchResult {
			T::UpdateParamsOrigin::ensure_origin(origin)?;
			UnstakeFeeRate::<T>::put(rate);
			Self::deposit_event(Event::<T>::UnstakeFeeRateUpdated { rate });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn update_inflation_rate_per_block(origin: OriginFor<T>, rate: Rate) -> DispatchResult {
			T::UpdateParamsOrigin::ensure_origin(origin)?;
			InflationRatePerBlock::<T>::put(rate);
			Self::deposit_event(Event::<T>::InflationRatePerBlockUpdated { rate });
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

		//TODO: treasury principle?

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

	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}
}
