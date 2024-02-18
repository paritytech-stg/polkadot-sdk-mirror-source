// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{self as delegated_staking};
use frame_support::{
	assert_ok, derive_impl,
	pallet_prelude::*,
	parameter_types,
	traits::{ConstU64, Currency},
	PalletId,
};

use sp_runtime::{traits::IdentityLookup, BuildStorage, Perbill};

use frame_election_provider_support::{
	bounds::{ElectionBounds, ElectionBoundsBuilder},
	onchain, SequentialPhragmen,
};
use pallet_staking::CurrentEra;
use sp_staking::delegation::{DelegationInterface, StakingDelegationSupport};

pub type T = Runtime;
type Block = frame_system::mocking::MockBlock<Runtime>;
pub type AccountId = u64;

pub const GENESIS_VALIDATOR: AccountId = 1;
pub const GENESIS_NOMINATOR_ONE: AccountId = 101;
pub const GENESIS_NOMINATOR_TWO: AccountId = 102;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<5>;
	type WeightInfo = ();
}

pub type Balance = u128;

parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
}
impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<128>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = ();
}

pallet_staking_reward_curve::build! {
	const I_NPOS: sp_runtime::curve::PiecewiseLinear<'static> = curve!(
		min_inflation: 0_025_000,
		max_inflation: 0_100_000,
		ideal_stake: 0_500_000,
		falloff: 0_050_000,
		max_piece_count: 40,
		test_precision: 0_005_000,
	);
}

parameter_types! {
	pub const RewardCurve: &'static sp_runtime::curve::PiecewiseLinear<'static> = &I_NPOS;
	pub static BondingDuration: u32 = 3;
	pub static ElectionsBoundsOnChain: ElectionBounds = ElectionBoundsBuilder::default().build();
}
pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
	type System = Runtime;
	type Solver = SequentialPhragmen<u64, sp_runtime::Perbill>;
	type DataProvider = Staking;
	type WeightInfo = ();
	type MaxWinners = ConstU32<100>;
	type Bounds = ElectionsBoundsOnChain;
}

impl pallet_staking::Config for Runtime {
	type Currency = Balances;
	type CurrencyBalance = Balance;
	type DelegationSupport = DelegatedStaking;
	type UnixTime = pallet_timestamp::Pallet<Self>;
	type CurrencyToVote = ();
	type RewardRemainder = ();
	type RuntimeEvent = RuntimeEvent;
	type Slash = ();
	type Reward = ();
	type SessionsPerEra = ();
	type SlashDeferDuration = ();
	type AdminOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type BondingDuration = BondingDuration;
	type SessionInterface = ();
	type EraPayout = pallet_staking::ConvertCurve<RewardCurve>;
	type NextNewSession = ();
	type HistoryDepth = ConstU32<84>;
	type MaxExposurePageSize = ConstU32<64>;
	type OffendingValidatorsThreshold = ();
	type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
	type GenesisElectionProvider = Self::ElectionProvider;
	type VoterList = pallet_staking::UseNominatorsAndValidatorsMap<Self>;
	type TargetList = pallet_staking::UseValidatorsMap<Self>;
	type NominationsQuota = pallet_staking::FixedNominationsQuota<16>;
	type MaxUnlockingChunks = ConstU32<32>;
	type MaxControllersInDeprecationBatch = ConstU32<100>;
	type EventListeners = ();
	type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
	type WeightInfo = ();
}

parameter_types! {
	pub const DelegatedStakingPalletId: PalletId = PalletId(*b"py/dlstk");
}
impl delegated_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type PalletId = DelegatedStakingPalletId;
	type Currency = Balances;
	type OnSlash = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type CoreStaking = Staking;
}

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Staking: pallet_staking,
		DelegatedStaking: delegated_staking,
	}
);

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {}
	}
}

impl ExtBuilder {
	fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();
		let mut storage =
			frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		let _ = pallet_balances::GenesisConfig::<T> {
			balances: vec![
				(GENESIS_VALIDATOR, 10000),
				(GENESIS_NOMINATOR_ONE, 1000),
				(GENESIS_NOMINATOR_TWO, 2000),
			],
		}
		.assimilate_storage(&mut storage);

		let stakers = vec![
			(
				GENESIS_VALIDATOR,
				GENESIS_VALIDATOR,
				1000,
				sp_staking::StakerStatus::<AccountId>::Validator,
			),
			(
				GENESIS_NOMINATOR_ONE,
				GENESIS_NOMINATOR_ONE,
				100,
				sp_staking::StakerStatus::<AccountId>::Nominator(vec![1]),
			),
			(
				GENESIS_NOMINATOR_TWO,
				GENESIS_NOMINATOR_TWO,
				200,
				sp_staking::StakerStatus::<AccountId>::Nominator(vec![1]),
			),
		];

		let _ = pallet_staking::GenesisConfig::<T> {
			stakers: stakers.clone(),
			// ideal validator count
			validator_count: 2,
			minimum_validator_count: 1,
			invulnerables: vec![],
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: ExistentialDeposit::get(),
			min_validator_bond: ExistentialDeposit::get(),
			..Default::default()
		}
		.assimilate_storage(&mut storage);

		let mut ext = sp_io::TestExternalities::from(storage);

		ext.execute_with(|| {
			// for events to be deposited.
			frame_system::Pallet::<Runtime>::set_block_number(1);
		});

		ext
	}
	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		sp_tracing::try_init_simple();
		let mut ext = self.build();
		ext.execute_with(test);
		ext.execute_with(|| {
			DelegatedStaking::do_try_state().unwrap();
		});
	}
}

/// fund and return who.
pub(crate) fn fund(who: &AccountId, amount: Balance) {
	let _ = Balances::deposit_creating(who, amount);
}

/// Sets up delegation for passed delegators, returns total delegated amount.
///
/// `delegate_amount` is incremented by the amount `increment` starting with `base_delegate_amount`
/// from lower index to higher index of delegators.
pub(crate) fn setup_delegation_stake(
	delegate: AccountId,
	reward_acc: AccountId,
	delegators: Vec<AccountId>,
	base_delegate_amount: Balance,
	increment: Balance,
) -> Balance {
	fund(&delegate, 100);
	assert_ok!(DelegatedStaking::accept_delegations(&delegate, &reward_acc));
	let mut delegated_amount: Balance = 0;
	for (index, delegator) in delegators.iter().enumerate() {
		let amount_to_delegate = base_delegate_amount + increment * index as Balance;
		delegated_amount += amount_to_delegate;

		fund(delegator, amount_to_delegate + ExistentialDeposit::get());
		assert_ok!(DelegatedStaking::delegate(delegator, &delegate, amount_to_delegate));
		assert_ok!(DelegatedStaking::bond_all(&delegate));
	}

	// sanity checks
	assert_eq!(DelegatedStaking::stakeable_balance(&delegate), delegated_amount);
	assert_eq!(DelegatedStaking::unbonded_balance(&delegate), 0);

	delegated_amount
}

pub(crate) fn start_era(era: sp_staking::EraIndex) {
	CurrentEra::<T>::set(Some(era));
}

pub(crate) fn eq_stake(who: AccountId, total: Balance, active: Balance) -> bool {
	use sp_staking::{Stake, StakingInterface};
	Staking::stake(&who).unwrap() == Stake { total, active }
}