//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, transactional, traits::Get};
use frame_system::pallet_prelude::*;

use orml_traits::MultiCurrency;

use acala_primitives::{Balance, CurrencyId};
use module_support::{Ratio, Rate};

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

		#[pallet::constant]
		type TreasuryShare: Get<Ratio>;

		#[pallet::constant]
		type DaoShare: Get<Ratio>;
	}

	#[pallet::storage]
	#[pallet::getter(fn unstake_fee)]
	pub type UnstakeFee<T> = StorageValue<_, Rate, ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		InsufficientBalance,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		Staked {
			who: T::AccountId,
			amount: Balance,
		},
		Unstaked {
			who: T::AccountId,
			amount: Balance,
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[transactional]
		pub fn stake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			//TODO: stake

			Self::deposit_event(Event::<T>::Staked { who, amount });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn unstake(origin: OriginFor<T>, amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			//TODO: unstake

			Self::deposit_event(Event::<T>::Unstaked { who, amount });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn mint(_to: T::AccountId, _amount: Balance) -> DispatchResult {
		//TODO: mint

		Ok(())
	}

	fn inflate(_amount: Balance) -> DispatchResult {
		//TODO: inflate

		Ok(())
	}
}
