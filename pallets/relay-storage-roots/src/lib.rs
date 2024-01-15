// Copyright Moonsong Labs
// This file is part of Moonkit.

// Moonkit is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonkit is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonkit.  If not, see <http://www.gnu.org/licenses/>.

//! # Relay Storage Roots Pallet
//!
//! This pallet stores the latest `MaxStorageRoots` relay storage roots, which can be used to
//! verify state proofs against an old state of the relay chain.
//!
//! This is useful when the proof needs to be generated by an end user, because
//! by the time their transaction is included in a block, the latest relay
//! block will probably have changed and therefore the proof will be invalid.
//! To avoid that, we expect the user to generate a proof against the latest relay block stored
//! in this pallet. This proof will then be valid as long as that relay block is not removed from
//! here.
//!
//! This pallet SHOULD NOT be used for data that can change quickly, because we allow the user to
//! submit a proof of an old state. Therefore a valid proof does not imply that the current relay
//! state is the expected one.
//!
//! ### Design
//!
//! The relay storage roots are inserted in the `on_finalize` hook, so the storage root of the
//! current relay block will not be available in the mapping until the next block, but it can be
//! read using the `RelaychainStateProvider` at any point after the `on_initialize` of
//! `cumulus_pallet_parachain_system`.
//!
//! One storage root is inserted per parachain block, but there may be more than one relay block in
//! between two parachain blocks. In that case, there will be a gap in the `RelayStorageRoot`
//! mapping. When creating a proof, users should ensure that they are using the latest storage root
//! available in the mapping, otherwise it may not be possible to validate their proof.
//!
//! The `RelayStorageRoot` mapping is bounded by `MaxStorageRoots`. To ensure that oldest storage
//! roots are removed first, there is an additional `RelayStorageRootKeys` storage item that stores
//! a sorted list of all the keys. This is needed because it is not possible to iterate over a
//! mapping in order (unless if using an `Identity` hash). The `MaxStorageRoots` limit applies to
//! the number of items, not to their age.

#![cfg_attr(not(feature = "std"), no_std)]

pub use crate::weights::WeightInfo;
use cumulus_pallet_parachain_system::RelaychainStateProvider;
use cumulus_primitives_core::relay_chain::BlockNumber as RelayBlockNumber;
use frame_support::pallet;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_core::Get;
use sp_core::H256;
use sp_std::collections::vec_deque::VecDeque;

#[cfg(any(test, feature = "runtime-benchmarks"))]
mod benchmarks;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RelaychainStateProvider: RelaychainStateProvider;
		#[pallet::constant]
		type MaxStorageRoots: Get<u32>;
		/// Weight info
		type WeightInfo: WeightInfo;
	}

	/// Map of relay block number to relay storage root
	#[pallet::storage]
	pub type RelayStorageRoot<T: Config> =
		StorageMap<_, Twox64Concat, RelayBlockNumber, H256, OptionQuery>;

	/// List of all the keys in `RelayStorageRoot`.
	/// Used to remove the oldest key without having to iterate over all of them.
	#[pallet::storage]
	pub type RelayStorageRootKeys<T: Config> =
		StorageValue<_, BoundedVec<RelayBlockNumber, T::MaxStorageRoots>, ValueQuery>;

	impl<T: Config> Pallet<T> {
		/// Populates `RelayStorageRoot` using `RelaychainStateProvider`.
		pub fn set_relay_storage_root() {
			let relay_state = T::RelaychainStateProvider::current_relay_chain_state();

			// If this relay block number has already been stored, skip it.
			if <RelayStorageRoot<T>>::contains_key(relay_state.number) {
				return;
			}

			<RelayStorageRoot<T>>::insert(relay_state.number, relay_state.state_root);
			let mut keys: VecDeque<_> = <RelayStorageRootKeys<T>>::get().into_inner().into();
			keys.push_back(relay_state.number);
			// Delete the oldest stored root if the total number is greater than MaxStorageRoots
			if u32::try_from(keys.len()).unwrap() > T::MaxStorageRoots::get() {
				let first_key = keys.pop_front().unwrap();
				<RelayStorageRoot<T>>::remove(first_key);
			}

			// If `MaxStorageRoots` has decreased, we need to delete more than one storage root.
			// But that would make this function have an unbounded amount of writes. So instead, we
			// will only delete the first one, and leak the rest.
			let keys = BoundedVec::truncate_from(keys.into());
			<RelayStorageRootKeys<T>>::put(keys);
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			// Account for weight used in on_finalize
			T::WeightInfo::set_relay_storage_root()
		}
		fn on_finalize(_now: BlockNumberFor<T>) {
			Self::set_relay_storage_root();
		}
	}
}
