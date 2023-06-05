// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Everything required to run benchmarks of messages module, based on
//! `bridge_runtime_common::messages` implementation.

#![cfg(feature = "runtime-benchmarks")]

// use crate::{
// 	messages::{BridgedChain, HashOf, ThisChain},
// 	messages_generation::{
// 		encode_all_messages, encode_lane_data, prepare_message_delivery_storage_proof,
// 		prepare_messages_storage_proof,
// 	},
// };

use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, MessagePayload,
};
use bp_polkadot_core::parachains::ParaHash;
use bp_runtime::{Chain, Parachain, StorageProofSize};
// TODO: https://github.com/paritytech/parity-bridges-common/issues/1666 - merge with message
// generation methods from messages pallet and use it here

use bp_messages::{storage_keys, ChainWithMessages};
use bp_runtime::{
	grow_trie_leaf_value, record_all_trie_keys, AccountIdOf, HashOf, HasherOf, UntrustedVecDb,
};
use codec::Encode;
use frame_support::weights::Weight;
use pallet_bridge_messages::{
	benchmarking::{MessageDeliveryProofParams, MessageProofParams},
	messages_generation::{encode_all_messages, encode_lane_data, prepare_messages_storage_proof},
	BridgedChainOf, ThisChainOf,
};
use sp_runtime::{
	traits::{Header, Zero},
	StateVersion,
};
use sp_std::prelude::*;
use sp_trie::{
	LayoutV0, LayoutV1, MemoryDB, StorageProof, TrieConfiguration, TrieDBMutBuilder, TrieMut,
};
use xcm::latest::prelude::*;

/// Prepare inbound bridge message according to given message proof parameters.
fn prepare_inbound_message(
	params: &MessageProofParams,
	successful_dispatch_message_generator: impl Fn(usize) -> MessagePayload,
) -> MessagePayload {
	// we only care about **this** message size when message proof needs to be `Minimal`
	let expected_size = match params.size {
		StorageProofSize::Minimal(size) => size as usize,
		_ => 0,
	};

	// if we don't need a correct message, then we may just return some random blob
	if !params.is_successful_dispatch_expected {
		return vec![0u8; expected_size]
	}

	// else let's prepare successful message.
	let msg = successful_dispatch_message_generator(expected_size);
	assert!(
		msg.len() >= expected_size,
		"msg.len(): {} does not match expected_size: {}",
		expected_size,
		msg.len()
	);
	msg
}

/// Prepare proof of messages for the `receive_messages_proof` call.
///
/// In addition to returning valid messages proof, environment is prepared to verify this message
/// proof.
///
/// This method is intended to be used when benchmarking pallet, linked to the chain that
/// uses GRANDPA finality. For parachains, please use the `prepare_message_proof_from_parachain`
/// function.
pub fn prepare_message_proof_from_grandpa_chain<R, FI, MI>(
	params: MessageProofParams,
	message_generator: impl Fn(usize) -> MessagePayload,
) -> (FromBridgedChainMessagesProof<HashOf<BridgedChainOf<R, MI>>>, Weight)
where
	R: pallet_bridge_grandpa::Config<FI, BridgedChain = BridgedChainOf<R, MI>>
		+ pallet_bridge_messages::Config<
			MI,
			BridgedHeaderChain = pallet_bridge_grandpa::Pallet<R, FI>,
		>,
	FI: 'static,
	MI: 'static,
{
	// prepare storage proof
	let (state_root, storage) =
		prepare_messages_storage_proof::<BridgedChainOf<R, MI>, ThisChainOf<R, MI>>(
			params.lane,
			params.message_nonces.clone(),
			params.outbound_lane_data.clone(),
			params.size,
			|_| prepare_inbound_message(&params, &message_generator),
			encode_all_messages,
			encode_lane_data,
			false,
			false,
		);

	// update runtime storage
	let (_, bridged_header_hash) = insert_header_to_grandpa_pallet::<R, FI>(state_root);

	(
		FromBridgedChainMessagesProof {
			bridged_header_hash,
			storage,
			lane: params.lane,
			nonces_start: *params.message_nonces.start(),
			nonces_end: *params.message_nonces.end(),
		},
		Weight::MAX / 1000,
	)
}

/// Prepare proof of messages for the `receive_messages_proof` call.
///
/// In addition to returning valid messages proof, environment is prepared to verify this message
/// proof.
///
/// This method is intended to be used when benchmarking pallet, linked to the chain that
/// uses parachain finality. For GRANDPA chains, please use the
/// `prepare_message_proof_from_grandpa_chain` function.
pub fn prepare_message_proof_from_parachain<R, PI, MI>(
	params: MessageProofParams,
	message_generator: impl Fn(usize) -> MessagePayload,
) -> (FromBridgedChainMessagesProof<HashOf<BridgedChainOf<R, MI>>>, Weight)
where
	R: pallet_bridge_parachains::Config<PI> + pallet_bridge_messages::Config<MI>,
	PI: 'static,
	MI: 'static,
	BridgedChainOf<R, MI>: Chain<Hash = ParaHash> + Parachain,
{
	// prepare storage proof
	let (state_root, storage) =
		prepare_messages_storage_proof::<BridgedChainOf<R, MI>, ThisChainOf<R, MI>>(
			params.lane,
			params.message_nonces.clone(),
			params.outbound_lane_data.clone(),
			params.size,
			|_| prepare_inbound_message(&params, &message_generator),
			encode_all_messages,
			encode_lane_data,
			false,
			false,
		);

	// update runtime storage
	let (_, bridged_header_hash) =
		insert_header_to_parachains_pallet::<R, PI, BridgedChainOf<R, MI>>(state_root);

	(
		FromBridgedChainMessagesProof {
			bridged_header_hash,
			storage,
			lane: params.lane,
			nonces_start: *params.message_nonces.start(),
			nonces_end: *params.message_nonces.end(),
		},
		Weight::MAX / 1000,
	)
}

/// Prepare proof of messages delivery for the `receive_messages_delivery_proof` call.
///
/// This method is intended to be used when benchmarking pallet, linked to the chain that
/// uses GRANDPA finality. For parachains, please use the
/// `prepare_message_delivery_proof_from_parachain` function.
pub fn prepare_message_delivery_proof_from_grandpa_chain<R, FI, MI>(
	params: MessageDeliveryProofParams<AccountIdOf<ThisChainOf<R, MI>>>,
) -> FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChainOf<R, MI>>>
where
	R: pallet_bridge_grandpa::Config<FI, BridgedChain = BridgedChainOf<R, MI>>
		+ pallet_bridge_messages::Config<
			MI,
			BridgedHeaderChain = pallet_bridge_grandpa::Pallet<R, FI>,
		>,
	FI: 'static,
	MI: 'static,
{
	// prepare storage proof
	let lane = params.lane;
	let (state_root, storage_proof) = prepare_message_delivery_proof::<R, MI>(params);

	// update runtime storage
	let (_, bridged_header_hash) = insert_header_to_grandpa_pallet::<R, FI>(state_root);

	FromBridgedChainMessagesDeliveryProof {
		bridged_header_hash: bridged_header_hash.into(),
		storage_proof,
		lane,
	}
}

/// Prepare proof of messages delivery for the `receive_messages_delivery_proof` call.
///
/// This method is intended to be used when benchmarking pallet, linked to the chain that
/// uses parachain finality. For GRANDPA chains, please use the
/// `prepare_message_delivery_proof_from_grandpa_chain` function.
pub fn prepare_message_delivery_proof_from_parachain<R, PI, MI>(
	params: MessageDeliveryProofParams<AccountIdOf<ThisChainOf<R, MI>>>,
) -> FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChainOf<R, MI>>>
where
	R: pallet_bridge_parachains::Config<PI> + pallet_bridge_messages::Config<MI>,
	PI: 'static,
	MI: 'static,
	BridgedChainOf<R, MI>: Chain<Hash = ParaHash> + Parachain,
{
	// prepare storage proof
	let lane = params.lane;
	let (state_root, storage_proof) = prepare_message_delivery_proof::<R, MI>(params);

	// update runtime storage
	let (_, bridged_header_hash) =
		insert_header_to_parachains_pallet::<R, PI, BridgedChainOf<R, MI>>(state_root);

	FromBridgedChainMessagesDeliveryProof {
		bridged_header_hash: bridged_header_hash.into(),
		storage_proof,
		lane,
	}
}

/// Prepare in-memory message delivery proof, without inserting anything to the runtime storage.
fn prepare_message_delivery_proof<R, MI>(
	params: MessageDeliveryProofParams<AccountIdOf<ThisChainOf<R, MI>>>,
) -> (HashOf<BridgedChainOf<R, MI>>, UntrustedVecDb)
where
	R: pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	match BridgedChainOf::<R, MI>::STATE_VERSION {
		StateVersion::V0 =>
			do_prepare_message_delivery_proof::<R, MI, LayoutV0<HasherOf<BridgedChainOf<R, MI>>>>(
				params,
			),
		StateVersion::V1 =>
			do_prepare_message_delivery_proof::<R, MI, LayoutV1<HasherOf<BridgedChainOf<R, MI>>>>(
				params,
			),
	}
}

/// Prepare in-memory message delivery proof, without inserting anything to the runtime storage.
fn do_prepare_message_delivery_proof<
	R,
	MI,
	L: TrieConfiguration<Hash = HasherOf<BridgedChainOf<R, MI>>>,
>(
	params: MessageDeliveryProofParams<AccountIdOf<ThisChainOf<R, MI>>>,
) -> (HashOf<BridgedChainOf<R, MI>>, UntrustedVecDb)
where
	R: pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	// prepare Bridged chain storage with inbound lane state
	let storage_key = storage_keys::inbound_lane_data_key(
		R::ThisChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
		&params.lane,
	)
	.0;
	let mut root = Default::default();
	let mut mdb = MemoryDB::default();
	{
		let mut trie = TrieDBMutBuilder::<L>::new(&mut mdb, &mut root).build();
		let inbound_lane_data =
			grow_trie_leaf_value(params.inbound_lane_data.encode(), params.size);
		trie.insert(&storage_key, &inbound_lane_data)
			.map_err(|_| "TrieMut::insert has failed")
			.expect("TrieMut::insert should not fail in benchmarks");
	}

	// generate storage proof to be delivered to This chain
	let read_proof = record_all_trie_keys::<L, _>(&mdb, &root)
		.map_err(|_| "record_all_trie_keys has failed")
		.expect("record_all_trie_keys should not fail in benchmarks");
	let storage_proof = UntrustedVecDb::try_new::<HasherOf<BridgedChainOf<R, MI>>>(
		StorageProof::new(read_proof),
		root,
		vec![storage_key],
	)
	.unwrap();

	(root, storage_proof)
}

/// Insert header to the bridge GRANDPA pallet.
pub(crate) fn insert_header_to_grandpa_pallet<R, GI>(
	state_root: bp_runtime::HashOf<R::BridgedChain>,
) -> (bp_runtime::BlockNumberOf<R::BridgedChain>, bp_runtime::HashOf<R::BridgedChain>)
where
	R: pallet_bridge_grandpa::Config<GI>,
	GI: 'static,
	R::BridgedChain: bp_runtime::Chain,
{
	let bridged_block_number = Zero::zero();
	let bridged_header = bp_runtime::HeaderOf::<R::BridgedChain>::new(
		bridged_block_number,
		Default::default(),
		state_root,
		Default::default(),
		Default::default(),
	);
	let bridged_header_hash = bridged_header.hash();
	pallet_bridge_grandpa::initialize_for_benchmarks::<R, GI>(bridged_header);
	(bridged_block_number, bridged_header_hash)
}

/// Insert header to the bridge parachains pallet.
pub(crate) fn insert_header_to_parachains_pallet<R, PI, PC>(
	state_root: bp_runtime::HashOf<PC>,
) -> (bp_runtime::BlockNumberOf<PC>, bp_runtime::HashOf<PC>)
where
	R: pallet_bridge_parachains::Config<PI>,
	PI: 'static,
	PC: Chain<Hash = ParaHash> + Parachain,
{
	let bridged_block_number = Zero::zero();
	let bridged_header = bp_runtime::HeaderOf::<PC>::new(
		bridged_block_number,
		Default::default(),
		state_root,
		Default::default(),
		Default::default(),
	);
	let bridged_header_hash = bridged_header.hash();
	pallet_bridge_parachains::initialize_for_benchmarks::<R, PI, PC>(bridged_header);
	(bridged_block_number, bridged_header_hash)
}

/// Returns callback which generates `BridgeMessage` from Polkadot XCM builder based on
/// `expected_message_size` for benchmark.
pub fn generate_xcm_builder_bridge_message_sample(
	destination: InteriorLocation,
) -> impl Fn(usize) -> MessagePayload {
	move |expected_message_size| -> MessagePayload {
		// For XCM bridge hubs, it is the message that
		// will be pushed further to some XCM queue (XCMP/UMP)
		let location = xcm::VersionedInteriorLocation::from(destination.clone());
		let location_encoded_size = location.encoded_size();

		// we don't need to be super-precise with `expected_size` here
		let xcm_size = expected_message_size.saturating_sub(location_encoded_size);
		let xcm_data_size = xcm_size.saturating_sub(
			// minus empty instruction size
			Instruction::<()>::ExpectPallet {
				index: 0,
				name: vec![],
				module_name: vec![],
				crate_major: 0,
				min_crate_minor: 0,
			}
			.encoded_size(),
		);

		log::trace!(
			target: "runtime::bridge-benchmarks",
			"generate_xcm_builder_bridge_message_sample with expected_message_size: {}, location_encoded_size: {}, xcm_size: {}, xcm_data_size: {}",
			expected_message_size, location_encoded_size, xcm_size, xcm_data_size,
		);

		let xcm = xcm::VersionedXcm::<()>::from(Xcm(vec![Instruction::<()>::ExpectPallet {
			index: 0,
			name: vec![42; xcm_data_size],
			module_name: vec![],
			crate_major: 0,
			min_crate_minor: 0,
		}]));

		// this is the `BridgeMessage` from polkadot xcm builder, but it has no constructor
		// or public fields, so just tuple
		// (double encoding, because `.encode()` is called on original Xcm BLOB when it is pushed
		// to the storage)
		(location, xcm).encode().encode()
	}
}
