//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use codec::MaxEncodedLen;
use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::*;

use orml_traits::MultiCurrency;

use acala_primitives::{Balance, CurrencyId};

mod mock;
mod tests;

pub use module::*;

#[derive(Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct StakeInfo {
	shares: Balance,
	withdrawn: Balance,
}

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn total_share)]
	pub type TotalShares<T> = StorageValue<_, Balance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn total_withdrawn)]
	pub type TotalWithdrawn<T> = StorageValue<_, Balance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn principle)]
	pub type Principle<T> = StorageValue<_, Balance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn stake_infos)]
	pub type StakeInfos<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, StakeInfo, ValueQuery>;

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
		},
		Unstaked {
			who: T::AccountId,
			amount: Balance,
			fee: Balance,
		},
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
			let fee = 0;

			Self::deposit_event(Event::<T>::Unstaked { who, amount, fee });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn unstake_all(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			//TODO: unstake all
			let amount = 0;
			let fee = 0;

			Self::deposit_event(Event::<T>::Unstaked { who, amount, fee });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {}
