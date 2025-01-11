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

use aluvm::LibSite;
use amplify::confinement::SmallVec;
use amplify::num::u256;
use chrono::{DateTime, Utc};
use strict_encoding::TypeName;
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{
    fe256, AuthToken, CallId, CellAddr, CodexId, ConstU32, Contract, ContractId, ContractMeta, ContractName, Genesis,
    Identity, Input, Operation, StateCell, StateData, StateValue,
};

use crate::{Api, Articles, DataCell, MethodName, Schema, StateAtom, StateName};

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NamedState<T> {
    pub name: StateName,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub state: T,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CoreParams {
    pub method: MethodName,
    pub global: Vec<NamedState<StateAtom>>,
    pub owned: Vec<NamedState<DataCell>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IssueParams {
    pub name: TypeName,
    pub testnet: bool,
    pub timestamp: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub core: CoreParams,
}

impl Schema {
    pub fn start_issue(self, method: impl Into<MethodName>, testnet: bool) -> IssueBuilder {
        let builder = Builder::new(self.call_id(method));
        IssueBuilder { builder, schema: self, testnet }
    }

    pub fn start_issue_mainnet(self, method: impl Into<MethodName>) -> IssueBuilder { self.start_issue(method, false) }
    pub fn start_issue_testnet(self, method: impl Into<MethodName>) -> IssueBuilder { self.start_issue(method, true) }

    pub fn issue<const CAPS: u32>(self, params: IssueParams) -> Articles<CAPS> {
        let mut builder = self.start_issue(params.core.method, params.testnet);

        for NamedState { name, state } in params.core.global {
            builder = builder.append(name, state.verified, state.unverified)
        }
        for NamedState { name, state } in params.core.owned {
            builder = builder.assign(name, state.auth, state.data, state.lock)
        }

        let timestamp = params.timestamp.unwrap_or_else(Utc::now).timestamp();
        builder.finish(params.name, timestamp)
    }
}

#[derive(Clone, Debug)]
pub struct IssueBuilder {
    builder: Builder,
    schema: Schema,
    testnet: bool,
}

impl IssueBuilder {
    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.builder = self
            .builder
            .add_immutable(name, data, raw, &self.schema.default_api, &self.schema.types);
        self
    }

    pub fn assign(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        self.builder =
            self.builder
                .add_destructible(name, auth, data, lock, &self.schema.default_api, &self.schema.types);
        self
    }

    pub fn finish<const CAPS: u32>(self, name: impl Into<TypeName>, timestamp: i64) -> Articles<CAPS> {
        let meta = ContractMeta {
            capabilities: ConstU32::<CAPS>::new(),
            testnet: self.testnet,
            reserved: zero!(),
            timestamp,
            name: ContractName::Named(name.into()),
            issuer: Identity::default(),
        };
        let genesis = self.builder.issue_genesis(self.schema.codex.codex_id());
        let contract = Contract {
            version: default!(),
            meta,
            codex: self.schema.codex.clone(),
            genesis,
        };
        Articles { contract, contract_sigs: none!(), schema: self.schema }
    }
}

#[derive(Clone, Debug)]
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
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let data = api.build_destructible(name, data, sys);
        let cell = StateCell { data, auth, lock };
        self.destructible
            .push(cell)
            .expect("too many state elements");
        self
    }

    pub fn issue_genesis(self, codex_id: CodexId) -> Genesis {
        Genesis {
            codex_id,
            call_id: self.call_id,
            nonce: fe256::from(u256::ZERO),
            blank1: zero!(),
            blank2: zero!(),
            destructible: self.destructible,
            immutable: self.immutable,
            reserved: zero!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BuilderRef<'c> {
    type_system: &'c TypeSystem,
    api: &'c Api,
    inner: Builder,
}

impl<'c> BuilderRef<'c> {
    pub fn new(api: &'c Api, call_id: CallId, sys: &'c TypeSystem) -> Self {
        BuilderRef { type_system: sys, api, inner: Builder::new(call_id) }
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
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        self.inner = self
            .inner
            .add_destructible(name, auth, data, lock, self.api, self.type_system);
        self
    }

    pub fn issue_genesis(self, codex_id: CodexId) -> Genesis { self.inner.issue_genesis(codex_id) }
}

#[derive(Clone, Debug)]
pub struct OpBuilder {
    contract_id: ContractId,
    destroying: SmallVec<Input>,
    reading: SmallVec<CellAddr>,
    inner: Builder,
}

impl OpBuilder {
    pub fn new(contract_id: ContractId, call_id: CallId) -> Self {
        let inner = Builder::new(call_id);
        Self { contract_id, destroying: none!(), reading: none!(), inner }
    }

    pub fn add_immutable(
        mut self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        self.inner = self.inner.add_immutable(name, data, raw, api, sys);
        self
    }

    pub fn add_destructible(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        self.inner = self
            .inner
            .add_destructible(name, auth, data, lock, api, sys);
        self
    }

    pub fn access(mut self, addr: CellAddr) -> Self {
        self.reading
            .push(addr)
            .expect("number of read memory cells exceeds 64k limit");
        self
    }

    pub fn destroy(mut self, addr: CellAddr, witness: StrictVal) -> Self {
        // TODO: Convert witness
        let input = Input { addr, witness: StateValue::None };
        self.destroying
            .push(input)
            .expect("number of inputs exceeds 64k limit");
        self
    }

    pub fn finalize(self) -> Operation {
        Operation {
            contract_id: self.contract_id,
            call_id: self.inner.call_id,
            nonce: fe256::from(u256::ZERO),
            destroying: self.destroying,
            reading: self.reading,
            destructible: self.inner.destructible,
            immutable: self.inner.immutable,
            reserved: zero!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpBuilderRef<'c> {
    type_system: &'c TypeSystem,
    api: &'c Api,
    inner: OpBuilder,
}

impl<'c> OpBuilderRef<'c> {
    pub fn new(api: &'c Api, contract_id: ContractId, call_id: CallId, sys: &'c TypeSystem) -> Self {
        let inner = OpBuilder::new(contract_id, call_id);
        Self { api, type_system: sys, inner }
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
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        self.inner = self
            .inner
            .add_destructible(name, auth, data, lock, self.api, self.type_system);
        self
    }

    pub fn access(mut self, addr: CellAddr) -> Self {
        self.inner = self.inner.access(addr);
        self
    }

    pub fn destroy(mut self, addr: CellAddr, witness: StrictVal) -> Self {
        self.inner = self.inner.destroy(addr, witness);
        self
    }

    pub fn finalize(self) -> Operation { self.inner.finalize() }
}
