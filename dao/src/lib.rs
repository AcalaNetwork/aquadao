//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::EnsureOrigin, transactional, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{
		AccountIdConversion, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, IntegerSquareRoot, One,
		UniqueSaturatedInto,
	},
	ArithmeticError, FixedI128, FixedPointNumber, FixedU128,
};
use sp_std::result::Result;

use orml_traits::MultiCurrency;

use acala_primitives::{
	Balance,
	CurrencyId::{self, Token},
	TokenSymbol::*,
};
use module_support::{DEXPriceProvider, Price};

mod mock;
mod tests;

pub use module::*;

pub type SubscriptionId = u32;
pub type DiscountRate = FixedI128;

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Subscription<BlockNumber> {
	currency_id: CurrencyId,
	vesting_period: BlockNumber,
	/// minimum subscription amount
	min_amount: Balance,
	min_price: Price,
	amount: Balance,
	discount: Discount,
	state: SubscriptionState<BlockNumber>,
}

pub type SubscriptionOf<T> = Subscription<<T as frame_system::Config>::BlockNumber>;

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, Default, TypeInfo)]
pub struct Discount {
	/// Max discount rate.
	max: DiscountRate,
	/// The percentage to increase on each idle block.
	/// `idle`: the period when there is no new subscription.
	inc_on_idle: DiscountRate,
	/// The percentage to decrease with each unit subscribed.
	/// Could be negative.
	dec_per_unit: DiscountRate,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SubscriptionState<BlockNumber> {
	total_sold: Balance,
	last_sold_at: BlockNumber,
	last_discount: DiscountRate,
}

pub trait StakedTokenManager<AccountId, BlockNumber> {
	fn mint_for_subscription(
		who: &AccountId,
		subscription_amount: Balance,
		vesting_period: BlockNumber,
	) -> DispatchResult;
}

#[frame_support::pallet]
pub mod module {
	use acala_primitives::TokenSymbol;

	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		type StableTokenSymbol: Get<TokenSymbol>;

		type CreatingOrigin: EnsureOrigin<Self::Origin>;

		type Oracle: DEXPriceProvider<CurrencyId>;

		type StakedToken: StakedTokenManager<Self::AccountId, Self::BlockNumber>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn subscription_index)]
	pub type SubscriptionIndex<T> = StorageValue<_, SubscriptionId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn subscriptions)]
	pub type Subscriptions<T: Config> = StorageMap<_, Twox64Concat, SubscriptionId, SubscriptionOf<T>, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// Subscription not found.
		SubscriptionNotFound,
		/// No Price.
		NoPrice,
		/// Subscription is full.
		SubscriptionIsFull,
		/// The received amount on subscription is below minimum target amount.
		BelowMinTargetAmount,
		/// Below minimum subscription amount.
		BelowMinSubscriptionAmount,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		SubscriptionCreated {
			id: SubscriptionId,
			subscription: SubscriptionOf<T>,
		},
		SubscriptionUpdated {
			id: SubscriptionId,
		},
		SubscriptionClosed {
			id: SubscriptionId,
		},
		Subscribed {
			who: T::AccountId,
			subscription_id: SubscriptionId,
			payment_amount: Balance,
			subscription_amount: Balance,
		},
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[transactional]
		pub fn create_subscription(origin: OriginFor<T>, subscription: SubscriptionOf<T>) -> DispatchResult {
			T::CreatingOrigin::ensure_origin(origin)?;

			let subscription_id = SubscriptionIndex::<T>::try_mutate(|id| -> Result<SubscriptionId, DispatchError> {
				let current_id = *id;
				*id = id.checked_add(One::one()).ok_or(ArithmeticError::Overflow)?;
				Ok(current_id)
			})?;
			Subscriptions::<T>::insert(subscription_id, subscription);

			Self::deposit_event(Event::<T>::SubscriptionCreated {
				id: subscription_id,
				subscription,
			});
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn update_subscription(
			origin: OriginFor<T>,
			subscription_id: SubscriptionId,
			vesting_period: Option<T::BlockNumber>,
			min_amount: Option<Balance>,
			min_price: Option<Price>,
			amount: Option<Balance>,
			discount: Option<Discount>,
		) -> DispatchResult {
			T::CreatingOrigin::ensure_origin(origin)?;

			Subscriptions::<T>::try_mutate_exists(subscription_id, |maybe_subscription| -> DispatchResult {
				let subscription = maybe_subscription.as_mut().ok_or(Error::<T>::SubscriptionNotFound)?;

				if let Some(new_vesting_period) = vesting_period {
					subscription.vesting_period = new_vesting_period;
				}
				if let Some(new_min_amount) = min_amount {
					subscription.min_amount = new_min_amount;
				}
				if let Some(new_min_price) = min_price {
					subscription.min_price = new_min_price;
				}
				if let Some(new_amount) = amount {
					subscription.amount = new_amount;
				}
				if let Some(new_discount) = discount {
					subscription.discount = new_discount;
				}

				Self::deposit_event(Event::<T>::SubscriptionUpdated { id: subscription_id });
				Ok(())
			})
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn close_subscription(origin: OriginFor<T>, subscription_id: SubscriptionId) -> DispatchResult {
			T::CreatingOrigin::ensure_origin(origin)?;
			Subscriptions::<T>::take(subscription_id).ok_or(Error::<T>::SubscriptionNotFound)?;
			Self::deposit_event(Event::<T>::SubscriptionClosed { id: subscription_id });
			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn subscribe(
			origin: OriginFor<T>,
			subscription_id: SubscriptionId,
			payment_amount: Balance,
			min_target_amount: Balance,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Subscriptions::<T>::try_mutate_exists(subscription_id, |maybe_subscription| -> DispatchResult {
				let subscription = maybe_subscription.as_mut().ok_or(Error::<T>::SubscriptionNotFound)?;
				let now = frame_system::Pallet::<T>::block_number();
				let (subscription_amount, last_discount) =
					Self::subscription_amount(&subscription, payment_amount, now)?;

				ensure!(
					subscription_amount >= subscription.min_amount,
					Error::<T>::BelowMinSubscriptionAmount
				);
				ensure!(
					subscription_amount <= subscription.amount.saturating_sub(subscription.state.total_sold),
					Error::<T>::SubscriptionIsFull
				);
				ensure!(
					subscription_amount >= min_target_amount,
					Error::<T>::BelowMinTargetAmount
				);

				subscription.state.total_sold = subscription
					.state
					.total_sold
					.checked_add(subscription_amount)
					.expect("Subscription amount is smaller than remaining; qed");
				subscription.state.last_sold_at = now;
				subscription.state.last_discount = last_discount;

				// payment
				T::Currency::transfer(subscription.currency_id, &who, &Self::account_id(), payment_amount)?;
				// mint ADAO token
				T::StakedToken::mint_for_subscription(&who, subscription_amount, subscription.vesting_period)?;

				Self::deposit_event(Event::<T>::Subscribed {
					who,
					subscription_id,
					payment_amount,
					subscription_amount,
				});
				Ok(())
			})
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Calculate the amount of ADAO tokens to be minted for a subscription.
	///
	/// Returns `(amount, last_discount)` if `Ok`.
	fn subscription_amount(
		subscription: &SubscriptionOf<T>,
		payment: Balance,
		now: T::BlockNumber,
	) -> Result<(Balance, DiscountRate), DispatchError> {
		let Subscription {
			currency_id,
			min_price,
			discount,
			state: subscription_state,
			..
		} = subscription;

		// price
		let adao_price = T::Oracle::get_relative_price(Token(ADAO), Token(T::StableTokenSymbol::get()))
			.ok_or(Error::<T>::NoPrice)?;
		let payment_price = T::Oracle::get_relative_price(*currency_id, Token(T::StableTokenSymbol::get()))
			.ok_or(Error::<T>::NoPrice)?;

		// discount

		// idle_block = now - last_sold_at
		let idle_blocks = now
			.checked_sub(&subscription.state.last_sold_at)
			.map(|n| {
				let n_u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(n);
				DiscountRate::checked_from_integer(n_u64 as i128).expect("Block number can't overflow; qed")
			})
			.ok_or(ArithmeticError::Underflow)?;
		// discount_inc = inc_on_idle * idle_blocks
		// discount_dec = dec_per_unit * total_sold
		let discount_inc = discount
			.inc_on_idle
			.checked_mul(&idle_blocks)
			.ok_or(ArithmeticError::Overflow)?;
		let total_sold = DiscountRate::checked_from_integer(subscription_state.total_sold.unique_saturated_into())
			.ok_or(ArithmeticError::Overflow)?;
		let discount_dec = discount
			.dec_per_unit
			.checked_mul(&total_sold)
			.ok_or(ArithmeticError::Overflow)?;
		// price_discount = min(max_discount, last_discount + discount_inc - discount_dec)
		let price_discount = {
			let d = subscription_state
				.last_discount
				.checked_add(&discount_inc)
				.ok_or(ArithmeticError::Overflow)?
				.checked_sub(&discount_dec)
				.ok_or(ArithmeticError::Underflow)?;
			FixedI128::min(d, discount.max)
		};

		// start_price = max(price * (1 - price_discount), min_price)
		let start_price = {
			let ratio = DiscountRate::one()
				.checked_sub(&price_discount)
				.ok_or(ArithmeticError::Underflow)?;
			// ratio is positive, as `discount` <= `discount.max`.
			let ratio_fixed_u128 = Price::from_inner(ratio.into_inner().abs() as u128);
			let discounted_price = adao_price
				.checked_mul(&ratio_fixed_u128)
				.ok_or(ArithmeticError::Overflow)?;
			Price::max(discounted_price, *min_price)
		};

		let payment_value = Price::checked_from_integer(payment)
			.ok_or(ArithmeticError::Overflow)?
			.checked_mul(&payment_price)
			.ok_or(ArithmeticError::Overflow)?;
		let dec_per_unit = Price::from_inner(discount.dec_per_unit.into_inner().abs() as u128);
		let inc = adao_price.checked_mul(&dec_per_unit).ok_or(ArithmeticError::Overflow)?;
		// subscription_amount = (sqrt(2 * inc * payment_value + start_price ** 2) - startPrice) / inc
		let x = (Price::one() + Price::one())
			.checked_mul(&inc)
			.ok_or(ArithmeticError::Overflow)?
			.checked_mul(&payment_value)
			.ok_or(ArithmeticError::Overflow)?;
		let y = start_price.checked_mul(&start_price).ok_or(ArithmeticError::Overflow)?;
		let z = x.checked_add(&y).ok_or(ArithmeticError::Overflow)?;
		let subscription_amount = fixed_u128_sqrt(z)
			.checked_sub(&start_price)
			.ok_or(ArithmeticError::Underflow)?
			.checked_div(&inc)
			.ok_or(ArithmeticError::DivisionByZero)?;

		Ok((fixed_u128_to_integer(subscription_amount), price_discount))
	}

	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}
}

fn fixed_u128_sqrt(n: FixedU128) -> FixedU128 {
	let inner = n.into_inner();
	let inner_sqrt = inner.integer_sqrt();
	let div_sqrt = FixedU128::accuracy().integer_sqrt();
	let new_inner = div_sqrt
		.checked_div(inner_sqrt)
		.expect("`FixedPointNumber` accuracy can't be zero; qed");
	FixedU128::from_inner(new_inner)
}

fn fixed_u128_to_integer(n: FixedU128) -> u128 {
	n.into_inner()
		.checked_div(FixedU128::accuracy())
		.expect("`FixedPointNumber` accuracy can't be zero; qed")
}
