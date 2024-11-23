// SONIC: Toolchain for formally-verifiable distributed contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2024 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
//
// Copyright (C) 2019-2025 LNP/BP Standards Association, Switzerland.
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

use aluvm::LibSite;
use amplify::confinement::SmallVec;
use sonicapi::{Api, StateName};
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{fe128, CallId, CellAddr, Codex, ContractId, Genesis, Input, Operation, StateCell, StateData};

pub struct Builder {
    call_id: CallId,
    destructible: SmallVec<StateCell>,
    immutable: SmallVec<StateData>,
}

impl Builder {
    pub fn new(call_id: CallId) -> Self { Builder { call_id, destructible: none!(), immutable: none!() } }

    pub fn add_immutable(
        mut self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let data = api.build_immutable(name, data, raw, sys);
        self.immutable.push(data).expect("too many state elements");
        self
    }

    pub fn add_destructible(
        mut self,
        name: impl Into<StateName>,
        seal: fe128,
        data: StrictVal,
        lock: Option<LibSite>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let data = api.build_destructible(name, data, sys);
        let cell = StateCell { data, seal, lock };
        self.destructible
            .push(cell)
            .expect("too many state elements");
        self
    }

    pub fn issue_genesis(self, codex: &Codex) -> Genesis {
        Genesis {
            codex_id: codex.codex_id(),
            call_id: self.call_id,
            destructible: self.destructible,
            immutable: self.immutable,
            reserved: zero!(),
        }
    }
}

pub struct BuilderRef<'c> {
    type_system: &'c TypeSystem,
    api: &'c Api,
    codex: &'c Codex,
    inner: Builder,
}

impl<'c> BuilderRef<'c> {
    pub fn new(api: &'c Api, codex: &'c Codex, call_id: CallId, sys: &'c TypeSystem) -> Self {
        BuilderRef { type_system: sys, api, codex, inner: Builder::new(call_id) }
    }

    pub fn add_immutable(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.inner = self
            .inner
            .add_immutable(name, data, raw, self.api, self.type_system);
        self
    }

    pub fn add_destructible(
        mut self,
        name: impl Into<StateName>,
        seal: fe128,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        self.inner = self
            .inner
            .add_destructible(name, seal, data, lock, self.api, self.type_system);
        self
    }

    pub fn issue_genesis(self) -> Genesis { self.inner.issue_genesis(&self.codex) }
}

pub struct OpBuilder<'c> {
    contract_id: ContractId,
    destroying: SmallVec<Input>,
    reading: SmallVec<CellAddr>,
    inner: BuilderRef<'c>,
}

impl<'c> OpBuilder<'c> {
    pub fn add_immutable(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.inner = self.inner.add_immutable(name, data, raw);
        self
    }

    pub fn add_destructible(
        mut self,
        name: impl Into<StateName>,
        seal: fe128,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        self.inner = self.inner.add_destructible(name, seal, data, lock);
        self
    }

    pub fn finalize(self) -> Operation {
        Operation {
            contract_id: self.contract_id,
            call_id: self.inner.inner.call_id,
            destroying: self.destroying,
            reading: self.reading,
            destructible: self.inner.inner.destructible,
            immutable: self.inner.inner.immutable,
            reserved: zero!(),
        }
    }
}
