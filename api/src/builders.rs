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

use std::convert::Infallible;
use std::ops::{Deref, DerefMut};

use amplify::confinement::SmallVec;
use amplify::num::u256;
use chrono::{DateTime, Utc};
use strict_encoding::TypeName;
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{
    fe256, AuthToken, CallId, CellAddr, CellLock, CodexId, Consensus, ContractId, ContractMeta, ContractName, Genesis,
    Identity, Input, Issue, Operation, StateCell, StateData, StateValue,
};

use crate::{Api, Articles, DataCell, Issuer, IssuerId, MethodName, StateAtom, StateName};

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NamedState<T> {
    pub name: StateName,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub state: T,
}

impl NamedState<DataCell> {
    pub fn new_unlocked(name: impl Into<StateName>, auth: impl Into<AuthToken>, data: impl Into<StrictVal>) -> Self {
        NamedState { name: name.into(), state: DataCell::new_unlocked(auth, data) }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CoreParams {
    pub method: MethodName,
    pub global: Vec<NamedState<StateAtom>>,
    pub owned: Vec<NamedState<DataCell>>,
}

impl CoreParams {
    pub fn new(method: impl Into<MethodName>) -> Self {
        Self { method: method.into(), global: none!(), owned: none!() }
    }

    pub fn push_global_verified(&mut self, name: impl Into<StateName>, state: impl Into<StateAtom>) {
        self.global
            .push(NamedState { name: name.into(), state: state.into() });
    }

    pub fn push_owned_unlocked(
        &mut self,
        name: impl Into<StateName>,
        auth: impl Into<AuthToken>,
        data: impl Into<StrictVal>,
    ) {
        self.owned.push(NamedState::new_unlocked(name, auth, data));
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", untagged))]
pub enum VersionRange {
    Range { min: u16, max: u16 },
    After { min: u16 },
    Before { max: u16 },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, From)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", untagged))]
pub enum IssuerSpec {
    #[from]
    Exact(IssuerId),

    #[from]
    Latest(CodexId),

    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    ExactVer { codex_id: CodexId, version: u16 },

    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    VersionRange { codex_id: CodexId, version: VersionRange },
}

impl IssuerSpec {
    pub fn check(&self, issuer_id: IssuerId) -> bool {
        match self {
            IssuerSpec::Exact(id) => *id == issuer_id,
            IssuerSpec::Latest(codex_id) => *codex_id == issuer_id.codex_id,
            IssuerSpec::ExactVer { codex_id, version } => {
                *codex_id == issuer_id.codex_id && *version == issuer_id.version
            }
            IssuerSpec::VersionRange { codex_id, version: VersionRange::After { min } } => {
                *codex_id == issuer_id.codex_id && issuer_id.version >= *min
            }
            IssuerSpec::VersionRange { codex_id, version: VersionRange::Before { max } } => {
                *codex_id == issuer_id.codex_id && issuer_id.version < *max
            }
            IssuerSpec::VersionRange { codex_id, version: VersionRange::Range { min, max } } => {
                *codex_id == issuer_id.codex_id && (*min..*max).contains(&issuer_id.version)
            }
        }
    }

    pub fn codex_id(&self) -> CodexId {
        match self {
            IssuerSpec::Exact(id) => id.codex_id,
            IssuerSpec::Latest(codex_id) => *codex_id,
            IssuerSpec::ExactVer { codex_id, .. } => *codex_id,
            IssuerSpec::VersionRange { codex_id, .. } => *codex_id,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IssueParams {
    pub issuer: IssuerSpec,
    pub name: TypeName,
    pub consensus: Consensus,
    pub testnet: bool,
    pub timestamp: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub core: CoreParams,
}

impl Deref for IssueParams {
    type Target = CoreParams;

    fn deref(&self) -> &Self::Target { &self.core }
}

impl DerefMut for IssueParams {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.core }
}

impl IssueParams {
    pub fn new_testnet(codex_id: CodexId, name: impl Into<TypeName>, consensus: Consensus) -> Self {
        Self {
            issuer: IssuerSpec::Latest(codex_id),
            name: name.into(),
            consensus,
            testnet: true,
            timestamp: None,
            core: CoreParams::new("issue"),
        }
    }

    pub fn set_timestamp(&mut self, timestamp: DateTime<Utc>) { self.timestamp = Some(timestamp); }

    pub fn set_timestamp_now(&mut self) { self.timestamp = Some(Utc::now()); }
}

impl Issuer {
    pub fn start_issue(self, method: impl Into<MethodName>, consensus: Consensus, testnet: bool) -> IssueBuilder {
        let builder = Builder::new(self.call_id(method));
        IssueBuilder { builder, issuer: self, testnet, consensus }
    }

    pub fn start_issue_mainnet(self, method: impl Into<MethodName>, consensus: Consensus) -> IssueBuilder {
        self.start_issue(method, consensus, false)
    }
    pub fn start_issue_testnet(self, method: impl Into<MethodName>, consensus: Consensus) -> IssueBuilder {
        self.start_issue(method, consensus, true)
    }

    pub fn issue(self, params: IssueParams) -> Articles {
        if !params.issuer.check(self.issuer_id()) {
            panic!("issuer version does not match requested version");
        }

        let mut builder = self.start_issue(params.core.method, params.consensus, params.testnet);

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
    issuer: Issuer,
    testnet: bool,
    consensus: Consensus,
}

impl IssueBuilder {
    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.builder = self
            .builder
            .add_global(name, data, raw, self.issuer.default_api(), self.issuer.types());
        self
    }

    pub fn assign(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<CellLock>,
    ) -> Self {
        self.builder = self
            .builder
            .add_owned(name, auth, data, lock, self.issuer.default_api(), self.issuer.types());
        self
    }

    pub fn finish(self, name: impl Into<TypeName>, timestamp: i64) -> Articles {
        let meta = ContractMeta {
            consensus: self.consensus,
            testnet: self.testnet,
            timestamp,
            features: default!(),
            name: ContractName::Named(name.into()),
            issuer: Identity::default(),
        };
        let genesis = self.builder.issue_genesis(self.issuer.codex_id());
        let (codex, semantics) = self.issuer.dismember();
        let issue = Issue { version: default!(), meta, codex, genesis };
        Articles::with(semantics, issue, None, |_, _, _| -> Result<_, Infallible> { unreachable!() })
            .expect("broken issue builder")
    }
}

#[derive(Clone, Debug)]
pub struct Builder {
    call_id: CallId,
    destructible_out: SmallVec<StateCell>,
    immutable_out: SmallVec<StateData>,
}

impl Builder {
    pub fn new(call_id: CallId) -> Self { Builder { call_id, destructible_out: none!(), immutable_out: none!() } }

    pub fn add_global(
        mut self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let name = name.into();
        let data = api
            .build_immutable(name.clone(), data, raw, sys)
            .unwrap_or_else(|e| panic!("invalid immutable state '{name}'; {e}"));
        self.immutable_out
            .push(data)
            .expect("too many state elements");
        self
    }

    pub fn add_owned(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<CellLock>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let data = api
            .build_destructible(name, data, sys)
            .expect("invalid destructible state");
        let cell = StateCell { data, auth, lock };
        self.destructible_out
            .push(cell)
            .expect("too many state elements");
        self
    }

    pub fn issue_genesis(self, codex_id: CodexId) -> Genesis {
        Genesis {
            version: default!(),
            codex_id,
            call_id: self.call_id,
            nonce: fe256::from(u256::ZERO),
            blank0: zero!(),
            blank1: zero!(),
            blank2: zero!(),
            destructible_out: self.destructible_out,
            immutable_out: self.immutable_out,
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

    pub fn add_global(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.inner = self
            .inner
            .add_global(name, data, raw, self.api, self.type_system);
        self
    }

    pub fn add_owned(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<CellLock>,
    ) -> Self {
        self.inner = self
            .inner
            .add_owned(name, auth, data, lock, self.api, self.type_system);
        self
    }

    pub fn issue_genesis(self, codex_id: CodexId) -> Genesis { self.inner.issue_genesis(codex_id) }
}

#[derive(Clone, Debug)]
pub struct OpBuilder {
    contract_id: ContractId,
    destructible_in: SmallVec<Input>,
    immutable_in: SmallVec<CellAddr>,
    inner: Builder,
}

impl OpBuilder {
    pub fn new(contract_id: ContractId, call_id: CallId) -> Self {
        let inner = Builder::new(call_id);
        Self {
            contract_id,
            destructible_in: none!(),
            immutable_in: none!(),
            inner,
        }
    }

    pub fn add_global(
        mut self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        self.inner = self.inner.add_global(name, data, raw, api, sys);
        self
    }

    pub fn add_owned(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<CellLock>,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        self.inner = self.inner.add_owned(name, auth, data, lock, api, sys);
        self
    }

    pub fn access(mut self, addr: CellAddr) -> Self {
        self.immutable_in
            .push(addr)
            .expect("number of read memory cells exceeds 64k limit");
        self
    }

    pub fn destroy(mut self, addr: CellAddr) -> Self {
        let input = Input { addr, witness: StateValue::None };
        self.destructible_in
            .push(input)
            .expect("the number of inputs exceeds the 64k limit");
        self
    }

    pub fn destroy_satisfy(
        mut self,
        addr: CellAddr,
        name: impl Into<StateName>,
        witness: StrictVal,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        let witness = api
            .build_witness(name, witness, sys)
            .expect("invalid witness data");
        let input = Input { addr, witness };
        self.destructible_in
            .push(input)
            .expect("the number of inputs exceeds the 64k limit");
        self
    }

    pub fn finalize(self) -> Operation {
        Operation {
            version: default!(),
            contract_id: self.contract_id,
            call_id: self.inner.call_id,
            nonce: fe256::from(u256::ZERO),
            witness: none!(),
            destructible_in: self.destructible_in,
            immutable_in: self.immutable_in,
            destructible_out: self.inner.destructible_out,
            immutable_out: self.inner.immutable_out,
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

    pub fn add_global(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.inner = self
            .inner
            .add_global(name, data, raw, self.api, self.type_system);
        self
    }

    pub fn add_owned(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<CellLock>,
    ) -> Self {
        self.inner = self
            .inner
            .add_owned(name, auth, data, lock, self.api, self.type_system);
        self
    }

    pub fn access(mut self, addr: CellAddr) -> Self {
        self.inner = self.inner.access(addr);
        self
    }

    pub fn destroy(mut self, addr: CellAddr) -> Self {
        self.inner = self.inner.destroy(addr);
        self
    }

    pub fn destroy_satisfy(
        mut self,
        addr: CellAddr,
        name: impl Into<StateName>,
        witness: StrictVal,
        api: &Api,
        sys: &TypeSystem,
    ) -> Self {
        self.inner = self.inner.destroy_satisfy(addr, name, witness, api, sys);
        self
    }

    pub fn finalize(self) -> Operation { self.inner.finalize() }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use core::str::FromStr;

    use super::*;
    use crate::ApisChecksum;

    #[test]
    fn issuer_spec_yaml_latest() {
        let val = IssuerSpec::Latest(strict_dumb!());
        let s = "AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_spec_yaml_exact() {
        let val = IssuerSpec::Exact(strict_dumb!());
        let s = "\
codexId: AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
version: 0
checksum: AAAAAA
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_spec_yaml_ver_nosum() {
        let val = IssuerSpec::ExactVer { codex_id: strict_dumb!(), version: 0 };
        let s = "\
codexId: AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
version: 0
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_spec_yaml_min() {
        let val = IssuerSpec::VersionRange {
            codex_id: strict_dumb!(),
            version: VersionRange::After { min: 0 },
        };
        let s = "\
codexId: AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
version:
  min: 0
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_spec_yaml_max() {
        let val = IssuerSpec::VersionRange {
            codex_id: strict_dumb!(),
            version: VersionRange::Before { max: 1 },
        };
        let s = "\
codexId: AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
version:
  max: 1
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_spec_yaml_range() {
        let val = IssuerSpec::VersionRange {
            codex_id: strict_dumb!(),
            version: VersionRange::Range { min: 0, max: 1 },
        };
        let s = "\
codexId: AAAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA#origami-bruno-life
version:
  min: 0
  max: 1
";
        assert_eq!(serde_yaml::to_string(&val).unwrap(), s);
        assert_eq!(serde_yaml::from_str::<IssuerSpec>(s).unwrap(), val);
    }

    #[test]
    fn issuer_display_fromstr() {
        let s = "nmThRWDr-0hOJgJt-OFVCZTA-XX8aOWj-bkqWzK7-_jAtdhQ/0#NRIsWA";
        let issuer_id = IssuerId::from_str(s).unwrap();
        assert_eq!(issuer_id.to_string(), s);
        assert_eq!(issuer_id.codex_id, CodexId::from_str(s.split_once("/").unwrap().0).unwrap());
        assert_eq!(issuer_id.checksum, ApisChecksum::from_str(s.split_once("#").unwrap().1).unwrap());
        assert_eq!(issuer_id.version, 0);
    }

    #[test]
    fn issuer_check() {
        let s = "nmThRWDr-0hOJgJt-OFVCZTA-XX8aOWj-bkqWzK7-_jAtdhQ/0#NRIsWA";
        let issuer_id = IssuerId::from_str(s).unwrap();
        assert!(IssuerSpec::Exact(issuer_id).check(issuer_id));

        let mut changed_ver = issuer_id;
        changed_ver.version += 1;
        assert!(!IssuerSpec::Exact(issuer_id).check(changed_ver));
        assert!(IssuerSpec::Latest(issuer_id.codex_id).check(changed_ver));
        assert!(!IssuerSpec::ExactVer { codex_id: issuer_id.codex_id, version: issuer_id.version }.check(changed_ver));

        let mut changed_sum = issuer_id;
        changed_sum.checksum = ApisChecksum::from_str("rLAuRQ").unwrap();
        assert!(!IssuerSpec::Exact(issuer_id).check(changed_sum));
        assert!(IssuerSpec::Latest(issuer_id.codex_id).check(changed_sum));
        assert!(IssuerSpec::ExactVer { codex_id: issuer_id.codex_id, version: issuer_id.version }.check(changed_sum));
    }
}
