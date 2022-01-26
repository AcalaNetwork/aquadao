//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, transactional, traits::EnsureOrigin};
use frame_system::pallet_prelude::*;
use sp_runtime::FixedI128;

use orml_traits::MultiCurrency;

use acala_primitives::{Balance, CurrencyId};
use module_support::Price;

mod mock;
mod tests;

pub use module::*;

pub type SubscriptionId = u32;
pub type DiscountRate = FixedI128;

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, Default, TypeInfo)]
pub struct Discount {
	/// Max discount rate.
	max: DiscountRate,
	/// Min discount rate.
	min: DiscountRate,
	/// The percentage to increase on each idle period.
	/// `idle`: the period when there is no new subscription.
	inc_on_idle: DiscountRate,
	/// The percentage to decrease with each unit subscribed.
	/// Could be negative.
	dec_per_unit: DiscountRate,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Subscription<BlockNumber> {
	currency_id: CurrencyId,
	vesting_period: BlockNumber,
	min_price: Price,
	discount: Discount,
}

#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, TypeInfo)]
pub struct SubscriptionState<BlockNumber> {
	total_sold: Balance,
	last_sold_at: BlockNumber,
	last_discount: DiscountRate,
}

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		type CreatingOrigin: EnsureOrigin<Self::Origin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		Dummy,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		SubscriptionCreated {
			id: SubscriptionId,
			subscription: Subscription<T::BlockNumber>,
		},
		Subscribed {
			who: T::AccountId,
			subscription_id: SubscriptionId,
			payment_amount: Balance,
			received_amount: Balance,
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
		pub fn create_subscription(origin: OriginFor<T>, subscription: Subscription<T::BlockNumber>) -> DispatchResult {
			T::CreatingOrigin::ensure_origin(origin)?;

			//TODO: create subscription

			Self::deposit_event(Event::<T>::SubscriptionCreated { id: 0, subscription });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn subscribe(origin: OriginFor<T>, subscription_id: SubscriptionId, payment_amount: Balance, _min_target_amount: Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;

			//TODO: subscribe

			Self::deposit_event(Event::<T>::Subscribed { who, subscription_id, payment_amount, received_amount: 0 });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {}
