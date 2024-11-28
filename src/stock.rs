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
use sonicapi::{MethodName, OpBuilder, StateName};
use strict_encoding::{StrictWriter, TypeName, WriteRaw};
use strict_types::StrictVal;
use ultrasonic::{fe256, AuthToken, CallError, Capabilities, CellAddr, Operation, Opid};

use crate::{AdaptedState, Aora, Articles, EffectiveState, RawState, Transition};

pub trait Persistence {
    type Stash: Aora<Operation>;
    type Trace: Aora<Transition>;

    fn stash(&self) -> &Self::Stash;
    fn trace(&self) -> &Self::Trace;

    fn stash_mut(&mut self) -> &mut Self::Stash;
    fn trace_mut(&mut self) -> &mut Self::Trace;

    fn save_articles<C: Capabilities>(&self, obj: &Articles<C>);
    fn load_articles<C: Capabilities>(&self) -> Articles<C>;

    fn save_raw_state(&self, state: &RawState);
    fn load_raw_state(&self) -> RawState;

    fn save_state(&self, name: Option<&TypeName>, state: &AdaptedState);
    fn load_state(&self, name: Option<&TypeName>) -> AdaptedState;
}

/// Append-only, random-accessed deeds & trace; updatable and rollback-enabled state.
#[derive(Getters)]
pub struct Stock<C: Capabilities, P: Persistence> {
    articles: Articles<C>,
    state: EffectiveState,

    #[getter(skip)]
    persistence: P,
}

impl<C: Capabilities, P: Persistence> Stock<C, P> {
    pub fn with(articles: Articles<C>, persistence: P) -> Self {
        let mut state = EffectiveState::default();

        let genesis = articles
            .contract
            .genesis
            .to_operation(articles.contract.contract_id());

        state.apply(genesis, &articles.schema.default_api, articles.schema.custom_apis.keys(), &articles.schema.types);

        let mut me = Self { articles, state, persistence };
        me.recompute_state();
        me.save();
        me
    }

    // TODO: Return statistics
    pub fn consume(&mut self, deeds: impl IntoIterator<Item = Operation>) -> Result<(), CallError> {
        for operation in deeds {
            let opid = operation.opid();
            if self.persistence.stash().has(opid) {
                continue;
            }

            self.apply(operation)?;
        }

        self.recompute_state();
        self.save_state();
        Ok(())
    }

    fn recompute_state(&mut self) {
        self.state.main.compute(&self.articles.schema.default_api);
        self.state.aux = bmap! {};
        for api in self.articles.schema.custom_apis.keys() {
            let mut s = AdaptedState::default();
            s.compute(api);
            // TODO: Store API name in map, not in API itself
            self.state
                .aux
                .insert(api.name().cloned().expect("unnamed aux API"), s);
        }
    }

    pub fn consign(&self, terminals: impl IntoIterator<Item = fe256>, writer: &mut StrictWriter<impl WriteRaw>) {
        todo!()
    }

    pub fn rollback(&self, ops: impl IntoIterator<Item = Opid>) { todo!() }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_, C, P> {
        let builder = OpBuilder::new(self.articles.contract.contract_id(), self.articles.schema.call_id(method));
        DeedBuilder { builder, stock: self }
    }

    pub fn apply(&mut self, operation: Operation) -> Result<(), CallError> {
        self.articles.schema.codex.verify(
            self.articles.contract.contract_id(),
            &operation,
            &self.state.raw,
            &self.articles.schema,
        )?;

        self.persistence.stash_mut().append(&operation);

        let transition = self.state.apply(
            operation,
            &self.articles.schema.default_api,
            self.articles.schema.custom_apis.keys(),
            &self.articles.schema.types,
        );
        self.persistence.trace_mut().append(&transition);

        Ok(())
    }

    fn save_state(&self) {
        self.persistence.save_raw_state(&self.state.raw);
        self.persistence.save_state(None, &self.state.main);
        for (name, aux) in &self.state.aux {
            self.persistence.save_state(Some(name), aux);
        }
    }

    pub fn save(&self) {
        self.persistence.save_articles(&self.articles);
        self.save_state();
    }
}

pub struct DeedBuilder<'c, C: Capabilities, P: Persistence> {
    pub(super) builder: OpBuilder,
    pub(super) stock: &'c mut Stock<C, P>,
}

impl<'c, C: Capabilities, P: Persistence> DeedBuilder<'c, C, P> {
    pub fn reading(mut self, addr: CellAddr) -> Self {
        self.builder = self.builder.access(addr);
        self
    }

    pub fn using(mut self, auth: AuthToken, witness: StrictVal) -> Self {
        let addr = self.stock.state.addr(auth);
        self.builder = self.builder.destroy(addr, witness);
        self
    }

    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        let api = &self.stock.articles.schema.default_api;
        let types = &self.stock.articles.schema.types;
        self.builder = self.builder.add_immutable(name, data, raw, api, types);
        self
    }

    pub fn assign(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        let api = &self.stock.articles.schema.default_api;
        let types = &self.stock.articles.schema.types;
        self.builder = self
            .builder
            .add_destructible(name, auth, data, lock, api, types);
        self
    }

    pub fn commit(self) -> Opid {
        let deed = self.builder.finalize();
        let opid = deed.opid();
        if let Err(err) = self.stock.apply(deed) {
            panic!("Invalid operation data: {err}");
        }
        self.stock.recompute_state();
        opid
    }
}

#[cfg(feature = "persist-file")]
mod fs {
    use std::fs;
    use std::path::{Path, PathBuf};

    use ultrasonic::ContractName;

    use super::*;
    use crate::FileAora;

    pub struct FilePersistence {
        path: PathBuf,
        stash: FileAora<Operation>,
        trace: FileAora<Transition>,
    }

    impl FilePersistence {
        pub fn new(name: &str, path: impl AsRef<Path>) -> Self {
            let mut path = path.as_ref().to_path_buf();
            path.push(name);
            path.set_extension("stock");
            fs::create_dir_all(&path).expect("Unable to create directory to store Stock");

            let stash = FileAora::new(&path, "stash");
            let trace = FileAora::new(&path, "trace");

            Self { path, stash, trace }
        }
    }

    impl Persistence for FilePersistence {
        type Stash = FileAora<Operation>;
        type Trace = FileAora<Transition>;

        fn stash(&self) -> &Self::Stash { &self.stash }

        fn trace(&self) -> &Self::Trace { &self.trace }

        fn stash_mut(&mut self) -> &mut Self::Stash { &mut self.stash }

        fn trace_mut(&mut self) -> &mut Self::Trace { &mut self.trace }

        fn save_articles<C: Capabilities>(&self, obj: &Articles<C>) {
            let path = self.path.clone().join("articles.ste");
            obj.save(path).expect("unable to save articles");
        }

        fn load_articles<C: Capabilities>(&self) -> Articles<C> { todo!() }

        fn save_raw_state(&self, state: &RawState) {
            let path = self.path.clone().join("raw.state");
            let file = fs::File::create(path).expect("unable to create state file");
            serde_cbor::to_writer(file, state).expect("unable to serialize state");
        }

        fn load_raw_state(&self) -> RawState { todo!() }

        fn save_state(&self, name: Option<&TypeName>, state: &AdaptedState) {
            let name = match name {
                None => "default",
                Some(n) => n.as_str(),
            };
            let mut path = self.path.clone().join(name);
            path.set_extension("state");
            let file = fs::File::create(path).expect("unable to create state file");
            serde_cbor::to_writer(file, state).expect("unable to serialize state");
        }

        fn load_state(&self, name: Option<&TypeName>) -> AdaptedState { todo!() }
    }

    impl<C: Capabilities> Stock<C, FilePersistence> {
        pub fn new(articles: Articles<C>, path: impl AsRef<Path>) -> Self {
            let name = match &articles.contract.meta.name {
                ContractName::Unnamed => articles.contract_id().to_string(),
                ContractName::Named(name) => name.to_string(),
            };
            let persistence = FilePersistence::new(&name, path);
            Self::with(articles, persistence)
        }

        pub fn open(path: impl AsRef<Path>) -> Self { todo!() }
    }
}
