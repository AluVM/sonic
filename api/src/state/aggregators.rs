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

use alloc::collections::BTreeMap;

use aluvm::{Lib, LibId, LibSite};
use amplify::confinement::TinyBlob;
use indexmap::IndexMap;
use sonic_callreq::StateName;
use strict_encoding::StrictDumb;
use strict_types::value::{EnumTag, StrictNum};
use strict_types::{SemId, StrictVal, TypeSystem};
use ultrasonic::CellAddr;

use crate::{StateAtom, LIB_NAME_SONIC};

/// Structure which allows applying aggregators either to a global or a different aggregated
/// state.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Aggregated(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateSelector {
    #[strict_type(tag = 0)]
    Global(
        StateName,
        /** Flag indicating that if multiple state elements are known, only the first one should
         * be used. */
        bool,
    ),
    #[strict_type(tag = 1)]
    Aggregated(StateName),
}

/// A set of pre-defined top-level state aggregators (see [`crate::Api::aggregators`].
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Some(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum Aggregator {
    /// Produces constant value of `Option::None` type.
    #[strict_type(tag = 0)]
    None,

    /// Wrap into an optional value.
    ///
    /// If the underlying aggregated state fails, sets the aggregated state to `None`.
    #[strict_type(tag = 1)]
    #[cfg_attr(feature = "serde", serde(with = "serde_yaml::with::singleton_map"))]
    Some(SubAggregator),

    /// Takes the underlying aggregated state and applies nothing on top.
    ///
    /// If the underlying aggregator fails, the aggregated state is not produced.
    #[strict_type(tag = 2)]
    // https://github.com/dtolnay/serde-yaml/issues/363
    // We should repeat this if we encounter any other nested enums.
    #[cfg_attr(feature = "serde", serde(with = "serde_yaml::with::singleton_map"))]
    Take(SubAggregator),

    /// If the underlying aggregated state fails, returns the provided constant value.
    #[strict_type(tag = 3)]
    Or(#[cfg_attr(feature = "serde", serde(with = "serde_yaml::with::singleton_map"))] SubAggregator, SemId, TinyBlob),

    /// Execute a custom function on the state.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl Aggregator {
    /// Returns names of the other computed state which this aggregator depends on
    /// and which needs to be computed before running this aggregator.
    pub fn depends_on(&self) -> impl Iterator<Item = &StateName> {
        match self {
            Self::Take(sub) | Self::Some(sub) | Self::Or(sub, _, _) => sub.depends_on(),
            Self::None | Self::AluVM(_) => vec![],
        }
        .into_iter()
    }

    /// Compute state via applying some aggregator function.
    ///
    /// # Returns
    ///
    /// Aggregated state value. If the computing fails due to any exception, `None`.
    pub fn aggregate<'libs>(
        &self,
        global: &BTreeMap<StateName, BTreeMap<CellAddr, StateAtom>>,
        aggregated: &BTreeMap<StateName, StrictVal>,
        libs: impl IntoIterator<Item = &'libs Lib>,
        types: &TypeSystem,
    ) -> Option<StrictVal> {
        match self {
            Self::None => Some(StrictVal::none()),

            Self::Take(sub) => sub.aggregate(global, aggregated, types),

            Self::Some(sub) => Some(match sub.aggregate(global, aggregated, types) {
                Some(val) => StrictVal::some(val),
                None => StrictVal::none(),
            }),

            Self::Or(sub, sem_id, val) => sub
                .aggregate(global, aggregated, types)
                .or_else(|| deserialize(*sem_id, val, types)),

            Self::AluVM(entry) => {
                let libs = libs
                    .into_iter()
                    .map(|lib| (lib.lib_id(), lib))
                    .collect::<IndexMap<_, _>>();
                let mut vm = aluvm::Vm::<aluvm::isa::Instr<LibId>>::new();
                // For now, we ignore all computations and return `None` anyway.
                // This leaves a way to add proper VM computing in the future
                // in a backward-compatible way.
                let _status = vm.exec(*entry, &(), |id| libs.get(&id));
                None
            }
        }
    }
}

/// A set of pre-defined state sub-aggregators (see [`crate::Api::aggregators`].
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Neg(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum SubAggregator {
    /// The aggregated state is generated with a predefined constant value.
    ///
    /// To produce a state with a unit value, use `Self::Const(SemId::unit(), none!())`.
    #[strict_type(tag = 0)]
    Const(SemId, TinyBlob),

    /// Takes the only element of the global state.
    ///
    /// Fails if the state is not defined or has more than one defined element.
    #[strict_type(tag = 1)]
    TheOnly(StateName),

    /// Takes some other aggregated and copies it.
    ///
    /// Fails if the other aggregated state is not defined.
    #[strict_type(tag = 2)]
    Copy(StateName),

    /// Unwraps an optional value.
    ///
    /// Fails if the value is `None`, is not defined, multiple, or not an optional.
    #[strict_type(tag = 3)]
    Unwrap(StateName),

    /// Takes the first element of the global state.
    ///
    /// Fails if the global state is not defined, i.e., has zero elements.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 4)]
    First(StateName),

    /// Takes the nth element of the global state.
    ///
    /// Fails if the global state is not defined, i.e., has zero elements,
    /// or if the nth-element is empty.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 5)]
    Nth(StateName, u16),

    /// Takes the last element of the global state.
    ///
    /// Fails if the global state is not defined, i.e., has zero elements.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 6)]
    Last(StateName),

    /// Takes the nth element of the global state, counting from the end of the list.
    ///
    /// Fails if the global state is not defined, i.e., has zero elements,
    /// or if the nth-element is empty.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 7)]
    NthBack(StateName, u16),

    /// Integer-negate state.
    ///
    /// Fails if the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer or is greater than `i64::MAX`.
    #[strict_type(tag = 0x10)]
    Neg(StateSelector),

    /// Sum two states of different types, expecting them to be integers.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer or there is an overflow.
    #[strict_type(tag = 0x11)]
    Add(StateSelector, StateSelector),

    /// Substracts the second state from the first state, expecting both to be integers.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer or there is an overflow.
    #[strict_type(tag = 0x12)]
    Sub(StateSelector, StateSelector),

    /// Product two states of different types, expecting them to be integers.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer or there is an overflow.
    #[strict_type(tag = 0x13)]
    Mul(StateSelector, StateSelector),

    /// Divide the first state on the second state, expecting them to be integers.
    /// The resulting value is rounded towards zero.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer, or the second state is zero.
    #[strict_type(tag = 0x14)]
    Div(StateSelector, StateSelector),

    /// Modulo-divide the first state on the second state, expecting them to be integers.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the state is not an unsigned 64-bit integer, or the second state is zero.
    #[strict_type(tag = 0x15)]
    Rem(StateSelector, StateSelector),

    /// Exponentiates the first state with the second state, expecting them to be integers.
    /// The resulting value is rounded towards zero.
    ///
    /// Fails if any of the state is not defined or contains multiple elements.
    /// Also fails if the first state is not an unsigned 64-bit integer,
    /// the second state is not an unsigned 32-bit integer, or there is an overflow.
    #[strict_type(tag = 0x16)]
    Exp(StateSelector, StateSelector),

    /// Count the number of elements of the global state of a certain type.
    #[strict_type(tag = 0x20)]
    Count(StateName),

    /// Count the number of unique elements of the global state of a certain type.
    #[strict_type(tag = 0x21)]
    CountUnique(StateName),

    /// Convert a verified state under the same state type into an ordered set.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// If the global state with the name is absent returns an empty set.
    #[strict_type(tag = 0x22)]
    SetV(StateName),

    /// Map from a field-based element state to a non-verifiable structured state;
    /// when the field-based element state repeats, it is ignored and only the initial state is
    /// kept.
    ///
    /// The map is sorted by its values, lexicographically.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// If the global state with the name is absent returns an empty map.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 0x23)]
    MapV2U(StateName),

    /// Map from a field-based element state to a list of non-verifiable structured state;
    /// when the field-based element state repeats, the list is extended with the non-verifiable
    /// state.
    ///
    /// The map is ordered according to the global state element order.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// If the global state with the name is absent returns an empty map.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 0x24)]
    MapV2ListU(StateName),

    /// Map from a field-based element state to a set of non-verifiable structured state;
    /// when the field-based element state repeats, the set is extended with the non-verifiable
    /// state.
    ///
    /// The map is ordered according to the global state element order.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// If the global state with the name is absent returns an empty map.
    ///
    /// # Nota bene
    ///
    /// The global state does not have only a partial ordering (it is a lattice).
    ///
    /// It is only in the case when one operation depends on outputs of another
    /// (via global or owned state) there is a guarantee that the global state
    /// defined by the second operation will always follow the first one.
    ///
    /// It is the responsibility of the codex developer
    /// to ensure non-ambiguity when this aggregator is used.
    #[strict_type(tag = 0x25)]
    MapV2SetU(StateName),

    /// Sums over verifiable part of a global state.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// Fails if the global state doesn't have any elements, or if there is an overflow,
    /// or the state type is not an unsigned integer.
    #[strict_type(tag = 0x30)]
    SumUnwrap(StateName),

    /// Sums over verifiable part of a global state.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// Produces zero f the global state doesn't have any elements, or if there is an overflow.
    ///
    /// If any of the elements of the global state are not an unsigner integer, treats them as zero.
    #[strict_type(tag = 0x31)]
    SumOrDefault(StateName),

    /// Takes a product of the elements of a global state, taking their verifiable part.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// Fails if the global state doesn't have any elements, or if there is an overflow,
    /// or the state type is not an unsigned integer.
    #[strict_type(tag = 0x32)]
    ProdUnwrap(StateName),

    /// Takes a product of the elements of a global state, taking their verifiable part.
    ///
    /// Acts only on a global state; doesn't recognize aggregated state.
    ///
    /// Produces zero f the global state doesn't have any elements, or if there is an overflow.
    ///
    /// If any of the elements of the global state are not an unsigner integer, treats them as one.
    #[strict_type(tag = 0x33)]
    ProdOrDefault(StateName),
}

impl SubAggregator {
    /// Returns names of the other computed state which this aggregator depends on
    /// and which needs to be computed before running this aggregator.
    pub fn depends_on(&self) -> Vec<&StateName> {
        match self {
            Self::Neg(StateSelector::Aggregated(state))
            | Self::Add(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Sub(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Mul(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Div(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Rem(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Exp(StateSelector::Global(_, _), StateSelector::Aggregated(state))
            | Self::Add(StateSelector::Aggregated(state), StateSelector::Global(_, _))
            | Self::Sub(StateSelector::Aggregated(state), StateSelector::Global(_, _))
            | Self::Mul(StateSelector::Aggregated(state), StateSelector::Global(_, _))
            | Self::Div(StateSelector::Aggregated(state), StateSelector::Global(_, _))
            | Self::Rem(StateSelector::Aggregated(state), StateSelector::Global(_, _))
            | Self::Exp(StateSelector::Aggregated(state), StateSelector::Global(_, _)) => vec![state],

            Self::Add(StateSelector::Aggregated(a), StateSelector::Aggregated(b))
            | Self::Sub(StateSelector::Aggregated(a), StateSelector::Aggregated(b))
            | Self::Mul(StateSelector::Aggregated(a), StateSelector::Aggregated(b))
            | Self::Div(StateSelector::Aggregated(a), StateSelector::Aggregated(b))
            | Self::Rem(StateSelector::Aggregated(a), StateSelector::Aggregated(b))
            | Self::Exp(StateSelector::Aggregated(a), StateSelector::Aggregated(b)) => vec![a, b],

            Self::Const(_, _)
            | Self::TheOnly(_)
            | Self::Count(_)
            | Self::CountUnique(_)
            | Self::Copy(_)
            | Self::Unwrap(_)
            | Self::First(_)
            | Self::Nth(_, _)
            | Self::Last(_)
            | Self::NthBack(_, _)
            | Self::Neg(_)
            | Self::Add(_, _)
            | Self::Sub(_, _)
            | Self::Mul(_, _)
            | Self::Div(_, _)
            | Self::Rem(_, _)
            | Self::Exp(_, _)
            | Self::SetV(_)
            | Self::MapV2U(_)
            | Self::MapV2ListU(_)
            | Self::MapV2SetU(_)
            | Self::SumUnwrap(_)
            | Self::SumOrDefault(_)
            | Self::ProdUnwrap(_)
            | Self::ProdOrDefault(_) => vec![],
        }
    }

    /// Compute state via applying some aggregator function.
    ///
    /// # Returns
    ///
    /// Aggregated state value.
    /// If the computing fails due to any exception, `None`.
    pub fn aggregate(
        &self,
        global: &BTreeMap<StateName, BTreeMap<CellAddr, StateAtom>>,
        aggregated: &BTreeMap<StateName, StrictVal>,
        types: &TypeSystem,
    ) -> Option<StrictVal> {
        let get_u64 = |sel: &StateSelector| -> Option<u64> {
            let state = match sel {
                StateSelector::Global(name, first) => {
                    let map = global.get(name)?;
                    if map.len() != 1 && !*first {
                        return None;
                    }
                    let (_, atom) = map.first_key_value()?;
                    &atom.verified
                }
                StateSelector::Aggregated(name) => aggregated.get(name)?,
            };
            match state {
                StrictVal::Number(StrictNum::Uint(val)) => Some(*val),
                _ => None,
            }
        };

        match self {
            Self::Const(sem_id, val) => deserialize(*sem_id, val, types),

            Self::TheOnly(name) => {
                let state = global.get(name)?;
                if state.len() != 1 {
                    return None;
                }
                Some(state.first_key_value()?.1.verified.clone())
            }

            Self::Copy(name) => aggregated.get(name).cloned(),

            Self::Unwrap(name) => {
                let state = global.get(name)?;
                if state.len() != 1 {
                    return None;
                }
                let (_, atom) = state.first_key_value()?;
                let StrictVal::Union(tag, sv) = &atom.verified else {
                    return None;
                };
                Some(match tag {
                    EnumTag::Name(name) if name.as_str() == "some" => sv.as_ref().clone(),
                    EnumTag::Ord(1) => sv.as_ref().clone(),
                    _ => return None,
                })
            }

            Self::First(name) => {
                let state = global.get(name)?;
                Some(state.first_key_value()?.1.verified.clone())
            }

            Self::Nth(name, pos) => {
                let state = global.get(name)?;
                Some(state.iter().nth(*pos as usize)?.1.verified.clone())
            }

            Self::Last(name) => {
                let state = global.get(name)?;
                Some(state.last_key_value()?.1.verified.clone())
            }

            Self::NthBack(name, pos) => {
                let state = global.get(name)?;
                Some(state.iter().nth_back(*pos as usize)?.1.verified.clone())
            }

            Self::Neg(name) => {
                let val = get_u64(name)?;
                let neg = (val as i64).checked_neg()?;
                Some(svnum!(neg))
            }
            Self::Add(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sum = a.checked_add(b)?;
                Some(svnum!(sum))
            }
            Self::Sub(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sub = a.checked_sub(b)?;
                Some(svnum!(sub))
            }
            Self::Mul(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sub = a.checked_mul(b)?;
                Some(svnum!(sub))
            }
            Self::Div(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sub = a.checked_div(b)?;
                Some(svnum!(sub))
            }
            Self::Rem(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sub = a.checked_rem(b)?;
                Some(svnum!(sub))
            }
            Self::Exp(a, b) => {
                let a = get_u64(a)?;
                let b = get_u64(b)?;
                let sub = a.checked_pow(b.try_into().ok()?)?;
                Some(svnum!(sub))
            }

            Self::Count(name) => {
                let count = global
                    .get(name)
                    .into_iter()
                    .flat_map(BTreeMap::values)
                    .count();
                Some(svnum!(count as u64))
            }

            Self::CountUnique(name) => {
                let mut unique = Vec::new();
                for item in global.get(name)?.values() {
                    if !unique.contains(&item) {
                        unique.push(item);
                    }
                }
                Some(svnum!(unique.len() as u64))
            }

            Self::SetV(name) => {
                let mut set = Vec::new();
                for state in global.get(name).into_iter().flat_map(BTreeMap::values) {
                    if !set.contains(&state.verified) {
                        set.push(state.verified.clone());
                    }
                }
                Some(StrictVal::Set(set))
            }

            Self::MapV2U(name) => {
                let mut map = Vec::new();
                for atom in global.get(name)?.values() {
                    let Some(val) = &atom.unverified else { continue };
                    if map.iter().any(|(key, _)| &atom.verified == key) {
                        continue;
                    }
                    map.push((atom.verified.clone(), val.clone()));
                }
                Some(StrictVal::Map(map))
            }

            Self::MapV2ListU(name) => {
                let mut map = Vec::<(StrictVal, StrictVal)>::new();
                for atom in global.get(name)?.values() {
                    let Some(val) = &atom.unverified else { continue };
                    if let Some((_key, list)) = map.iter_mut().find(|(key, _)| &atom.verified == key) {
                        let StrictVal::List(list) = list else {
                            unreachable!();
                        };
                        list.push(val.clone());
                    } else {
                        map.push((atom.verified.clone(), StrictVal::List(vec![val.clone()])));
                    }
                }
                Some(StrictVal::Map(map))
            }

            Self::MapV2SetU(name) => {
                let mut map = Vec::<(StrictVal, StrictVal)>::new();
                for atom in global.get(name)?.values() {
                    let Some(val) = &atom.unverified else { continue };
                    if let Some((_key, list)) = map.iter_mut().find(|(key, _)| &atom.verified == key) {
                        let StrictVal::List(list) = list else {
                            unreachable!();
                        };
                        if !list.contains(val) {
                            list.push(val.clone());
                        }
                    } else {
                        map.push((atom.verified.clone(), StrictVal::Set(vec![val.clone()])));
                    }
                }
                Some(StrictVal::Map(map))
            }

            Self::SumUnwrap(name) => {
                let sum = global
                    .get(name)
                    .into_iter()
                    .flat_map(BTreeMap::values)
                    .try_fold(0u64, |sum, val| match &val.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => sum.checked_add(*val),
                        _ => None,
                    })?;
                Some(svnum!(sum))
            }

            Self::SumOrDefault(name) => {
                let sum = global
                    .get(name)
                    .into_iter()
                    .flat_map(BTreeMap::values)
                    .try_fold(0u64, |sum, val| match &val.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => sum.checked_add(*val),
                        _ => Some(sum),
                    })?;
                Some(svnum!(sum))
            }

            Self::ProdUnwrap(name) => {
                let sum = global
                    .get(name)
                    .into_iter()
                    .flat_map(BTreeMap::values)
                    .try_fold(0u64, |prod, val| match &val.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => prod.checked_mul(*val),
                        _ => None,
                    })?;
                Some(svnum!(sum))
            }

            Self::ProdOrDefault(name) => {
                let sum = global
                    .get(name)
                    .into_iter()
                    .flat_map(BTreeMap::values)
                    .try_fold(0u64, |prod, val| match &val.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => prod.checked_mul(*val),
                        _ => Some(prod),
                    })?;
                Some(svnum!(sum))
            }
        }
    }
}

fn deserialize(sem_id: SemId, val: &TinyBlob, types: &TypeSystem) -> Option<StrictVal> {
    let ty = types.strict_deserialize_type(sem_id, val.as_slice()).ok()?;
    Some(ty.unbox())
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]
    use super::*;

    fn addr(no: u16) -> CellAddr { CellAddr::new(strict_dumb!(), no) }
    fn state() -> BTreeMap<StateName, BTreeMap<CellAddr, StateAtom>> {
        bmap! {
            vname!("pairs") => bmap! {
                addr(0) => StateAtom::with(5u64, "state 1"),
                addr(1) => StateAtom::with(1u64, "state 2"),
                addr(2) => StateAtom::with(2u64, "state 3"),
                addr(3) => StateAtom::with(3u64, "state 4"),
                addr(4) => StateAtom::with(4u64, "state 5"),
                addr(5) => StateAtom::with(5u64, "state 6"),
            },
            vname!("verified") => bmap! {
                addr(0) => StateAtom::new_verified(5u64),
                addr(1) => StateAtom::new_verified(1u64),
                addr(2) => StateAtom::new_verified(2u64),
                addr(3) => StateAtom::new_verified(3u64),
                addr(4) => StateAtom::new_verified(4u64),
                addr(5) => StateAtom::new_verified(5u64),
            },
            vname!("unverified") => bmap! {
                addr(0) => StateAtom::new_unverified("state 1"),
                addr(1) => StateAtom::new_unverified("state 2"),
                addr(2) => StateAtom::new_unverified("state 3"),
                addr(3) => StateAtom::new_unverified("state 4"),
                addr(4) => StateAtom::new_unverified("state 5"),
                addr(5) => StateAtom::new_unverified("state 6"),
            },
        }
    }
    fn call(aggregator: Aggregator) -> StrictVal {
        aggregator
            .aggregate(&state(), &none!(), None, &none!())
            .unwrap()
    }

    #[test]
    fn verified_readers() {
        assert_eq!(call(Aggregator::Take(SubAggregator::Count(vname!("verified")))), svnum!(6u64));
        assert_eq!(
            call(Aggregator::Take(SubAggregator::SumUnwrap(vname!("verified")))),
            svnum!(5u64 + 1 + 2 + 3 + 4 + 5)
        );
        assert_eq!(
            call(Aggregator::Take(SubAggregator::SetV(vname!("verified")))),
            svset!([5u64, 1u64, 2u64, 3u64, 4u64])
        );
        assert_eq!(call(Aggregator::Take(SubAggregator::MapV2U(vname!("verified")))), StrictVal::Map(none!()));
    }

    #[test]
    fn unverified_readers() {
        assert_eq!(call(Aggregator::Take(SubAggregator::Count(vname!("verified")))), svnum!(6u64));
        assert_eq!(call(Aggregator::Take(SubAggregator::SetV(vname!("unverified")))), svset!([()]));
        assert_eq!(
            call(Aggregator::Take(SubAggregator::MapV2U(vname!("unverified")))),
            StrictVal::Map(vec![(StrictVal::Unit, svstr!("state 1"))])
        );
    }

    #[test]
    #[should_panic]
    fn unverified_sum() { call(Aggregator::Take(SubAggregator::SumUnwrap(vname!("unverified")))); }

    #[test]
    fn pair_readers() {
        assert_eq!(call(Aggregator::Take(SubAggregator::Count(vname!("verified")))), svnum!(6u64));
        assert_eq!(call(Aggregator::Take(SubAggregator::SumUnwrap(vname!("pairs")))), svnum!(5u64 + 1 + 2 + 3 + 4 + 5));
        assert_eq!(
            call(Aggregator::Take(SubAggregator::SetV(vname!("pairs")))),
            svset!([5u64, 1u64, 2u64, 3u64, 4u64])
        );
        assert_eq!(
            call(Aggregator::Take(SubAggregator::MapV2U(vname!("pairs")))),
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
