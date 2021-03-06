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

//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::EnsureOrigin, transactional, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{
		AccountIdConversion, BlockNumberProvider, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, IntegerSquareRoot,
		One, Saturating, UniqueSaturatedInto, Zero,
	},
	ArithmeticError, FixedI128, FixedPointNumber, FixedU128,
};
use sp_std::result::Result;

use orml_traits::MultiCurrency;

use acala_primitives::{
	Balance,
	CurrencyId::{self, Token},
	TokenInfo,
	TokenSymbol::*,
};
use module_support::{DEXPriceProvider, Price, PriceProvider, Ratio};

mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;

pub use module::*;

pub type SubscriptionId = u32;
pub type DiscountRate = FixedI128;

/// Subscription parameters and state.
#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Subscription<BlockNumber> {
	/// The currency id for payments.
	pub currency_id: CurrencyId,
	/// Vesting period for staked tokens on subscribe.
	pub vesting_period: BlockNumber,
	/// minimum subscription amount.
	pub min_amount: Balance,
	/// At least this amount of subscribed currency per aDAO.
	pub min_ratio: Ratio,
	/// The maximum amount that can be subscribed.
	pub amount: Balance,
	/// Discount parameters.
	pub discount: Discount<BlockNumber>,
	/// Subscription state.
	pub state: SubscriptionState<BlockNumber>,
}

pub type SubscriptionOf<T> = Subscription<<T as frame_system::Config>::BlockNumber>;

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, Default, TypeInfo)]
pub struct Discount<BlockNumber> {
	/// Max discount rate.
	pub max: DiscountRate,
	/// The amount of block number, as the unit for `inc_on_idle` calculation.
	pub interval: BlockNumber,
	/// The percentage to increase for each interval.
	/// `idle`: the period when there is no new subscription.
	pub inc_on_idle: DiscountRate,
	/// The percentage to decrease with 1 aDAO subscribed.
	/// Could be negative.
	pub dec_per_unit: DiscountRate,
}

/// The subscription state.
#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SubscriptionState<BlockNumber> {
	/// Total amount of sold subscriptions.
	pub total_sold: Balance,
	/// The block number on which the latest subscribing happened.
	pub last_sold_at: BlockNumber,
	/// The discount of the latest subscribing.
	pub last_discount: DiscountRate,
}

/// SDAO token manager.
pub trait StakedTokenManager<AccountId, BlockNumber> {
	/// Mint given `amount` of ADAO token, stake and vesting for `vesting_period` blocks.
	fn mint_for_subscription(
		who: &AccountId,
		subscription_amount: Balance,
		vesting_period: BlockNumber,
	) -> DispatchResult;
}

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		type StableCurrencyId: Get<CurrencyId>;

		/// The required origin to create/update/close subscriptions.
		type UpdateOrigin: EnsureOrigin<Self::Origin>;

		/// Used for payment currency prices.
		type AssetPriceProvider: PriceProvider<CurrencyId>;

		/// Used for `ADAO` token price.
		type AdaoPriceProvider: DEXPriceProvider<CurrencyId>;

		/// The block number provider
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = Self::BlockNumber>;

		/// The staked token manager.
		type StakedToken: StakedTokenManager<Self::AccountId, Self::BlockNumber>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

		type WeightInfo: WeightInfo;
	}

	/// Track the next available subscription index.
	#[pallet::storage]
	#[pallet::getter(fn subscription_index)]
	pub type SubscriptionIndex<T> = StorageValue<_, SubscriptionId, ValueQuery>;

	/// Subscriptions. Will be remove if closed.
	/// Subscriptions: map SubscriptionId -> Option<Subscription>
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
		/// Currency has no decimals info.
		NoDecimalsInfo,
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
		/// Create a subscription. Requires `T::UpdateOrigin` origin.
		#[pallet::weight(<T as Config>::WeightInfo::create_subscription())]
		#[transactional]
		pub fn create_subscription(
			origin: OriginFor<T>,
			currency_id: CurrencyId,
			vesting_period: T::BlockNumber,
			#[pallet::compact] min_amount: Balance,
			min_ratio: Ratio,
			#[pallet::compact] amount: Balance,
			discount: Discount<T::BlockNumber>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			let subscription_id = SubscriptionIndex::<T>::try_mutate(|id| -> Result<SubscriptionId, DispatchError> {
				let current_id = *id;
				*id = id.checked_add(One::one()).ok_or(ArithmeticError::Overflow)?;
				Ok(current_id)
			})?;
			let subscription: SubscriptionOf<T> = Subscription {
				currency_id,
				vesting_period,
				min_amount,
				min_ratio,
				amount,
				discount,
				state: SubscriptionState {
					total_sold: Zero::zero(),
					last_sold_at: T::BlockNumberProvider::current_block_number(),
					last_discount: Zero::zero(),
				},
			};
			Subscriptions::<T>::insert(subscription_id, subscription);

			Self::deposit_event(Event::<T>::SubscriptionCreated {
				id: subscription_id,
				subscription,
			});
			Ok(())
		}

		/// Update a subscription. Requires `T::UpdateOrigin` origin.
		#[pallet::weight(<T as Config>::WeightInfo::update_subscription())]
		#[transactional]
		pub fn update_subscription(
			origin: OriginFor<T>,
			subscription_id: SubscriptionId,
			vesting_period: Option<T::BlockNumber>,
			min_amount: Option<Balance>,
			min_ratio: Option<Ratio>,
			amount: Option<Balance>,
			discount: Option<Discount<T::BlockNumber>>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			Subscriptions::<T>::try_mutate_exists(subscription_id, |maybe_subscription| -> DispatchResult {
				let subscription = maybe_subscription.as_mut().ok_or(Error::<T>::SubscriptionNotFound)?;

				if let Some(new_vesting_period) = vesting_period {
					subscription.vesting_period = new_vesting_period;
				}
				if let Some(new_min_amount) = min_amount {
					subscription.min_amount = new_min_amount;
				}
				if let Some(new_min_ratio) = min_ratio {
					subscription.min_ratio = new_min_ratio;
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

		/// Close a subscription. Requires `T::ClosingOrigin` origin.
		#[pallet::weight(<T as Config>::WeightInfo::close_subscription())]
		#[transactional]
		pub fn close_subscription(origin: OriginFor<T>, subscription_id: SubscriptionId) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			Subscriptions::<T>::take(subscription_id).ok_or(Error::<T>::SubscriptionNotFound)?;
			Self::deposit_event(Event::<T>::SubscriptionClosed { id: subscription_id });
			Ok(())
		}

		/// Subscribe to given `subscription_id`, would fail if below minimum target amount.
		#[pallet::weight(<T as Config>::WeightInfo::subscribe())]
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
				let now = T::BlockNumberProvider::current_block_number();
				let (subscription_amount, last_discount) =
					Self::subscription_amount(subscription, payment_amount, now)?;

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
			min_ratio,
			discount,
			state: subscription_state,
			..
		} = subscription;

		// ADAO price: from DEX
		let adao_price = T::AdaoPriceProvider::get_relative_price(Token(ADAO), T::StableCurrencyId::get())
			.ok_or(Error::<T>::NoPrice)?;
		// Payment currency price, from oracles
		let payment_price = T::AssetPriceProvider::get_relative_price(*currency_id, T::StableCurrencyId::get())
			.ok_or(Error::<T>::NoPrice)?;

		// discount

		// idle_intervals = (now - last_sold_at) / interval
		let idle_intervals = now
			.saturating_sub(subscription.state.last_sold_at)
			.checked_div(&subscription.discount.interval)
			.map(|n| {
				let n_u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(n);
				DiscountRate::checked_from_integer(n_u64 as i128).expect("Block number can't overflow; qed")
			})
			.ok_or(ArithmeticError::Underflow)?;
		// discount_inc = inc_on_idle * idle_intervals
		let discount_inc = discount
			.inc_on_idle
			.checked_mul(&idle_intervals)
			.ok_or(ArithmeticError::Overflow)?;
		// discount_dec = dec_per_unit * total_sold
		let discount_dec = {
			let adao_accuracy = Self::currency_accuracy(Token(ADAO))?;
			// one unit: 1 ADAO, which is 10 ^ 12
			let total_sold_units: i128 = subscription_state
				.total_sold
				.checked_div(adao_accuracy)
				.expect("Currency decimals cannot be zero; qed")
				.unique_saturated_into();
			discount
				.dec_per_unit
				.checked_mul(&DiscountRate::checked_from_integer(total_sold_units).ok_or(ArithmeticError::Overflow)?)
				.ok_or(ArithmeticError::Overflow)?
		};
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

		// start_price = price * (1 - price_discount)
		let start_price = {
			let ratio = DiscountRate::one()
				.checked_sub(&price_discount)
				.ok_or(ArithmeticError::Underflow)?;
			// ratio is positive, as `discount` <= `discount.max`.
			let ratio_fixed_u128 = Price::from_inner(ratio.into_inner().abs() as u128);
			adao_price
				.checked_mul(&ratio_fixed_u128)
				.ok_or(ArithmeticError::Overflow)?
		};

		let payment_value = Price::checked_from_integer(payment)
			.ok_or(ArithmeticError::Overflow)?
			.checked_mul(&payment_price)
			.ok_or(ArithmeticError::Overflow)?;
		let dec_per_unit = Price::from_inner(discount.dec_per_unit.into_inner().abs() as u128);
		let inc = adao_price.checked_mul(&dec_per_unit).ok_or(ArithmeticError::Overflow)?;
		// receive_amount = (sqrt(2 * inc * payment_value + start_price ** 2) - startPrice) / inc
		let x = {
			let payment_accuracy = Self::currency_accuracy(*currency_id)?;
			(Price::one() + Price::one())
				.checked_mul(&inc)
				.ok_or(ArithmeticError::Overflow)?
				.checked_mul(&payment_value)
				.ok_or(ArithmeticError::Overflow)?
				// payment value needs to be normalized into units
				.checked_div(&Price::saturating_from_integer(payment_accuracy))
				.expect("Currency accuracy cannot be zero; qed")
		};
		let y = start_price.checked_mul(&start_price).ok_or(ArithmeticError::Overflow)?;
		let z = x.checked_add(&y).ok_or(ArithmeticError::Overflow)?;

		let receive_amount = {
			let sqrt = fixed_u128_sqrt(z)?;
			let amount = sqrt
				.checked_sub(&start_price)
				.ok_or(ArithmeticError::Underflow)?
				.checked_div(&inc)
				.ok_or(ArithmeticError::DivisionByZero)?;
			Self::fixed_u128_to_adao_balance(amount)?
		};
		let max_amount = min_ratio
			.reciprocal()
			.ok_or(ArithmeticError::DivisionByZero)?
			.checked_mul_int(payment)
			.ok_or(ArithmeticError::Overflow)?;
		let final_amount = receive_amount.min(max_amount);

		Ok((final_amount, price_discount))
	}

	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}

	fn currency_accuracy(currency: CurrencyId) -> Result<u128, DispatchError> {
		let decimals = currency.decimals().ok_or(Error::<T>::NoDecimalsInfo)?;
		Ok(10_u128.pow(decimals as u32))
	}

	fn fixed_u128_to_adao_balance(n: FixedU128) -> Result<Balance, DispatchError> {
		let adao_accuracy = Self::currency_accuracy(Token(ADAO))?;
		Ok(n.into_inner()
			.checked_mul(adao_accuracy)
			.ok_or(ArithmeticError::Overflow)?
			.checked_div(FixedU128::accuracy())
			.expect("`FixedPointNumber` accuracy can't be zero; qed"))
	}
}

/// Square root of a `FixedU128` number.
fn fixed_u128_sqrt(n: FixedU128) -> Result<FixedU128, DispatchError> {
	let inner = n.into_inner();
	let inner_sqrt = inner.integer_sqrt();
	let accuracy_sqrt = FixedU128::accuracy().integer_sqrt();
	let new_inner = inner_sqrt.checked_mul(accuracy_sqrt).ok_or(ArithmeticError::Overflow)?;
	Ok(FixedU128::from_inner(new_inner))
}
