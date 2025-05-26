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

use alloc::collections::BTreeSet;
use core::borrow::Borrow;
use std::io;

use amplify::MultiError;
use commit_verify::StrictHash;
use indexmap::IndexSet;
use sonic_callreq::MethodName;
use sonicapi::{Api, NamedState, OpBuilder, SemanticError, Semantics, SigBlob};
use strict_encoding::{
    DecodeError, ReadRaw, SerializeError, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw,
};
use ultrasonic::{AuthToken, CallError, CellAddr, ContractId, Identity, Issue, Operation, Opid, VerifiedOperation};

use crate::deed::{CallParams, DeedBuilder};
use crate::{Articles, EffectiveState, IssueError, ProcessedState, Stock, Transition};

/// Contract with all its state and operations, supporting updates and rollbacks.
// We need this structure to hide internal persistence methods and not to expose them.
// We need the persistence trait (`Stock`) in order to allow different persistence storage
// implementations.
#[derive(Clone, Debug)]
pub struct Ledger<S: Stock>(S, /** Cached value */ ContractId);

impl<S: Stock> Ledger<S> {
    /// Instantiates a new contract from the provided articles, creating its persistence with the
    /// provided configuration.
    ///
    /// # Panics
    ///
    /// This call must not panic, and instead must return an error.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY perform any I/O operations.
    pub fn new(articles: Articles, conf: S::Conf) -> Result<Self, MultiError<IssueError, S::Error>> {
        let contract_id = articles.contract_id();
        let state = EffectiveState::with_articles(&articles)
            .map_err(|e| IssueError::Genesis(articles.issue().meta.name.clone(), e))
            .map_err(MultiError::A)?;
        let mut stock = S::new(articles, state, conf).map_err(MultiError::B)?;
        let genesis_opid = stock.articles().genesis_opid();
        stock.mark_valid(genesis_opid);
        stock.commit_transaction();
        Ok(Self(stock, contract_id))
    }

    /// Loads a contract using the provided configuration for persistence.
    ///
    /// # Panics
    ///
    /// This call must not panic, and instead must return an error.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY perform any I/O operations.
    pub fn load(conf: S::Conf) -> Result<Self, S::Error> {
        S::load(conf).map(|stock| {
            let contract_id = stock.articles().contract_id();
            Self(stock, contract_id)
        })
    }

    pub fn config(&self) -> S::Conf { self.0.config() }

    pub fn stock(&self) -> &S { &self.0 }

    /// Provides contract id.
    ///
    /// The contract id value is cached; thus, calling this operation is inexpensive.
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    #[inline]
    pub fn contract_id(&self) -> ContractId { self.1 }

    /// Provides contract [`Articles`], which include contract genesis.
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    #[inline]
    pub fn articles(&self) -> &Articles { self.0.articles() }

    /// Provides contract [`EffectiveState`].
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    #[inline]
    pub fn state(&self) -> &EffectiveState { self.0.state() }

    /// Detects whether an operation with a given `opid` participates in the current state.
    pub fn is_valid(&self, opid: Opid) -> bool { self.0.is_valid(opid) }

    /// Detects whether an operation with a given `opid` is known to the contract.
    ///
    /// # Nota bene
    ///
    /// Does not include genesis operation id.
    ///
    /// Positive response doesn't indicate that the operation participates in the current contract
    /// state or in a current valid contract history, which may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Ledger::rollback`]),
    /// as well as re-included later with forwards (see [`Ledger::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    #[inline]
    pub fn has_operation(&self, opid: Opid) -> bool { self.0.has_operation(opid) }

    /// Returns an operation ([`Operation`]) with a given `opid` from the set of known contract
    /// operations ("stash").
    ///
    /// # Nota bene
    ///
    /// Does not include genesis operation.
    ///
    /// If the method returns an operation, this doesn't indicate that the operation participates in
    /// the current contract state or in a current valid contract history, which/ may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Ledger::rollback`]),
    /// as well as re-included later with forwards (see [`Ledger::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// If an `opid` is not present in the contract stash, or it corresponds to the genesis
    /// operation.
    ///
    /// In order to avoid panics always call the method after calling `has_operation`.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    #[inline]
    pub fn operation(&self, opid: Opid) -> Operation { self.0.operation(opid) }

    /// Returns an iterator over all operations known to the contract (i.e., the complete contract
    /// stash).
    ///
    /// # Nota bene
    ///
    /// Does not include genesis operation.
    ///
    /// Contract stash is a broader concept than contract history. It includes operations which may
    /// not contribute to the current contract state or participate in the contract history, which
    /// may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Ledger::rollback`]),
    /// as well as re-included later with forwards (see [`Ledger::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// The method MUST NOT panic
    ///
    /// # Blocking I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    #[inline]
    pub fn operations(&self) -> impl Iterator<Item = (Opid, Operation)> + use<'_, S> { self.0.operations() }

    /// Returns an iterator over all state transitions known to the contract (i.e., the complete
    /// contract trace).
    ///
    /// # Nota bene
    ///
    /// Contract trace is a broader concept than contract history. It includes state transition
    /// which may not contribute to the current contract state or participate in the contract
    /// history, which may be exported.
    ///
    /// State transitions may be excluded from the history due to rollbacks (see
    /// [`Ledger::rollback`]), as well as re-included later with forwards (see
    /// [`Ledger::forward`]). In both cases corresponding state transitions are kept in the
    /// contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// The method MUST NOT panic
    ///
    /// # Blocking I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    #[inline]
    pub fn trace(&self) -> impl Iterator<Item = (Opid, Transition)> + use<'_, S> { self.0.trace() }

    #[inline]
    pub fn read_by(&self, addr: CellAddr) -> impl Iterator<Item = Opid> + use<'_, S> { self.0.read_by(addr) }
    #[inline]
    pub fn spent_by(&self, addr: CellAddr) -> Option<Opid> { self.0.spent_by(addr) }

    /// # Nota bene
    ///
    /// Ancestors do include the original operations
    pub fn ancestors(&self, opids: impl IntoIterator<Item = Opid>) -> impl DoubleEndedIterator<Item = Opid> {
        let mut chain = opids.into_iter().collect::<IndexSet<_>>();
        // Get all subsequent operations
        let mut index = 0usize;
        let genesis_opid = self.articles().genesis_opid();
        while let Some(opid) = chain.get_index(index).copied() {
            if opid != genesis_opid {
                let op = self.0.operation(opid);
                for inp in op.immutable_in {
                    let parent = inp.opid;
                    if !chain.contains(&parent) {
                        chain.insert(parent);
                    }
                }
                for inp in op.destructible_in {
                    let parent = inp.addr.opid;
                    if !chain.contains(&parent) {
                        chain.insert(parent);
                    }
                }
            }
            index += 1;
        }
        chain.into_iter()
    }

    /// # Nota bene
    ///
    /// Descendants do include the original operations
    pub fn descendants(&self, opids: impl IntoIterator<Item = Opid>) -> impl DoubleEndedIterator<Item = Opid> {
        let mut chain = opids.into_iter().collect::<IndexSet<_>>();
        // Get all subsequent operations
        let mut index = 0usize;
        while let Some(opid) = chain.get_index(index).copied() {
            let op = self.0.operation(opid);
            for no in 0..op.immutable_out.len_u16() {
                let addr = CellAddr::new(opid, no);
                for read in self.0.read_by(addr) {
                    if !chain.contains(&read) {
                        chain.insert(read);
                    }
                }
            }
            for no in 0..op.destructible_out.len_u16() {
                let addr = CellAddr::new(opid, no);
                let Some(spent) = self.0.spent_by(addr) else { continue };
                if !chain.contains(&spent) {
                    chain.insert(spent);
                }
            }
            index += 1;
        }
        chain.into_iter()
    }

    /// Exports contract with all known operations
    pub fn export_all(&self, writer: StrictWriter<impl WriteRaw>) -> io::Result<()> {
        self.export_internal(self.0.operation_count() as u32, writer, |_| true, |_, _, w| Ok(w))
    }

    /// Export a part of a contract history: a graph between set of terminals and genesis.
    pub fn export(
        &self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        writer: StrictWriter<impl WriteRaw>,
    ) -> io::Result<()> {
        self.export_aux(terminals, writer, |_, _, w| Ok(w))
    }

    /// Exports contract and operations to a stream, extending operation data with some auxiliary
    /// information returned by `aux`.
    // TODO: (v0.13) Return statistics
    pub fn export_aux<W: WriteRaw>(
        &self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        writer: StrictWriter<W>,
        aux: impl FnMut(Opid, &Operation, StrictWriter<W>) -> io::Result<StrictWriter<W>>,
    ) -> io::Result<()> {
        let mut queue = terminals
            .into_iter()
            .map(|terminal| self.0.state().addr(*terminal.borrow()).opid)
            .collect::<BTreeSet<_>>();
        let articles = self.articles();
        let genesis_opid = articles.genesis_opid();
        queue.remove(&genesis_opid);
        let mut opids = queue.clone();
        while let Some(opid) = queue.pop_first() {
            let st = self.0.transition(opid);
            for prev in st.destroyed.into_keys().map(|a| a.opid) {
                if !opids.contains(&prev) && prev != genesis_opid {
                    opids.insert(prev);
                    queue.insert(prev);
                }
            }
        }

        // Include all operations defining published state
        let state = self.state();
        let mut collect = |api: &Api, state: &ProcessedState| {
            for (state_name, owned) in &api.global {
                if owned.published {
                    let Some(cells) = state.global.get(state_name) else {
                        continue;
                    };
                    opids.extend(cells.keys().map(|addr| addr.opid));
                }
            }
        };
        collect(&articles.semantics().default, &state.main);
        for (api_name, api) in &articles.semantics().custom {
            let Some(state) = state.aux.get(api_name) else {
                continue;
            };
            collect(api, state);
        }
        opids.remove(&genesis_opid);

        self.export_internal(opids.len() as u32, writer, |opid| opids.remove(opid), aux)?;

        debug_assert!(
            opids.is_empty(),
            "Missing operations: {}",
            opids
                .into_iter()
                .map(|opid| opid.to_string())
                .collect::<Vec<_>>()
                .join("\n -")
        );

        Ok(())
    }

    /// Exports only operations for which `should_include` returns `true`.
    ///
    /// # Nota bene
    ///
    /// Does not write the contract id.
    pub fn export_internal<W: WriteRaw>(
        &self,
        count: u32,
        mut writer: StrictWriter<W>,
        mut should_include: impl FnMut(&Opid) -> bool,
        mut aux: impl FnMut(Opid, &Operation, StrictWriter<W>) -> io::Result<StrictWriter<W>>,
    ) -> io::Result<()> {
        let articles = self.articles();
        let genesis_opid = articles.genesis_opid();

        // Write contract id
        let contract_id = self.contract_id();
        writer = self.contract_id().strict_encode(writer)?;
        // Write articles
        writer = articles.strict_encode(writer)?;
        writer = aux(genesis_opid, &articles.genesis().to_operation(contract_id), writer)?;
        // Write no of operations
        writer = count.strict_encode(writer)?;
        // Stream operations
        for (opid, op) in self.0.operations() {
            if !should_include(&opid) {
                continue;
            }
            writer = op.strict_encode(writer)?;
            writer = aux(opid, &op, writer)?;
        }
        Ok(())
    }

    pub fn upgrade_apis(&mut self, new_articles: Articles) -> Result<bool, MultiError<SemanticError, S::Error>> {
        self.0
            .update_articles(|articles| articles.upgrade_apis(new_articles))
    }

    pub fn accept<E>(
        &mut self,
        reader: &mut StrictReader<impl ReadRaw>,
        sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
    ) -> Result<(), MultiError<AcceptError, S::Error>> {
        // We need this closure to avoid multiple `map_err`.
        let count = (|| -> Result<u32, AcceptError> {
            let contract_id = ContractId::strict_decode(reader)?;

            let semantics = Semantics::strict_decode(reader)?;
            let sig = Option::<SigBlob>::strict_decode(reader)?;
            let issue = Issue::strict_decode(reader)?;
            let articles = Articles::with(semantics, issue, sig, sig_validator)?;
            if articles.contract_id() != contract_id {
                return Err(AcceptError::Articles(SemanticError::ContractMismatch));
            }

            self.upgrade_apis(articles)
                .map_err(|e| AcceptError::Persistence(e.to_string()))?;

            let count = u32::strict_decode(reader)?;
            Ok(count)
        })()
        .map_err(MultiError::A)?;

        // We need to account for genesis, which is not included in the `count`
        for _ in 0..=count {
            let op = match Operation::strict_decode(reader) {
                Ok(operation) => operation,
                Err(DecodeError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(MultiError::A(e.into())),
            };
            self.apply_verify(op, false)?;
        }
        self.commit_transaction();
        Ok(())
    }

    pub fn rollback(&mut self, opids: impl IntoIterator<Item = Opid>) -> Result<(), S::Error> {
        for opid in self.descendants(opids).rev() {
            let mut transition = self.0.transition(opid);
            // We need to filter out already invalidated inputs
            let inputs = transition
                .destroyed
                .keys()
                .copied()
                .collect::<IndexSet<_>>();
            for addr in inputs {
                if !self.is_valid(addr.opid) {
                    // empty destroyed is allowed
                    let _ = transition.destroyed.remove(&addr);
                }
            }
            self.0.update_state(|state, articles| {
                state.rollback(transition, articles.semantics());
            })?;
            self.0.mark_invalid(opid);
        }
        self.commit_transaction();
        Ok(())
    }

    pub fn forward(&mut self, opids: impl IntoIterator<Item = Opid>) -> Result<(), MultiError<AcceptError, S::Error>> {
        for opid in self.descendants(opids) {
            debug_assert!(!self.is_valid(opid));
            if self
                .ancestors([opid])
                .filter(|id| *id != opid)
                .all(|id| self.is_valid(id))
            {
                let op = self.0.operation(opid);
                self.apply_verify(op, true)?;
                debug_assert!(self.is_valid(opid));
            }
        }
        self.commit_transaction();
        Ok(())
    }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_, S> {
        let builder = OpBuilder::new(self.contract_id(), self.0.articles().call_id(method));
        DeedBuilder { builder, ledger: self }
    }

    pub fn call(&mut self, params: CallParams) -> Result<Opid, MultiError<AcceptError, S::Error>> {
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
        for (addr, satisfaction) in params.using {
            if let Some(satisfaction) = satisfaction {
                builder = builder.satisfying(addr, satisfaction.name, satisfaction.witness);
            } else {
                builder = builder.using(addr);
            }
        }

        builder.commit()
    }

    /// Adds operation which was already checked to the stock. This does the following:
    /// - includes raw operation to stash;
    /// - computes state modification and applies it to the state;
    /// - saves removed state as a [`Transition`] and adds it to the execution trace.
    ///
    /// # Returns
    ///
    /// Whether the operation was already successfully included (`true`), or was already present in
    /// the stash.
    ///
    /// # Nota bene
    ///
    /// It is required to call [`Self::commit_transaction`] after all calls to this method.
    pub fn apply_verify(
        &mut self,
        operation: Operation,
        force: bool,
    ) -> Result<bool, MultiError<AcceptError, S::Error>> {
        if operation.contract_id != self.contract_id() {
            return Err(MultiError::A(AcceptError::Articles(SemanticError::ContractMismatch)));
        }

        let opid = operation.opid();

        let present = self.0.is_valid(opid);
        let articles = self.0.articles();
        if !present || force {
            let verified = articles
                .codex()
                .verify(self.contract_id(), operation, &self.0.state().raw, articles)
                .map_err(AcceptError::from)
                .map_err(MultiError::A)?;
            self.apply_internal(opid, verified, present && !force)
                .map_err(MultiError::B)?;
        }

        Ok(present)
    }

    /// Adds operation which was already checked to the stock. This does the following:
    /// - includes raw operation to stash;
    /// - computes state modification and applies it to the state;
    /// - saves removed state as a [`Transition`] and adds it to the execution trace.
    ///
    /// # Returns
    ///
    /// State invalidated by the operation in the form of a [`Transition`].
    ///
    /// # Nota bene
    ///
    /// It is required to call [`Self::commit_transaction`] after all calls to this method.
    pub fn apply(&mut self, operation: VerifiedOperation) -> Result<Transition, S::Error> {
        let opid = operation.opid();
        let present = self.0.is_valid(opid);
        self.apply_internal(opid, operation, present)
    }

    fn apply_internal(
        &mut self,
        opid: Opid,
        operation: VerifiedOperation,
        present: bool,
    ) -> Result<Transition, S::Error> {
        if !present {
            self.0.add_operation(opid, operation.as_operation());
        }

        let op = operation.as_operation();
        for read in &op.immutable_in {
            self.0.add_reading(*read, opid);
        }
        for prevout in &op.destructible_in {
            self.0.add_spending(prevout.addr, opid);
        }

        let transition = self
            .0
            .update_state(|state, articles| state.apply(operation, articles.semantics()))?;

        self.0.add_transition(opid, &transition);
        self.0.mark_valid(opid);
        Ok(transition)
    }

    pub fn commit_transaction(&mut self) { self.0.commit_transaction(); }
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum AcceptError {
    #[from]
    Io(io::Error),

    #[from]
    Articles(SemanticError),

    #[from]
    Verify(CallError),

    #[from]
    Decode(DecodeError),

    #[from]
    Serialize(SerializeError),

    Persistence(String),

    #[cfg(feature = "binfile")]
    #[display("Invalid file format")]
    InvalidFileFormat,
}

#[cfg(feature = "binfile")]
mod _fs {
    use std::path::Path;

    use binfile::BinFile;
    use strict_encoding::{StreamReader, StreamWriter};

    use super::*;

    pub const DEEDS_MAGIC_NUMBER: u64 = u64::from_be_bytes(*b"DEEDLDGR");
    pub const DEEDS_VERSION: u16 = 0;

    impl<S: Stock> Ledger<S> {
        pub fn export_to_file(
            &mut self,
            terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
            output: impl AsRef<Path>,
        ) -> io::Result<()> {
            let file = BinFile::<DEEDS_MAGIC_NUMBER, DEEDS_VERSION>::create_new(output)?;
            let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
            self.export(terminals, writer)
        }

        pub fn accept_from_file<E>(
            &mut self,
            input: impl AsRef<Path>,
            sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
        ) -> Result<(), MultiError<AcceptError, S::Error>> {
            let file = BinFile::<DEEDS_MAGIC_NUMBER, DEEDS_VERSION>::open(input)
                .map_err(|_| AcceptError::InvalidFileFormat)
                .map_err(MultiError::from_a)?;
            let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
            self.accept(&mut reader, sig_validator)
        }
    }
}
#[cfg(feature = "binfile")]
pub use _fs::*;
