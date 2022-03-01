// This file is part of Acala.

// Copyright (C) 2020-2022 Acala Foundation.
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

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(clippy::unnecessary_cast)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}, dispatch::WeighData};
use sp_std::marker::PhantomData;

/// Weight functions needed for ecocsystem_aqua_staked_token.
pub trait WeightInfo {
	fn on_initialize() -> Weight;
	fn on_initialize_without_inflation() -> Weight;
	fn stake() -> Weight;
	fn unstake() -> Weight;
	fn claim() -> Weight;
	fn update_unstake_fee_rate() -> Weight;
}

/// Weights for ecocsystem_aqua_staked_token using the Acala node and recommended hardware.
pub struct AcalaWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for AcalaWeight<T> {
	fn on_initialize() -> Weight {
		0
	}
	fn on_initialize_without_inflation() -> Weight {
		0
	}
	fn stake() -> Weight {
		0
	}
	fn unstake() -> Weight {
		0
	}
	fn claim() -> Weight {
		0
	}
	fn update_unstake_fee_rate() -> Weight {
		0
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn on_initialize() -> Weight {
		0
	}
	fn on_initialize_without_inflation() -> Weight {
		0
	}
	fn stake() -> Weight {
		0
	}
	fn unstake() -> Weight {
		0
	}
	fn claim() -> Weight {
		0
	}
	fn update_unstake_fee_rate() -> Weight {
		0
	}
}
