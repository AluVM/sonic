// SONIC: Toolchain for formally-verifiable distributed contracts
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

use amplify::confinement::{LargeOrdMap, SmallOrdMap};
use sonicapi::{Api, Articles, Schema, StateAtom, StateName};
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
    pub main: AdaptedState,
    pub aux: BTreeMap<TypeName, AdaptedState>,
}

impl EffectiveState {
    pub fn from_genesis(articles: &Articles) -> Result<Self, CallError> {
        let mut state = EffectiveState::default();

        let genesis = articles
            .issue
            .genesis
            .to_operation(articles.issue.contract_id());

        let verified =
            articles
                .schema
                .codex
                .verify(articles.issue.contract_id(), genesis, &state.raw, &articles.schema)?;

        // We do not need state transition for genesis.
        let _ = state.apply(
            verified,
            &articles.schema.default_api,
            articles.schema.custom_apis.keys(),
            &articles.schema.types,
        );

        Ok(state)
    }

    /// NB: Do not forget to call `recompute state` after.
    pub fn with(raw: RawState, schema: &Schema) -> Self {
        let mut me = Self { raw, main: none!(), aux: none!() };
        me.main = AdaptedState::with(&me.raw, &schema.default_api, &schema.types);
        me.aux.clear();
        for api in schema.custom_apis.keys() {
            let Some(name) = api.name() else {
                continue;
            };
            let state = AdaptedState::with(&me.raw, api, &schema.types);
            me.aux.insert(name.clone(), state);
        }
        me.recompute(&schema.default_api, schema.custom_apis.keys());
        me
    }

    #[inline]
    pub fn addr(&self, auth: AuthToken) -> CellAddr { self.raw.addr(auth) }

    pub fn read(&self, name: impl Into<StateName>) -> &StrictVal {
        let name = name.into();
        self.main
            .computed
            .get(&name)
            .unwrap_or_else(|| panic!("Computed state {name} is not known"))
    }

    /// Re-evaluates computable part of the state
    pub(crate) fn recompute<'a>(&mut self, default_api: &Api, custom_apis: impl IntoIterator<Item = &'a Api>) {
        self.main.compute(default_api);
        self.aux = bmap! {};
        for api in custom_apis {
            let mut s = AdaptedState::default();
            s.compute(api);
            self.aux
                .insert(api.name().cloned().expect("unnamed aux API"), s);
        }
    }

    #[must_use]
    pub(crate) fn apply<'a>(
        &mut self,
        op: VerifiedOperation,
        default_api: &Api,
        custom_apis: impl IntoIterator<Item = &'a Api>,
        sys: &TypeSystem,
    ) -> Transition {
        self.main.apply(&op, default_api, sys);
        for api in custom_apis {
            // TODO: Remove name from API itself.
            // Skip default API (it is already processed as `main` above)
            let Some(name) = api.name() else {
                continue;
            };
            let state = self.aux.entry(name.clone()).or_default();
            state.apply(&op, api, sys);
        }
        self.raw.apply(op)
    }

    pub(crate) fn rollback<'a>(
        &mut self,
        transition: Transition,
        default_api: &Api,
        custom_apis: impl IntoIterator<Item = &'a Api>,
        sys: &TypeSystem,
    ) {
        self.main.rollback(&transition, default_api, sys);
        let mut count = 0usize;
        for api in custom_apis {
            // Skip default API (it is already processed as `main` above)
            let Some(name) = api.name() else {
                continue;
            };
            let state = self.aux.get_mut(name).expect("unknown aux API");
            state.rollback(&transition, api, sys);
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
    pub immutable: LargeOrdMap<CellAddr, StateData>,
    pub owned: LargeOrdMap<CellAddr, StateCell>,
}

impl StrictSerialize for RawState {}
impl StrictDeserialize for RawState {}

impl Memory for RawState {
    fn read_once(&self, addr: CellAddr) -> Option<StateCell> { self.owned.get(&addr).copied() }
    fn immutable(&self, addr: CellAddr) -> Option<StateValue> { self.immutable.get(&addr).map(|data| data.value) }
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

        for input in op.destroying {
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

        for (no, cell) in op.destructible.into_iter().enumerate() {
            let addr = CellAddr::new(opid, no as u16);
            self.auth
                .insert(cell.auth, addr)
                .expect("too many authentication tokens");
            self.owned.insert(addr, cell).expect("state too large");
        }

        self.immutable
            .extend(
                op.immutable
                    .into_iter()
                    .enumerate()
                    .map(|(no, data)| (CellAddr::new(opid, no as u16), data)),
            )
            .expect("exceed state size limit");

        transition
    }

    pub(self) fn rollback(&mut self, transition: Transition) {
        let opid = transition.opid;

        let mut immutable = mem::take(&mut self.immutable);
        let mut owned = mem::take(&mut self.owned);
        immutable = LargeOrdMap::from_iter_checked(immutable.into_iter().filter(|(addr, _)| addr.opid != opid));
        owned = LargeOrdMap::from_iter_checked(owned.into_iter().filter(|(addr, _)| addr.opid != opid));
        self.immutable = immutable;
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
pub struct AdaptedState {
    pub immutable: BTreeMap<StateName, BTreeMap<CellAddr, StateAtom>>,
    pub owned: BTreeMap<StateName, BTreeMap<CellAddr, StrictVal>>,
    pub computed: BTreeMap<StateName, StrictVal>,
}

impl AdaptedState {
    pub fn with(raw: &RawState, api: &Api, sys: &TypeSystem) -> Self {
        let mut me = AdaptedState::default();
        for (addr, state) in &raw.immutable {
            if let Some((name, atom)) = api.convert_immutable(state, sys) {
                me.immutable.entry(name).or_default().insert(*addr, atom);
            }
        }
        for (addr, state) in &raw.owned {
            if let Some((name, atom)) = api.convert_destructible(state.data, sys) {
                me.owned.entry(name).or_default().insert(*addr, atom);
            }
        }
        me
    }

    pub fn immutable(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StateAtom>> { self.immutable.get(name) }

    pub fn owned(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StrictVal>> { self.owned.get(name) }

    pub(super) fn compute(&mut self, api: &Api) {
        let empty = bmap![];
        self.computed = bmap! {};
        for reader in api.readers() {
            let val = api.read(reader, |name| match self.immutable(name) {
                None => empty.values(),
                Some(src) => src.values(),
            });
            self.computed.insert(reader.clone(), val);
        }
    }

    pub(self) fn apply(&mut self, op: &VerifiedOperation, api: &Api, sys: &TypeSystem) {
        let opid = op.opid();
        let op = op.as_operation();
        for (no, state) in op.immutable.iter().enumerate() {
            if let Some((name, atom)) = api.convert_immutable(state, sys) {
                self.immutable
                    .entry(name)
                    .or_default()
                    .insert(CellAddr::new(opid, no as u16), atom);
            }
            // TODO: Warn if no state is present
        }
        for input in &op.destroying {
            for map in self.owned.values_mut() {
                map.remove(&input.addr);
            }
        }
        for (no, state) in op.destructible.iter().enumerate() {
            if let Some((name, atom)) = api.convert_destructible(state.data, sys) {
                self.owned
                    .entry(name)
                    .or_default()
                    .insert(CellAddr::new(opid, no as u16), atom);
            }
        }
    }

    pub(self) fn rollback(&mut self, transition: &Transition, api: &Api, sys: &TypeSystem) {
        let opid = transition.opid;

        self.immutable
            .values_mut()
            .for_each(|state| state.retain(|addr, _| addr.opid != opid));
        self.owned
            .values_mut()
            .for_each(|state| state.retain(|addr, _| addr.opid != opid));

        for (addr, cell) in &transition.destroyed {
            if let Some((name, value)) = api.convert_destructible(cell.data, sys) {
                self.owned.entry(name).or_default().insert(*addr, value);
            }
            // TODO: Warn if no state is present
        }
    }
}

#[cfg(feature = "std")]
mod _fs {
    use std::path::Path;

    use amplify::confinement::U24 as U24MAX;
    use strict_encoding::{DeserializeError, SerializeError, StrictDeserialize, StrictSerialize};

    use super::RawState;

    // TODO: Use BinFile
    impl RawState {
        pub fn load(path: impl AsRef<Path>) -> Result<Self, DeserializeError> {
            Self::strict_deserialize_from_file::<U24MAX>(path)
        }

        pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SerializeError> {
            self.strict_serialize_to_file::<U24MAX>(path)
        }
    }
}
