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

use super::pallet;
use cumulus_pallet_parachain_system::{
	self as parachain_system,
	consensus_hook::{ConsensusHook, UnincludedSegmentCapacity},
	relay_state_snapshot::RelayChainStateProof,
};
use frame_support::pallet_prelude::*;
use sp_consensus_slots::{Slot, SlotDuration};
use sp_std::{marker::PhantomData, num::NonZeroU32};

/// A consensus hook for a fixed block processing velocity and unincluded segment capacity.
///
/// Relay chain slot duration must be provided in milliseconds.
pub struct NimbusVelocityConsensusHook<
	T,
	const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
	const V: u32,
	const C: u32,
>(PhantomData<T>);

impl<
		T: pallet::Config,
		const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
		const V: u32,
		const C: u32,
	> ConsensusHook for NimbusVelocityConsensusHook<T, RELAY_CHAIN_SLOT_DURATION_MILLIS, V, C>
{
	// Validates the number of authored blocks within the slot with respect to the `V + 1` limit.
	fn on_state_proof(state_proof: &RelayChainStateProof) -> (Weight, UnincludedSegmentCapacity) {
		// Ensure velocity is non-zero.
		let velocity = V.max(1);
		let relay_chain_slot = state_proof
			.read_slot()
			.expect("failed to read relay chain slot");

		// Since the slot is populated on an inherent, it can happen that is not present in the first block
		// in that case take default
		let (slot, authored) = pallet::Pallet::<T>::get_highest_slot_info().unwrap_or_default();

		let slot = u64::from(slot);

/* 		// Calculate the current relay slot in millis.
		let relay_chain_timestamp =
			u64::from(RELAY_CHAIN_SLOT_DURATION_MILLIS).saturating_mul(*relay_chain_slot);

		// Fetch the slot duration from parachain's config
		let para_slot_duration =
			SlotDuration::from_millis(pallet::Pallet::<T>::get_slot_duration());
		let para_slot_from_relay =
			Slot::from_timestamp(relay_chain_timestamp.into(), para_slot_duration);

		// Check that the slot fetched from storage is the same as the calculated using relay timestamp
		// Only check this if the slot is non-zero
		if slot != 0u64 {
			assert_eq!(
				Slot::from(slot),
				para_slot_from_relay,
				"slot number mismatch"
			);
		} */

		if authored > velocity + 1 {
			panic!("authored blocks limit is reached for the slot")
		}
		let weight = T::DbWeight::get().reads(1);

		(
			weight,
			NonZeroU32::new(sp_std::cmp::max(C, 1))
				.expect("1 is the minimum value and non-zero; qed")
				.into(),
		)
	}
}

impl<
		T: parachain_system::Config + pallet::Config,
		const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
		const V: u32,
		const C: u32,
	> NimbusVelocityConsensusHook<T, RELAY_CHAIN_SLOT_DURATION_MILLIS, V, C>
{
	/// Whether it is legal to extend the chain, assuming the given block is the most
	/// recently included one as-of the relay parent that will be built against, and
	/// the given slot.
	///
	/// This should be consistent with the logic the runtime uses when validating blocks to
	/// avoid issues.
	///
	/// When the unincluded segment is empty, i.e. `included_hash == at`, where at is the block
	/// whose state we are querying against, this must always return `true` as long as the slot
	/// is more recent than the included block itself.
	pub fn can_build_upon(included_hash: T::Hash, new_slot: u64) -> bool {
		let velocity = V.max(1);
		let (last_slot, authored_so_far) = match pallet::Pallet::<T>::get_highest_slot_info() {
			None => return true,
			Some(x) => x,
		};

		let size_after_included =
			parachain_system::Pallet::<T>::unincluded_segment_size_after(included_hash);

		// can never author when the unincluded segment is full.
		if size_after_included >= C {
			return false;
		}

		if u64::from(last_slot) == new_slot {
			authored_so_far < velocity + 1
		} else {
			// disallow slot from moving backwards.
			u64::from(last_slot) < new_slot
		}
	}
}
