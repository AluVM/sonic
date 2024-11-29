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

use amplify::confinement::SmallOrdMap;
use indexmap::IndexMap;
use sonicapi::{Api, StateAtom, StateName};
use strict_encoding::TypeName;
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{AuthToken, CellAddr, Memory, Operation, Opid, StateCell, StateData, StateValue};

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
    #[inline]
    pub fn addr(&self, auth: AuthToken) -> CellAddr { self.raw.addr(auth) }

    pub fn read(&self, name: impl Into<StateName>) -> &StrictVal {
        let name = name.into();
        self.main
            .computed
            .get(&name)
            .unwrap_or_else(|| panic!("Computed state {name} is not known"))
    }

    #[must_use]
    pub(crate) fn apply<'a>(
        &mut self,
        op: Operation,
        default_api: &Api,
        custom_apis: impl IntoIterator<Item = &'a Api>,
        sys: &TypeSystem,
    ) -> Transition {
        self.main.apply(&op, default_api, sys);
        for api in custom_apis {
            // TODO: Remove name from API itself.
            let Some(name) = api.name() else {
                continue;
            };
            let state = self.aux.entry(name.clone()).or_default();
            state.apply(&op, api, sys);
        }
        self.raw.apply(op)
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct RawState {
    /// Tokens of authority
    pub auth: IndexMap<AuthToken, CellAddr>,
    pub immutable: BTreeMap<CellAddr, StateData>,
    pub owned: BTreeMap<CellAddr, StateCell>,
}

impl Memory for RawState {
    fn read_once(&self, addr: CellAddr) -> Option<StateCell> { self.owned.get(&addr).copied() }
    fn immutable(&self, addr: CellAddr) -> Option<StateValue> { self.immutable.get(&addr).map(|data| data.value) }
}

impl RawState {
    pub fn addr(&self, auth: AuthToken) -> CellAddr { *self.auth.get(&auth).expect("undefined token oof authority") }

    #[must_use]
    pub fn apply(&mut self, op: Operation) -> Transition {
        let opid = op.opid();
        let mut transition = Transition::new(opid);

        for input in op.destroying {
            let res = self.owned.remove(&input.addr).expect("unknown input");
            self.auth.shift_remove(&res.auth);

            let res = transition
                .destroyed
                .insert(input.addr, res)
                .expect("transaction too large");
            debug_assert!(res.is_none());
        }

        for (no, cell) in op.destructible.into_iter().enumerate() {
            let addr = CellAddr::new(opid, no as u16);
            self.auth.insert(cell.auth, addr);
            self.owned.insert(addr, cell);
        }

        self.immutable.extend(
            op.immutable
                .into_iter()
                .enumerate()
                .map(|(no, data)| (CellAddr::new(opid, no as u16), data)),
        );

        transition
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
    pub fn immutable(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StateAtom>> { self.immutable.get(name) }

    pub fn owned(&self, name: &StateName) -> Option<&BTreeMap<CellAddr, StrictVal>> { self.owned.get(name) }

    pub(super) fn compute(&mut self, api: &Api) {
        let empty = bmap![];
        self.computed = bmap! {};
        for reader in api.readers() {
            let val = api.read(reader, |name| match self.immutable(&name) {
                None => empty.values(),
                Some(src) => src.values(),
            });
            self.computed.insert(reader.clone(), val);
        }
    }

    pub(self) fn apply(&mut self, op: &Operation, api: &Api, sys: &TypeSystem) {
        let opid = op.opid();
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
}
