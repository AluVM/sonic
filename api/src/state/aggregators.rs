// SONIC: Standard library for formally-verifiable distributed contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 Laboratories for Ubiquitous Deterministic Computing (UBIDECO),
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2019-2025 Dr Maxim Orlovsky.
// All rights under the above copyrights are reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use std::collections::BTreeMap;

use aluvm::{Lib, LibId, LibSite};
use amplify::confinement::{TinyBlob, TinyOrdSet};
use sonic_callreq::StateName;
use strict_encoding::StrictDumb;
use strict_types::value::StrictNum;
use strict_types::StrictVal;

use crate::{StateAtom, LIB_NAME_SONIC};

/// A set of pre-defined state aggregators (see [`crate::Api::aggregators`].
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Count(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateAggregator {
    /// The aggregated state should have a unit value.
    ///
    /// This is useful when an interface requires some aggregated state to be present,
    /// but you can't
    #[strict_type(tag = 0)]
    Unit,

    /// Wrap an existing state with an `Option::Some`.
    ///
    /// This is useful when an interface requires a value to be optional.
    #[strict_type(tag = 1)]
    ToSome(StateName),

    /// Unwraps an optional value.
    ///
    /// If the value is `None`, sets the state to the provided constant.
    #[strict_type(tag = 2)]
    UnwrapOr(StateName, TinyBlob),

    /// Provide aggregated state as a constant value.
    #[strict_type(tag = 3)]
    Const(TinyBlob),

    /// Count the number of elements in the state of a certain type.
    #[strict_type(tag = 0x10)]
    Count(StateName),

    /// Sum over the elements of a state of a certain type.
    ///
    /// If any of the elements of the state are not integers, returns `None`.
    #[strict_type(tag = 0x20)]
    Sum(StateName, StateName),

    #[strict_type(tag = 0x22)]
    Diff(StateName, StateName),

    /// Convert a verified state under the same state type into a vector.
    #[strict_type(tag = 0x30)]
    ListV(StateName),

    /// Convert a verified state under the same state type into a sorted set.
    #[strict_type(tag = 0x31)]
    SetV(StateName),

    /// Map from a field-based element state to a non-verifiable structured state
    #[strict_type(tag = 0x32)]
    MapV2U(StateName),

    /// Sum over verifiable field-element-based part of state.
    ///
    /// If any of the verifiable state is absent or not in the form of unsigned integer,
    /// it is treated as zero.
    #[strict_type(tag = 0x40)]
    SumV(StateName),

    /// Sum over verifiable field-element-based part of state.
    ///
    /// If any of the verifiable state is absent or not in the form of unsigned integer,
    /// sets the aggregated state to `None`.
    #[strict_type(tag = 0x41)]
    TrySumV(StateName),

    /// Execute a custom function on the state.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
        /// The aggregated state which must be computed when the script is called.
        TinyOrdSet<StateName>,
    ),
}

impl StateAggregator {
    /// Compute state via applying some aggregator function.
    ///
    /// # Returns
    ///
    /// Aggregated state value. If the computing fails due to any exception, `None`.
    pub fn aggregate<'libs, I: IntoIterator<Item = StateAtom>>(
        &self,
        state: impl Fn(&StateName) -> I,
        libs: impl IntoIterator<Item = &'libs Lib>,
    ) -> Option<StrictVal> {
        Some(match self {
            StateAggregator::Unit => StrictVal::Unit,
            //EmbeddedReaders::Const(val) => val.clone(),
            StateAggregator::Count(name) => {
                let count = state(name).into_iter().count();
                svnum!(count as u64)
            }
            StateAggregator::SumV(name) => {
                let sum = state(name)
                    .into_iter()
                    .map(|atom| match &atom.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => *val,
                        _ => 0u64,
                    })
                    .sum::<u64>();
                svnum!(sum)
            }
            StateAggregator::ListV(name) => StrictVal::List(
                state(name)
                    .into_iter()
                    .map(|atom| atom.verified.clone())
                    .collect(),
            ),
            StateAggregator::SetV(name) => {
                let mut set = Vec::new();
                for atom in state(name) {
                    if !set.contains(&atom.verified) {
                        set.push(atom.verified.clone());
                    }
                }
                StrictVal::Set(set)
            }
            StateAggregator::MapV2U(name) => {
                let mut map = Vec::new();
                for atom in state(name) {
                    let Some(val) = &atom.unverified else { continue };
                    if map.iter().any(|(key, _)| &atom.verified == key) {
                        continue;
                    }
                    map.push((atom.verified.clone(), val.clone()));
                }
                StrictVal::Map(map)
            }
            StateAggregator::Const(_) => todo!(),
            StateAggregator::Sum(_, _) => todo!(),
            StateAggregator::Diff(_, _) => todo!(),
            StateAggregator::ToSome(_) => todo!(),
            StateAggregator::UnwrapOr(_, _) => todo!(),
            StateAggregator::TrySumV(_) => todo!(),

            StateAggregator::AluVM(entry, _) => {
                let libs = libs
                    .into_iter()
                    .map(|lib| (lib.lib_id(), lib))
                    .collect::<BTreeMap<_, _>>();
                let mut vm = aluvm::Vm::<aluvm::isa::Instr<LibId>>::new();
                // For now, we ignore all computations and return `None` anyway.
                // This leaves a way to add proper VM computing in the future
                // in a backward-compatible way.
                let _status = vm.exec(*entry, &(), |id| libs.get(&id));
                return None;
            }
        })
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]
    use super::*;

    #[test]
    fn verified_readers() {
        let state = [
            StateAtom::new_verified(5u64),
            StateAtom::new_verified(1u64),
            StateAtom::new_verified(2u64),
            StateAtom::new_verified(3u64),
            StateAtom::new_verified(4u64),
            StateAtom::new_verified(5u64),
        ];

        let adaptor = StateAggregator::Count(vname!("test1"));
        assert_eq!(
            adaptor
                .aggregate(
                    |name| {
                        assert_eq!(name.as_str(), "test1");
                        state.clone().into_iter()
                    },
                    None
                )
                .unwrap(),
            svnum!(6u64)
        );

        let adaptor = StateAggregator::SumV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svnum!(5u64 + 1 + 2 + 3 + 4 + 5)
        );

        let adaptor = StateAggregator::ListV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svlist!([5u64, 1u64, 2u64, 3u64, 4u64, 5u64])
        );

        let adaptor = StateAggregator::SetV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svset!([5u64, 1u64, 2u64, 3u64, 4u64])
        );

        let adaptor = StateAggregator::MapV2U(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            StrictVal::Map(none!())
        );
    }

    #[test]
    fn unverified_readers() {
        let state = [
            StateAtom::new_unverified(5u64),
            StateAtom::new_unverified(1u64),
            StateAtom::new_unverified(2u64),
            StateAtom::new_unverified(3u64),
            StateAtom::new_unverified(4u64),
            StateAtom::new_unverified(5u64),
        ];

        let adaptor = StateAggregator::Count(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svnum!(6u64)
        );

        let adaptor = StateAggregator::SumV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svnum!(0u64)
        );

        let adaptor = StateAggregator::ListV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svlist!([(), (), (), (), (), ()])
        );

        let adaptor = StateAggregator::SetV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svset!([()])
        );

        let adaptor = StateAggregator::MapV2U(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            StrictVal::Map(vec![(StrictVal::Unit, svnum!(5u64)),])
        );
    }

    #[test]
    fn pair_readers() {
        let state = [
            StateAtom::with(5u64, "state 1"),
            StateAtom::with(1u64, "state 2"),
            StateAtom::with(2u64, "state 3"),
            StateAtom::with(3u64, "state 4"),
            StateAtom::with(4u64, "state 5"),
            StateAtom::with(5u64, "state 6"),
        ];

        let adaptor = StateAggregator::Count(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svnum!(6u64)
        );

        let adaptor = StateAggregator::SumV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svnum!(5u64 + 1 + 2 + 3 + 4 + 5)
        );

        let adaptor = StateAggregator::ListV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svlist!([5u64, 1u64, 2u64, 3u64, 4u64, 5u64])
        );

        let adaptor = StateAggregator::SetV(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            svset!([5u64, 1u64, 2u64, 3u64, 4u64])
        );

        let adaptor = StateAggregator::MapV2U(vname!("test"));
        assert_eq!(
            adaptor
                .aggregate(|_| { state.clone().into_iter() }, None)
                .unwrap(),
            StrictVal::Map(vec![
                (svnum!(5u64), svstr!("state 1")),
                (svnum!(1u64), svstr!("state 2")),
                (svnum!(2u64), svstr!("state 3")),
                (svnum!(3u64), svstr!("state 4")),
                (svnum!(4u64), svstr!("state 5"))
            ])
        );
    }
}
