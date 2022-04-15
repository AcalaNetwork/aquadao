//! # AquaDao pallet

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{log, pallet_prelude::*, require_transactional, traits::EnsureOrigin, transactional, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{AccountIdConversion, SaturatedConversion, Saturating, UniqueSaturatedInto, Zero},
	ArithmeticError, FixedI128, FixedPointNumber, FixedU128,
};
use sp_std::{collections::btree_map::BTreeMap, prelude::*, result::Result};

use orml_traits::MultiCurrency;

use acala_primitives::{
	Amount, Balance,
	CurrencyId::{self, Token},
	TokenSymbol::{self, *},
	TradingPair,
};
use module_support::{DEXManager, PriceProvider};

pub use module::*;

mod mock;
mod tests;

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct Allocation {
	value: Balance,
	range: Balance,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct AllocationAdjustment {
	value: i128,
	range: i128,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct AllocationPercent {
	value: FixedU128,
	min: FixedU128,
	max: FixedU128,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct AllocationDiff {
	current: FixedU128,
	target: FixedU128,
	diff: FixedI128,
	range_diff: FixedI128,
	diff_amount: Amount,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
struct CurrentAllocation {
	amount: Balance,
	value: Balance,
	percent: FixedU128,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Strategy {
	kind: StrategyKind,
	percent_per_trade: FixedU128,
	max_amount_per_trade: i128,
	min_amount_per_trade: i128,
}

impl Strategy {
	fn trade_amount(&self, diff: i128, max: i128) -> i128 {
		let diff_abs = diff.abs();
		if (max <= self.min_amount_per_trade) || (diff_abs <= self.min_amount_per_trade) {
			return Zero::zero();
		}
		let amount = self.percent_per_trade.saturating_mul_int(diff_abs);
		i128::min(self.min_amount_per_trade.max(amount), self.max_amount_per_trade).min(max)
	}
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum StrategyKind {
	LiquidityProvisionAusdAdao,
	LiquidityProvisionAusdOther(TokenSymbol),
}

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Currency: MultiCurrency<Self::AccountId, Balance = Balance, CurrencyId = CurrencyId>;

		type StableCurrencyId: Get<CurrencyId>;

		type AssetPriceProvider: PriceProvider<CurrencyId>;

		type UpdateOrigin: EnsureOrigin<Self::Origin>;

		type DEX: DEXManager<Self::AccountId, CurrencyId, Balance>;

		#[pallet::constant]
		type RebalancePeriod: Get<Self::BlockNumber>;

		#[pallet::constant]
		type RebalanceOffset: Get<Self::BlockNumber>;

		#[pallet::constant]
		type DaoAccount: Get<Self::AccountId>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	#[pallet::error]
	pub enum Error<T> {
		ZeroTargetAllocation,
		TargetAllocationNotFound,
		NoPrice,
		InvalidTradingPair,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		TargetAllocationSet {
			currency_id: CurrencyId,
			allocation: Allocation,
		},
		TargetAllocationRemoved {
			currency_id: CurrencyId,
		},
		TargetAllocationAdjusted {
			currency_id: CurrencyId,
			adjustment: AllocationAdjustment,
		},
		StrategiesSet {
			strategies: Vec<Strategy>,
		},
	}

	#[pallet::storage]
	#[pallet::getter(fn target_allocations)]
	pub type TargetAllocations<T> = StorageValue<_, BTreeMap<CurrencyId, Allocation>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn target_allocation_percents)]
	pub type TargetAllocationPercents<T> = StorageValue<_, BTreeMap<CurrencyId, AllocationPercent>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn strategies)]
	pub type Strategies<T> = StorageValue<_, Vec<Strategy>, ValueQuery>;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		fn on_initialize(now: T::BlockNumber) -> Weight {
			// Checked arithmetic but not supported by `BlockNumber`. `T::RebalancePeriod`
			// can't be zero in runtime config so it's safe.
			if (now % T::RebalancePeriod::get()) == T::RebalanceOffset::get() {
				let strategies = Strategies::<T>::get();
				let index: u32 = (now / T::RebalancePeriod::get()).unique_saturated_into();
				// Checked remainder to not panic
				let strategy_index = index
					.checked_rem(strategies.len().saturated_into::<u32>())
					.unwrap_or_default();

				if let Some(strategy) = strategies.get(strategy_index as usize) {
					match Self::allocation_diff() {
						Ok(diff) => {
							if let Err(e) = Self::rebalance(strategy, diff) {
								log::error!(target: "adao-manager", "Rebalance failed: {:?}", e);
							}
						}
						Err(e) => log::error!(target: "adao-manager", "Getting allocation diff failed: {:?}", e),
					}
				}
			}

			0
		}

		// Ensure `T::RebalancePeriod` is not zero
		#[cfg(feature = "std")]
		fn integrity_test() {
			assert!(!T::RebalancePeriod::get().is_zero());
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[transactional]
		pub fn set_target_allocations(
			origin: OriginFor<T>,
			targets: Vec<(CurrencyId, Option<Allocation>)>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			TargetAllocations::<T>::mutate(|allocations| {
				targets.into_iter().for_each(|(currency_id, maybe_allocation)| {
					if let Some(allocation) = maybe_allocation {
						allocations.insert(currency_id, allocation);
						Self::deposit_event(Event::<T>::TargetAllocationSet {
							currency_id,
							allocation,
						});
					} else {
						allocations.remove(&currency_id);
						Self::deposit_event(Event::<T>::TargetAllocationRemoved { currency_id });
					}
				});
			});

			Self::update_target_allocation_percents()
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn adjust_target_allocations(
			origin: OriginFor<T>,
			adjustments: Vec<(CurrencyId, AllocationAdjustment)>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			TargetAllocations::<T>::try_mutate(|allocations| -> DispatchResult {
				for (currency_id, adjustment) in adjustments.into_iter() {
					let mut allocation = allocations
						.get_mut(&currency_id)
						.ok_or(Error::<T>::TargetAllocationNotFound)?;

					allocation.value = if adjustment.value.is_positive() {
						allocation.value.saturating_add(adjustment.value.saturated_into())
					} else {
						allocation.value.saturating_sub(adjustment.value.abs().saturated_into())
					};

					allocation.range = if adjustment.range.is_positive() {
						allocation.range.saturating_add(adjustment.range.saturated_into())
					} else {
						allocation.range.saturating_sub(adjustment.range.abs().saturated_into())
					};

					Self::deposit_event(Event::<T>::TargetAllocationAdjusted {
						currency_id,
						adjustment,
					});
				}
				Ok(())
			})?;

			Self::update_target_allocation_percents()
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn set_strategies(origin: OriginFor<T>, strategies: Vec<Strategy>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			Strategies::<T>::set(strategies.clone());
			Self::deposit_event(Event::<T>::StrategiesSet { strategies });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}

	fn update_target_allocation_percents() -> DispatchResult {
		let target_total = Self::target_allocations()
			.values()
			.fold(Zero::zero(), |acc: u128, allocation| {
				acc.saturating_add(allocation.value)
			});
		if target_total.is_zero() {
			return Err(Error::<T>::ZeroTargetAllocation.into());
		}

		TargetAllocationPercents::<T>::mutate(|allocation_percents| {
			Self::target_allocations()
				.into_iter()
				.for_each(|(currency_id, allocation)| {
					// Checked that total vaule is not zero above, qed.
					let percent = FixedU128::saturating_from_rational(allocation.value, target_total);
					let min = FixedU128::saturating_from_rational(
						allocation.value.saturating_sub(allocation.range),
						target_total,
					);
					let max = FixedU128::saturating_from_rational(
						allocation.value.saturating_add(allocation.range),
						target_total,
					);
					allocation_percents.insert(
						currency_id,
						AllocationPercent {
							value: percent,
							min,
							max,
						},
					);
				});
		});

		Ok(())
	}

	fn allocation_diff() -> Result<BTreeMap<CurrencyId, AllocationDiff>, DispatchError> {
		let (current_allocations, total_value) = Self::current_allocations()?;
		let target_allocation_percents = Self::target_allocation_percents();

		let mut diff = BTreeMap::new();

		for (currency_id, target_percent) in target_allocation_percents.iter() {
			let target_value = target_percent.value.saturating_mul_int(total_value);
			let price = T::AssetPriceProvider::get_relative_price(*currency_id, T::StableCurrencyId::get())
				.ok_or(Error::<T>::NoPrice)?;
			let target_amount = price
				.reciprocal()
				.ok_or(ArithmeticError::DivisionByZero)?
				.saturating_mul_int(target_value);

			if let Some(current) = current_allocations.get(currency_id) {
				let range_diff = if current.percent < target_percent.min {
					// current.percent - target.minPercent
					let mut d_inner: i128 = target_percent
						.min
						.saturating_sub(current.percent)
						.into_inner()
						.unique_saturated_into();
					d_inner = d_inner.saturating_mul(-1);
					FixedI128::from_inner(d_inner)
				} else if current.percent > target_percent.max {
					// current.percent - target.maxPercent
					let d_inner: i128 = current
						.percent
						.saturating_sub(target_percent.max)
						.into_inner()
						.unique_saturated_into();
					FixedI128::from_inner(d_inner)
				} else {
					FixedI128::zero()
				};

				// diff = current.percent - target_percent.value
				let diff_percent = if current.percent > target_percent.value {
					let d_inner: i128 = current
						.percent
						.saturating_sub(target_percent.value)
						.into_inner()
						.unique_saturated_into();
					FixedI128::from_inner(d_inner)
				} else {
					let mut d_inner: i128 = target_percent
						.value
						.saturating_sub(current.percent)
						.into_inner()
						.unique_saturated_into();
					d_inner = d_inner.saturating_mul(-1);
					FixedI128::from_inner(d_inner)
				};

				// diff_amount = current.amount - target_amount
				let diff_amount = if current.amount > target_amount {
					current.amount.saturating_sub(target_amount).unique_saturated_into()
				} else {
					let a: i128 = target_amount.saturating_sub(current.amount).unique_saturated_into();
					a.saturating_mul(-1)
				};

				diff.insert(
					*currency_id,
					AllocationDiff {
						current: current.percent,
						target: target_percent.value,
						diff: diff_percent,
						range_diff,
						diff_amount,
					},
				);
			} else {
				// diff_percent = -target_percent.value
				let diff_percent = {
					let d_inner: i128 = target_percent.value.into_inner().unique_saturated_into();
					FixedI128::from_inner(d_inner.saturating_mul(-1))
				};
				// range_diff = -target_percent.min
				let range_diff = {
					let d_inner: i128 = target_percent.min.into_inner().unique_saturated_into();
					FixedI128::from_inner(d_inner.saturating_mul(-1))
				};
				// diff_amount = -target_amount
				let diff_amount = {
					let a: i128 = target_amount.unique_saturated_into();
					a.saturating_mul(-1)
				};
				diff.insert(
					*currency_id,
					AllocationDiff {
						current: FixedU128::zero(),
						target: target_percent.value,
						diff: diff_percent,
						range_diff,
						diff_amount,
					},
				);
			}
		}

		for (currency_id, current) in current_allocations.into_iter() {
			if !target_allocation_percents.contains_key(&currency_id) {
				diff.insert(
					currency_id,
					AllocationDiff {
						current: current.percent,
						target: FixedU128::zero(),
						diff: FixedI128::from_inner(current.percent.into_inner().unique_saturated_into()),
						range_diff: FixedI128::from_inner(current.percent.into_inner().unique_saturated_into()),
						diff_amount: current.amount.unique_saturated_into(),
					},
				);
			}
		}

		Ok(diff)
	}

	// Returns `(current_allocations, current_total_value)` if Ok.
	fn current_allocations() -> Result<(BTreeMap<CurrencyId, CurrentAllocation>, Balance), DispatchError> {
		let currency_ids = Self::target_allocations()
			.keys()
			.cloned()
			.filter(|currency_id| {
				(*currency_id != Token(TokenSymbol::ADAO)) && (*currency_id != Token(TokenSymbol::SDAO))
			})
			.collect::<Vec<CurrencyId>>();

		let mut total_value: Balance = Zero::zero();
		let mut allocations: BTreeMap<CurrencyId, CurrentAllocation> = BTreeMap::new();
		for currency_id in currency_ids.into_iter() {
			let price = T::AssetPriceProvider::get_relative_price(currency_id, T::StableCurrencyId::get())
				.ok_or(Error::<T>::NoPrice)?;
			let amount = T::Currency::total_balance(currency_id, &T::DaoAccount::get());
			let value = price.saturating_mul_int(amount);
			total_value = total_value.saturating_add(value);
			allocations.insert(
				currency_id,
				CurrentAllocation {
					amount,
					value,
					percent: Default::default(),
				},
			);
		}

		// Defensively check if it is zero, should never be in this state
		if total_value.is_zero() {
			return Err(Error::<T>::ZeroTargetAllocation.into());
		}

		allocations.iter_mut().for_each(|(_, allocation)| {
			// Checked that total vaule is not zero above, qed.
			allocation.percent = FixedU128::saturating_from_rational(allocation.value, total_value);
		});

		Ok((allocations, total_value))
	}

	#[transactional]
	fn rebalance(strategy: &Strategy, diff: BTreeMap<CurrencyId, AllocationDiff>) -> DispatchResult {
		match strategy.kind {
			StrategyKind::LiquidityProvisionAusdAdao => Self::rebalance_ausd_adao(strategy, diff),
			StrategyKind::LiquidityProvisionAusdOther(token) => Self::rebalance_ausd_other(strategy, token, diff),
		}
	}

	#[require_transactional]
	fn rebalance_ausd_adao(strategy: &Strategy, diff: BTreeMap<CurrencyId, AllocationDiff>) -> DispatchResult {
		let trading_pair = TradingPair::from_currency_ids(
			CurrencyId::Token(TokenSymbol::AUSD),
			CurrencyId::Token(TokenSymbol::ADAO),
		)
		.ok_or(Error::<T>::InvalidTradingPair)?;
		let lp = trading_pair.dex_share_currency_id();
		let lp_diff = match diff.get(&lp) {
			Some(d) => d,
			None => return Ok(()),
		};
		if lp_diff.range_diff >= FixedI128::zero() {
			return Ok(());
		}

		let max_amount = diff.get(&Token(AUSD)).map(|d| d.diff_amount).unwrap_or_default();
		let amount = strategy.trade_amount(lp_diff.diff_amount, max_amount).saturating_div(2);
		if amount <= 0 {
			return Ok(());
		}

		let adao_price = T::AssetPriceProvider::get_relative_price(Token(ADAO), T::StableCurrencyId::get())
			.ok_or(Error::<T>::NoPrice)?;
		let adao_to_mint = adao_price.saturating_mul_int(amount);
		let pallet_account = Self::account_id();
		T::Currency::deposit(Token(ADAO), &pallet_account, adao_to_mint.unique_saturated_into())?;
		let amount_u128: u128 = amount.unique_saturated_into();
		T::Currency::transfer(Token(AUSD), &T::DaoAccount::get(), &pallet_account, amount_u128)?;
		T::DEX::add_liquidity(
			&pallet_account,
			Token(ADAO),
			Token(AUSD),
			adao_to_mint.unique_saturated_into(),
			amount_u128,
			Zero::zero(),
			false,
		)?;

		let lp_share = T::Currency::free_balance(lp, &pallet_account);
		T::Currency::transfer(lp, &pallet_account, &T::DaoAccount::get(), lp_share)
	}

	#[require_transactional]
	fn rebalance_ausd_other(
		strategy: &Strategy,
		other: TokenSymbol,
		diff: BTreeMap<CurrencyId, AllocationDiff>,
	) -> DispatchResult {
		let trading_pair =
			TradingPair::from_currency_ids(CurrencyId::Token(TokenSymbol::AUSD), CurrencyId::Token(other))
				.ok_or(Error::<T>::InvalidTradingPair)?;
		let lp = trading_pair.dex_share_currency_id();
		let lp_diff = match diff.get(&lp) {
			Some(d) => d,
			None => return Ok(()),
		};
		if lp_diff.range_diff >= FixedI128::zero() {
			return Ok(());
		}

		let other_price = T::AssetPriceProvider::get_relative_price(Token(other), T::StableCurrencyId::get())
			.ok_or(Error::<T>::NoPrice)?;
		let max_other_to_add = T::Currency::free_balance(Token(other), &T::DaoAccount::get());
		let max_other_to_add_amount = other_price.saturating_mul_int(max_other_to_add);

		let max_amount = diff.get(&Token(AUSD)).map(|d| d.diff_amount).unwrap_or_default();
		let amount = strategy
			.trade_amount(
				lp_diff.diff_amount,
				max_amount.min(max_other_to_add_amount.unique_saturated_into()),
			)
			.saturating_div(2);
		let other_to_add = other_price.saturating_mul_int(amount);
		if amount <= 0 || other_to_add <= 0 {
			return Ok(());
		}

		T::DEX::add_liquidity(
			&T::DaoAccount::get(),
			Token(other),
			Token(AUSD),
			other_to_add.unique_saturated_into(),
			amount.unique_saturated_into(),
			Zero::zero(),
			false,
		)?;

		Ok(())
	}
}
