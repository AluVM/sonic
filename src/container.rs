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

use core::fmt::Debug;

use aluvm::{Lib, LibId, LibSite};
use amplify::confinement::{LargeVec, SmallOrdMap, SmallOrdSet, TinyOrdMap};
use commit_verify::ReservedBytes;
use sonicapi::{Api, MethodName, StateName};
use strict_encoding::{StrictDecode, StrictDeserialize, StrictDumb, StrictEncode, StrictSerialize, TypeName};
use strict_types::{StrictVal, TypeSystem};
use ultrasonic::{fe128, CallId, CellAddr, Codex, Identity, LibRepo, Operation, Opid, ProofOfPubl};

use crate::annotations::Annotations;
use crate::sigs::ContentSigs;
use crate::state::RawState;
use crate::{Builder, Contract, ContractMeta, ContractName, ContractState, OpBuilderRef, LIB_NAME_SONIC};

pub type Issuer = Container<()>;
pub type Deeds<PoP> = Container<ContractDeeds<PoP>>;

pub trait ContainerPayload: Clone + Eq + Debug + StrictDumb + StrictEncode + StrictDecode {}
impl ContainerPayload for () {}
impl<PoP: ProofOfPubl> ContainerPayload for ContractDeeds<PoP> {}

#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct Container<D: ContainerPayload> {
    pub codex: Codex,
    pub payload: D,
    pub default_api: Api,
    pub default_api_sigs: ContentSigs,
    pub custom_apis: SmallOrdMap<Api, ContentSigs>,
    pub libs: SmallOrdSet<Lib>,
    pub types: TypeSystem,
    pub codex_sigs: ContentSigs,
    pub annotations: TinyOrdMap<Annotations, ContentSigs>,
    pub reserved: ReservedBytes<8>,
}

impl<D: ContainerPayload> StrictSerialize for Container<D> {}
impl<D: ContainerPayload> StrictDeserialize for Container<D> {}

impl<D: ContainerPayload> LibRepo for Container<D> {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> { self.libs.iter().find(|lib| lib.lib_id() == lib_id) }
}

impl<D: ContainerPayload> Container<D> {
    fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        self.default_api
            .verifier(method)
            .expect("unknown issue method absent in Codex API")
    }
}

#[derive(Clone, Debug)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct ContractDeeds<PoP: ProofOfPubl> {
    pub contract: Contract<PoP>,
    pub operations: LargeVec<Operation>,
    pub contract_sigs: ContentSigs,
    #[strict_type(skip)]
    pub state: Option<ContractState>,
}

impl<PoP: ProofOfPubl> Eq for ContractDeeds<PoP> {}
impl<PoP: ProofOfPubl> PartialEq for ContractDeeds<PoP> {
    fn eq(&self, other: &Self) -> bool {
        self.contract.eq(&other.contract)
            && self.operations.eq(&other.operations)
            && self.contract_sigs.eq(&other.contract_sigs)
    }
}

impl<PoP: ProofOfPubl> StrictDumb for ContractDeeds<PoP> {
    fn strict_dumb() -> Self {
        Self {
            contract: strict_dumb!(),
            operations: strict_dumb!(),
            contract_sigs: strict_dumb!(),
            state: None,
        }
    }
}

impl<PoP: ProofOfPubl> ContractDeeds<PoP> {
    fn deeds_state_mut(&mut self) -> (&mut LargeVec<Operation>, &mut RawState) {
        (
            &mut self.operations,
            &mut self
                .state
                .as_mut()
                .expect("contract state must be present")
                .raw,
        )
    }
}

impl Issuer {
    pub fn new(codex: Codex, api: Api, libs: impl IntoIterator<Item = Lib>, types: TypeSystem) -> Self {
        // TODO: Ensure default API is unnamed?
        Issuer {
            codex,
            payload: (),
            default_api: api,
            default_api_sigs: none!(),
            custom_apis: none!(),
            libs: SmallOrdSet::from_iter_checked(libs),
            types,
            codex_sigs: none!(),
            annotations: none!(),
            reserved: zero!(),
        }
    }

    pub fn start_issue(self, method: impl Into<MethodName>) -> IssueBuilder {
        let builder = Builder::new(self.call_id(method));
        IssueBuilder { builder, issuer: self }
    }
}

impl<PoP: ProofOfPubl> Deeds<PoP> {
    pub fn computed_state(&self) -> &ContractState {
        match &self.payload.state {
            Some(state) => state,
            None => todo!("compute state"),
        }
    }

    pub fn read(&self, reader: impl Into<StateName>) -> StrictVal {
        let state = self.computed_state();
        let empty = bmap![];
        self.default_api
            .read(reader, |name| match state.main.immutable(&name) {
                None => empty.values(),
                Some(src) => src.values(),
            })
    }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_> {
        let builder = OpBuilderRef::new(
            &self.default_api,
            self.payload.contract.contract_id(),
            self.call_id(method),
            &self.types,
        );
        self.computed_state();
        let (deeds, state) = self.payload.deeds_state_mut();
        DeedBuilder { builder, deeds, state }
    }

    pub fn apply(&mut self, op: Operation) {
        //let state = self.computed_state();
        // TODO: Enable verification
        // self.codex
        //    .verify(self.payload.contract.contract_id(), &op, state, self)
        //    .expect("invalid genesis data");
        let state = self.payload.state.as_mut().unwrap();
        state.main.apply(&op, &self.default_api, &self.types);
        for api in self.custom_apis.keys() {
            // TODO: Remove name from API itself.
            let Some(name) = api.name() else {
                continue;
            };
            let state = state.apis.entry(name.clone()).or_default();
            state.apply(&op, api, &self.types);
        }
        state.raw.apply(op);
    }
}

pub struct IssueBuilder {
    builder: Builder,
    issuer: Issuer,
}

impl IssueBuilder {
    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.builder = self
            .builder
            .add_immutable(name, data, raw, &self.issuer.default_api, &self.issuer.types);
        self
    }

    pub fn assign(mut self, name: impl Into<StateName>, seal: fe128, data: StrictVal, lock: Option<LibSite>) -> Self {
        self.builder =
            self.builder
                .add_destructible(name, seal, data, lock, &self.issuer.default_api, &self.issuer.types);
        self
    }

    pub fn finish<PoP: ProofOfPubl + Default>(self, name: impl Into<TypeName>) -> Deeds<PoP> {
        let meta = ContractMeta {
            proof_of_publ: default!(),
            reserved: zero!(),
            salt: rand::random(),
            timestamp: chrono::Utc::now().timestamp(),
            name: ContractName::Named(name.into()),
            issuer: Identity::default(),
        };
        let genesis = self.builder.issue_genesis(self.issuer.codex.codex_id());
        let contract = Contract {
            version: default!(),
            meta,
            codex: self.issuer.codex.clone(),
            genesis,
        };
        let contract_id = contract.contract_id();
        let state = ContractState::default();
        let genesis_op = contract.genesis.to_operation(contract_id);
        let mut deeds = Deeds {
            codex: self.issuer.codex,
            payload: ContractDeeds {
                contract,
                operations: none!(),
                contract_sigs: none!(),
                state: Some(state),
            },
            default_api: self.issuer.default_api,
            default_api_sigs: self.issuer.default_api_sigs,
            custom_apis: self.issuer.custom_apis,
            libs: self.issuer.libs,
            types: self.issuer.types,
            codex_sigs: self.issuer.codex_sigs,
            annotations: self.issuer.annotations,
            reserved: self.issuer.reserved,
        };
        deeds.apply(genesis_op);
        deeds
    }
}

pub struct DeedBuilder<'c> {
    builder: OpBuilderRef<'c>,
    deeds: &'c mut LargeVec<Operation>,
    state: &'c mut RawState,
}

impl<'c> DeedBuilder<'c> {
    pub fn reading(mut self, addr: CellAddr) -> Self {
        self.builder = self.builder.access(addr);
        self
    }

    pub fn using(mut self, seal: fe128, witness: StrictVal) -> Self {
        let addr = self.state.seal_addr(seal);
        self.builder = self.builder.destroy(addr, witness);
        self
    }

    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        self.builder = self.builder.add_immutable(name, data, raw);
        self
    }

    pub fn assign(mut self, name: impl Into<StateName>, seal: fe128, data: StrictVal, lock: Option<LibSite>) -> Self {
        self.builder = self.builder.add_destructible(name, seal, data, lock);
        self
    }

    pub fn commit(self) -> Opid {
        let deed = self.builder.finalize();
        let opid = deed.opid();
        // TODO: Verify state
        self.state.apply(deed.clone());
        self.deeds
            .push(deed)
            .expect("more than 4 billions of deeds are not supported");
        opid
    }
}

#[cfg(feature = "std")]
mod _fs {
    use std::path::Path;

    use strict_encoding::{SerializeError, StrictSerialize};

    use crate::{Container, ContainerPayload};

    // TODO: Compute/verify state on load from file

    impl<D: ContainerPayload> Container<D> {
        pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SerializeError> {
            self.strict_serialize_to_file::<{ usize::MAX }>(path)
        }
    }
}
