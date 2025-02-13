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

use alloc::collections::{BTreeMap, BTreeSet};
use core::borrow::Borrow;
// Used in strict encoding; once solved there, remove here
use std::io;
use std::io::ErrorKind;

use aluvm::LibSite;
use sonic_callreq::{MethodName, StateName};
use sonicapi::{CoreParams, MergeError, NamedState, OpBuilder};
use strict_encoding::{DecodeError, ReadRaw, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw};
use strict_types::StrictVal;
use ultrasonic::{AuthToken, CallError, CellAddr, ContractId, Operation, Opid};

use crate::aora::Aora;
use crate::{Articles, EffectiveState, RawState, Transition};

/// NB: Methods in the trait do not error; instead, they must perform all operations asynchronously,
/// for instance in a separate thread or using channels; and in case of error the error must be
/// reported elsewhere (via logging, or using a dedicated error reporting microservice).
pub trait Supply {
    type Stash: Aora<Id = Opid, Item = Operation>;
    type Trace: Aora<Id = Opid, Item = Transition>;

    fn stash(&self) -> &Self::Stash;
    fn trace(&self) -> &Self::Trace;

    fn stash_mut(&mut self) -> &mut Self::Stash;
    fn trace_mut(&mut self) -> &mut Self::Trace;

    fn save_articles(&self, obj: &Articles);
    fn load_articles(&self) -> Articles;

    fn save_state(&self, state: &RawState);
    fn load_state(&self) -> RawState;
}

/// Append-only, random-accessed deeds & trace; updatable and rollback-enabled state.
#[derive(Getters)]
pub struct Stock<S: Supply> {
    articles: Articles,
    state: EffectiveState,

    #[getter(skip)]
    supply: S,
}

impl<S: Supply> Stock<S> {
    pub fn create(articles: Articles, persistence: S) -> Self {
        let mut state = EffectiveState::default();

        let genesis = articles
            .contract
            .genesis
            .to_operation(articles.contract.contract_id());

        // We do not need state transition for geneis.
        let _ = state.apply(
            genesis,
            &articles.schema.default_api,
            articles.schema.custom_apis.keys(),
            &articles.schema.types,
        );

        let mut me = Self { articles, state, supply: persistence };
        me.recompute_state();
        me.save();
        me
    }

    pub fn open(articles: Articles, persistence: S) -> Self {
        let raw = persistence.load_state();
        let state = EffectiveState::with(raw, &articles.schema);
        Self { articles, state, supply: persistence }
    }

    pub fn contract_id(&self) -> ContractId { self.articles.contract_id() }

    pub fn export_all(&mut self, mut writer: StrictWriter<impl WriteRaw>) -> io::Result<()> {
        // Write articles
        writer = self.articles.strict_encode(writer)?;
        // Stream operations
        for (_, op) in self.operations() {
            writer = op.strict_encode(writer)?;
        }
        Ok(())
    }

    pub fn export(
        &mut self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        writer: StrictWriter<impl WriteRaw>,
    ) -> io::Result<()> {
        self.export_aux(terminals, writer, |_, w| Ok(w))
    }

    // TODO: Return statistics
    pub fn export_aux<W: WriteRaw>(
        &mut self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        mut writer: StrictWriter<W>,
        mut aux: impl FnMut(Opid, StrictWriter<W>) -> io::Result<StrictWriter<W>>,
    ) -> io::Result<()> {
        let queue = terminals
            .into_iter()
            .map(|terminal| self.state.addr(*terminal.borrow()).opid)
            .collect::<BTreeSet<_>>();
        let mut opids = queue.clone();
        for opid in queue {
            let st = self.supply.trace_mut().read(opid);
            opids.extend(st.destroyed.into_keys().map(|a| a.opid));
        }

        // TODO: Include all operations defining published state

        // Write articles
        writer = self.articles.strict_encode(writer)?;
        writer = aux(self.articles.contract.genesis_opid(), writer)?;
        // Stream operations
        for (opid, op) in self.operations() {
            if !opids.contains(&opid) {
                continue;
            }
            writer = op.strict_encode(writer)?;
            writer = aux(opid, writer)?;
        }

        Ok(())
    }

    pub fn merge_articles(&mut self, articles: Articles) -> Result<(), MergeError> {
        self.articles.merge(articles)?;
        self.supply.save_articles(&self.articles);
        Ok(())
    }

    pub fn import(&mut self, reader: &mut StrictReader<impl ReadRaw>) -> Result<(), AcceptError> {
        let articles = Articles::strict_decode(reader)?;
        self.articles.merge(articles)?;

        loop {
            let op = match Operation::strict_decode(reader) {
                Ok(operation) => operation,
                Err(DecodeError::Io(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };
            self.check_apply(op)?;
        }
        self.recompute_state();
        self.save_state();
        Ok(())
    }

    pub fn rollback(&self, ops: impl IntoIterator<Item = Opid>) { todo!() }

    pub fn has_operation(&self, opid: Opid) -> bool { self.supply.stash().has(&opid) }

    pub fn operations(&mut self) -> impl Iterator<Item = (Opid, Operation)> + use<'_, S> {
        self.supply.stash_mut().iter()
    }

    pub fn operation(&mut self, opid: Opid) -> Operation { self.supply.stash_mut().read(opid) }

    pub fn trace(&mut self) -> impl Iterator<Item = (Opid, Transition)> + use<'_, S> { self.supply.trace_mut().iter() }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_, S> {
        let builder = OpBuilder::new(self.articles.contract.contract_id(), self.articles.schema.call_id(method));
        DeedBuilder { builder, stock: self }
    }

    pub fn call(&mut self, params: CallParams) -> Opid {
        let mut builder = self.start_deed(params.core.method);

        for NamedState { name, state } in params.core.global {
            builder = builder.append(name, state.verified, state.unverified);
        }
        for NamedState { name, state } in params.core.owned {
            builder = builder.assign(name, state.auth, state.data, state.lock);
        }
        for addr in params.reading {
            builder = builder.reading(addr);
        }
        for (addr, witness) in params.using {
            builder = builder.using(addr, witness);
        }

        builder.commit()
    }

    /// # Returns
    ///
    /// Whether operation was already successfully included (`true`), or was already present in the
    /// stash.
    fn check_apply(&mut self, operation: Operation) -> Result<bool, AcceptError> {
        if operation.contract_id != self.contract_id() {
            return Err(AcceptError::ContractMismatch);
        }

        let opid = operation.opid();

        if self.supply.stash().has(&opid) {
            return Ok(false);
        }

        self.articles.schema.codex.verify(
            self.articles.contract.contract_id(),
            &operation,
            &self.state.raw,
            &self.articles.schema,
        )?;

        self.apply(operation);

        Ok(true)
    }

    pub fn apply(&mut self, operation: Operation) {
        let opid = operation.opid();
        self.supply.stash_mut().append(opid, &operation);

        let transition = self.state.apply(
            operation,
            &self.articles.schema.default_api,
            self.articles.schema.custom_apis.keys(),
            &self.articles.schema.types,
        );
        self.supply.trace_mut().append(opid, &transition);
    }

    pub fn complete_update(&mut self) {
        self.recompute_state();
        self.save_state();
    }

    /// Recalculates computable part of the state
    fn recompute_state(&mut self) {
        self.state
            .recompute(&self.articles.schema.default_api, self.articles.schema.custom_apis.keys());
    }

    fn save_state(&self) { self.supply.save_state(&self.state.raw); }

    pub fn save(&self) {
        self.supply.save_articles(&self.articles);
        self.save_state();
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CallParams {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub core: CoreParams,
    pub using: BTreeMap<CellAddr, StrictVal>,
    pub reading: Vec<CellAddr>,
}

pub struct DeedBuilder<'c, S: Supply> {
    pub(super) builder: OpBuilder,
    pub(super) stock: &'c mut Stock<S>,
}

impl<S: Supply> DeedBuilder<'_, S> {
    pub fn reading(mut self, addr: CellAddr) -> Self {
        self.builder = self.builder.access(addr);
        self
    }

    pub fn using(mut self, addr: CellAddr, witness: StrictVal) -> Self {
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
        if let Err(err) = self.stock.check_apply(deed) {
            panic!("Invalid operation data: {err}");
        }
        self.stock.recompute_state();
        self.stock.save_state();
        opid
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error, From)]
#[display(inner)]
pub enum AcceptError {
    #[display("contract id doesn't match")]
    ContractMismatch,

    #[from]
    Articles(MergeError),

    #[from]
    Verify(CallError),

    #[from]
    #[cfg_attr(feature = "std", from(std::io::Error))]
    Decode(DecodeError),
}

#[cfg(feature = "persist-file")]
pub mod fs {
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};

    use amplify::confinement::U64 as U64MAX;
    use strict_encoding::{StreamReader, StreamWriter, StrictDeserialize, StrictSerialize};
    use ultrasonic::ContractName;

    use super::*;
    use crate::aora::file::FileAora;

    pub struct FileSupply {
        path: PathBuf,
        stash: FileAora<Opid, Operation>,
        trace: FileAora<Opid, Transition>,
    }

    impl FileSupply {
        const FILENAME_ARTICLES: &'static str = "contract.articles";
        const FILENAME_STATE_RAW: &'static str = "state.dat";

        pub fn new(name: &str, path: impl AsRef<Path>) -> Self {
            let mut path = path.as_ref().to_path_buf();
            path.push(name);
            path.set_extension("contract");
            fs::create_dir_all(&path).expect("Unable to create directory to store Stock");

            let stash = FileAora::new(&path, "stash");
            let trace = FileAora::new(&path, "trace");

            Self { path, stash, trace }
        }

        pub fn open(path: impl AsRef<Path>) -> Self {
            let path = path.as_ref().to_path_buf();
            let stash = FileAora::open(&path, "stash");
            let trace = FileAora::open(&path, "trace");
            Self { path, stash, trace }
        }
    }

    impl Supply for FileSupply {
        type Stash = FileAora<Opid, Operation>;
        type Trace = FileAora<Opid, Transition>;

        fn stash(&self) -> &Self::Stash { &self.stash }

        fn trace(&self) -> &Self::Trace { &self.trace }

        fn stash_mut(&mut self) -> &mut Self::Stash { &mut self.stash }

        fn trace_mut(&mut self) -> &mut Self::Trace { &mut self.trace }

        fn save_articles(&self, obj: &Articles) {
            let path = self.path.clone().join(Self::FILENAME_ARTICLES);
            obj.save(path).expect("unable to save articles");
        }

        fn load_articles(&self) -> Articles {
            let path = self.path.clone().join(Self::FILENAME_ARTICLES);
            Articles::load(path).expect("unable to load articles")
        }

        fn save_state(&self, state: &RawState) {
            let path = self.path.clone().join(Self::FILENAME_STATE_RAW);
            state
                .strict_serialize_to_file::<U64MAX>(path)
                .expect("unable to serialize state");
        }

        fn load_state(&self) -> RawState {
            let path = self.path.clone().join(Self::FILENAME_STATE_RAW);
            RawState::strict_deserialize_from_file::<U64MAX>(path).expect("unable to load state")
        }
    }

    impl Stock<FileSupply> {
        pub fn new(articles: Articles, path: impl AsRef<Path>) -> Self {
            let name = match &articles.contract.meta.name {
                ContractName::Unnamed => articles.contract_id().to_string(),
                ContractName::Named(name) => name.to_string(),
            };
            let persistence = FileSupply::new(&name, path);
            Self::create(articles, persistence)
        }

        pub fn load(path: impl AsRef<Path>) -> Self {
            let path = path.as_ref();
            let persistence = FileSupply::open(path);
            Self::open(persistence.load_articles(), persistence)
        }

        pub fn backup_to_file(&mut self, output: impl AsRef<Path>) -> io::Result<()> {
            let file = File::create_new(output)?;
            let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
            self.export_all(writer)
        }

        pub fn export_to_file(
            &mut self,
            terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
            output: impl AsRef<Path>,
        ) -> io::Result<()> {
            let file = File::create_new(output)?;
            let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
            self.export(terminals, writer)
        }

        pub fn accept_from_file(&mut self, input: impl AsRef<Path>) -> Result<(), AcceptError> {
            let file = File::open(input)?;
            let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
            self.import(&mut reader)
        }
    }
}
