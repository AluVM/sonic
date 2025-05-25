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
use std::mem;

use aluvm::Lib;
use amplify::confinement::{LargeOrdMap, SmallOrdMap, SmallOrdSet};
use sonicapi::{Api, Articles, Semantics, StateAtom, StateName};
use strict_encoding::{StrictDeserialize, StrictSerialize, TypeName};
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{AuthToken, CallError, CellAddr, Memory, Opid, StateCell, StateData, StateValue, VerifiedOperation};

use crate::LIB_NAME_SONIC;

/// State transitions keeping track of the operation reference plus the state destroyed by the
/// operation.
#[derive(Clone, PartialEq, Eq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Transition {
    pub opid: Opid,
    pub destroyed: SmallOrdMap<CellAddr, StateCell>,
}

impl Transition {
    fn new(opid: Opid) -> Self { Self { opid, destroyed: none!() } }
}

#[derive(Clone, Debug, Default)]
pub struct EffectiveState {
    pub raw: RawState,
    pub main: ProcessedState,
    pub aux: BTreeMap<TypeName, ProcessedState>,
}

impl EffectiveState {
    pub fn with_articles(articles: &Articles) -> Result<Self, CallError> {
        let mut state = EffectiveState::default();

        let contract_id = articles.contract_id();
        let genesis = articles.genesis().to_operation(contract_id);

        let verified = articles
            .codex()
            .verify(contract_id, genesis, &state.raw, articles)?;

        // We do not need state transition for genesis.
        let _ = state.apply(verified, articles.semantics());

        Ok(state)
    }

    pub fn with_raw_state(raw: RawState, articles: &Articles) -> Self {
        let mut me = Self { raw, main: none!(), aux: none!() };
        me.main = ProcessedState::with(&me.raw, articles.default_api(), articles.types());
        me.aux.clear();
        for (name, api) in articles.custom_apis() {
            let state = ProcessedState::with(&me.raw, api, articles.types());
            me.aux.insert(name.clone(), state);
        }
        me.recompute(articles.semantics());
        me
    }

    #[inline]
    pub fn addr(&self, auth: AuthToken) -> CellAddr { self.raw.addr(auth) }

    pub fn read(&self, name: impl Into<StateName>) -> &StrictVal {
        let name = name.into();
        self.main
            .aggregated
            .get(&name)
            .unwrap_or_else(|| panic!("Computed state {name} is not known"))
    }

    /// Re-evaluates computable part of the state
    pub fn recompute(&mut self, apis: &Semantics) {
        self.main.aggregate(&apis.default, &apis.api_libs);
        self.aux = bmap! {};
        for (name, api) in &apis.custom {
            let mut state = ProcessedState::default();
            state.aggregate(api, &apis.api_libs);
            self.aux.insert(name.clone(), state);
        }
    }

    #[must_use]
    pub(crate) fn apply(&mut self, op: VerifiedOperation, apis: &Semantics) -> Transition {
        self.main.apply(&op, &apis.default, &apis.types);
        for (name, api) in &apis.custom {
            let state = self.aux.entry(name.clone()).or_default();
            state.apply(&op, api, &apis.types);
        }
        self.raw.apply(op)
    }

    pub(crate) fn rollback(&mut self, transition: Transition, apis: &Semantics) {
        self.main.rollback(&transition, &apis.default, &apis.types);
        let mut count = 0usize;
        for (name, api) in &apis.custom {
            let state = self.aux.get_mut(name).expect("unknown aux API");
            state.rollback(&transition, api, &apis.types);
            count += 1;
        }
        debug_assert_eq!(count, self.aux.len());
        self.raw.rollback(transition);
    }
}

#[derive(Clone, Debug, Default)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct RawState {
    /// Tokens of authority
    pub auth: LargeOrdMap<AuthToken, CellAddr>,
    pub global: LargeOrdMap<CellAddr, StateData>,
    pub owned: LargeOrdMap<CellAddr, StateCell>,
}

impl StrictSerialize for RawState {}
impl StrictDeserialize for RawState {}

impl Memory for RawState {
    fn destructible(&self, addr: CellAddr) -> Option<StateCell> { self.owned.get(&addr).copied() }
    fn immutable(&self, addr: CellAddr) -> Option<StateValue> { self.global.get(&addr).map(|data| data.value) }
}

impl RawState {
    pub fn addr(&self, auth: AuthToken) -> CellAddr {
        *self
            .auth
            .get(&auth)
            .unwrap_or_else(|| panic!("undefined token of authority {auth}"))
    }

    #[must_use]
    pub fn apply(&mut self, op: VerifiedOperation) -> Transition {
        let opid = op.opid();
        let op = op.into_operation();
        let mut transition = Transition::new(opid);

        for input in op.destructible_in {
            let res = self
                .owned
                .remove(&input.addr)
                .expect("zero-sized confinement is allowed")
                .expect("unknown input");
            self.auth.remove(&res.auth).expect("zero-sized is allowed");

            let res = transition
                .destroyed
                .insert(input.addr, res)
                .expect("transaction too large");
            debug_assert!(res.is_none());
        }

        for (no, cell) in op.destructible_out.into_iter().enumerate() {
            let addr = CellAddr::new(opid, no as u16);
            self.auth
                .insert(cell.auth, addr)
                .expect("too many authentication tokens");
            self.owned.insert(addr, cell).expect("state too large");
        }

        self.global
            .extend(
                op.immutable_out
                    .into_iter()
                    .enumerate()
                    .map(|(no, data)| (CellAddr::new(opid, no as u16), data)),
            )
            .expect("exceed state size limit");

        transition
    }

    pub(self) fn rollback(&mut self, transition: Transition) {
        let opid = transition.opid;

        let mut global = mem::take(&mut self.global);
        let mut owned = mem::take(&mut self.owned);
        global = LargeOrdMap::from_iter_checked(global.into_iter().filter(|(addr, _)| addr.opid != opid));
        owned = LargeOrdMap::from_iter_checked(owned.into_iter().filter(|(addr, _)| addr.opid != opid));
        self.global = global;
        self.owned = owned;

        // TODO: Use `retain` instead of the above workaround once supported by amplify
        // self.immutable.retain(|addr, _| addr.opid != opid);
        // self.owned.retain(|addr, _| addr.opid != opid);

        for (addr, cell) in transition.destroyed {
            self.owned
                .insert(addr, cell)
                .expect("exceed state size limit");
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ProcessedState {
    pub global: BTreeMap<StateName, BTreeMap<CellAddr, StateAtom>>,
    pub owned: BTreeMap<StateName, BTreeMap<CellAddr, StrictVal>>,
    pub aggregated: BTreeMap<StateName, StrictVal>,
    pub invalid_global: BTreeMap<CellAddr, StateData>,
    pub invalid_owned: BTreeMap<CellAddr, StateValue>,
}

impl ProcessedState {
    pub fn with(raw: &RawState, api: &Api, sys: &TypeSystem) -> Self {
        let mut me = ProcessedState::default();
        for (addr, state) in &raw.global {
            me.process_global(*addr, state, api, sys);
        }
        for (addr, state) in &raw.owned {
            me.process_owned(*addr, state, api, sys);
        }
        me
    }

    pub fn global(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StateAtom>> { self.global.get(name) }

    pub fn owned(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StrictVal>> { self.owned.get(name) }

    pub(super) fn aggregate(&mut self, api: &Api, libs: &SmallOrdSet<Lib>) {
        self.aggregated = bmap! {};
        for (name, aggregator) in api.aggregators() {
            let val = aggregator.aggregate(
                |state_name| {
                    self.global(state_name)
                        .map(|map| map.values().cloned().collect::<Vec<_>>())
                        .or_else(|| {
                            let verified = self.aggregated.get(state_name)?.clone();
                            Some(vec![StateAtom { verified, unverified: None }])
                        })
                        .unwrap_or_default()
                },
                libs,
            );
            if let Some(val) = val {
                self.aggregated.insert(name.clone(), val);
            }
        }
    }

    pub(self) fn apply(&mut self, op: &VerifiedOperation, api: &Api, sys: &TypeSystem) {
        let opid = op.opid();
        let op = op.as_operation();
        for (no, state) in op.immutable_out.iter().enumerate() {
            let addr = CellAddr::new(opid, no as u16);
            self.process_global(addr, state, api, sys);
        }
        for input in &op.destructible_in {
            for map in self.owned.values_mut() {
                map.remove(&input.addr);
            }
        }
        for (no, state) in op.destructible_out.iter().enumerate() {
            let addr = CellAddr::new(opid, no as u16);
            self.process_owned(addr, state, api, sys);
        }
    }

    pub(self) fn rollback(&mut self, transition: &Transition, api: &Api, sys: &TypeSystem) {
        let opid = transition.opid;

        self.global
            .values_mut()
            .for_each(|state| state.retain(|addr, _| addr.opid != opid));
        self.owned
            .values_mut()
            .for_each(|state| state.retain(|addr, _| addr.opid != opid));

        for (addr, cell) in &transition.destroyed {
            self.process_owned(*addr, cell, api, sys);
        }
    }

    fn process_global(&mut self, addr: CellAddr, state: &StateData, api: &Api, sys: &TypeSystem) {
        match api.convert_global(state, sys) {
            // This means this state is unrelated to this API
            Ok(None) => {}
            Ok(Some((name, atom))) => {
                self.global.entry(name).or_default().insert(addr, atom);
            }
            Err(_) => {
                self.invalid_global.insert(addr, state.clone());
            }
        }
    }

    fn process_owned(&mut self, addr: CellAddr, state: &StateCell, api: &Api, sys: &TypeSystem) {
        match api.convert_owned(state.data, sys) {
            // This means this state is unrelated to this API
            Ok(None) => {}
            Ok(Some((name, atom))) => {
                self.owned.entry(name).or_default().insert(addr, atom);
            }
            Err(_) => {
                self.invalid_owned.insert(addr, state.data);
            }
        }
    }
}
