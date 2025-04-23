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

use alloc::collections::{BTreeSet, VecDeque};
use core::borrow::Borrow;
use std::io;

use amplify::hex::ToHex;
use sonic_callreq::MethodName;
use sonicapi::{MergeError, NamedState, OpBuilder, Schema};
use strict_encoding::{
    DecodeError, ReadRaw, SerializeError, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw,
};
use ultrasonic::{AuthToken, CallError, CellAddr, ContractId, Operation, Opid, VerifiedOperation};

use crate::deed::{CallParams, DeedBuilder};
use crate::{Articles, EffectiveState, IssueError, LoadError, Stock, StockError, Transition};

pub const LEDGER_MAGIC_NUMBER: [u8; 8] = *b"DEEDLDGR";
pub const LEDGER_VERSION: [u8; 2] = [0x00, 0x01];

/// Contract with all its state and operations, supporting updates and rollbacks.
// We need this structure to hide internal persistence methods and not to expose them.
// We need the persistence trait (`Stock`) in order to allow different persistence storage
// implementations.
pub struct Ledger<S: Stock>(pub(crate) S);

impl<S: Stock> Ledger<S> {
    /// Issues a new contract from the provided articles, creating its persistence using given
    /// implementation-specific configuration.
    ///
    /// # Panics
    ///
    /// This call must not panic, and instead must return an error.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY perform any I/O operations.
    pub fn issue(articles: Articles, conf: S::Conf) -> Result<Self, IssueError<S::Error>> {
        S::issue(articles, conf).map(Self)
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
    pub fn load(conf: S::Conf) -> Result<Self, LoadError<S::Error>> { S::load(conf).map(Self) }

    pub fn config(&self) -> S::Conf { self.0.config() }

    /// Provides [`Schema`] object, which includes codex, under which the contract was issued, and
    /// interfaces for the contract under that codex.
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    #[inline]
    pub fn schema(&self) -> &Schema { &self.0.articles().schema }

    /// Provides contract id.
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    // TODO: Cache the id
    #[inline]
    pub fn contract_id(&self) -> ContractId { self.0.articles().contract_id() }

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

    /// Detects whether an operation with a given `opid` is known to the contract.
    ///
    /// # Nota bene
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
    /// If the method returns an operation, this doesn't indicate that the operation participates in
    /// the current contract state or in a current valid contract history, which/ may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Ledger::rollback`]),
    /// as well as re-included later with forwards (see [`Ledger::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// If an `opid` is not present in the contract stash.
    ///
    /// In order to avoid panics always call the method after calling `has_operation`.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    #[inline]
    pub fn operation(&self, opid: Opid) -> Operation { self.0.operation(opid) }

    /// Returns an iterator over all operations known to the contract (i.e. the complete contract
    /// stash).
    ///
    /// # Nota bene
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

    /// Returns an iterator over all state transitions known to the contract (i.e. the complete
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

    pub fn export_all(&self, mut writer: StrictWriter<impl WriteRaw>) -> io::Result<()> {
        // Write articles
        writer = self.0.articles().strict_encode(writer)?;
        // Stream operations
        for (_, op) in self.0.operations() {
            writer = op.strict_encode(writer)?;
        }
        Ok(())
    }

    pub fn export(
        &self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        mut writer: StrictWriter<impl WriteRaw>,
    ) -> io::Result<()> {
        // This is compatible with BinFile
        writer = LEDGER_MAGIC_NUMBER.strict_encode(writer)?;
        // Version
        writer = LEDGER_VERSION.strict_encode(writer)?;
        writer = self.contract_id().strict_encode(writer)?;
        self.export_aux(terminals, writer, |_, w| Ok(w))
    }

    // TODO: Return statistics
    pub fn export_aux<W: WriteRaw>(
        &self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        mut writer: StrictWriter<W>,
        mut aux: impl FnMut(Opid, StrictWriter<W>) -> io::Result<StrictWriter<W>>,
    ) -> io::Result<()> {
        let mut queue = terminals
            .into_iter()
            .map(|terminal| self.0.state().addr(*terminal.borrow()).opid)
            .collect::<BTreeSet<_>>();
        let genesis_opid = self.0.articles().issue.genesis_opid();
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

        // TODO: Include all operations defining published state

        // Write articles
        writer = self.0.articles().strict_encode(writer)?;
        writer = aux(genesis_opid, writer)?;
        // Stream operations
        for (opid, op) in self.0.operations() {
            if !opids.remove(&opid) {
                continue;
            }
            writer = op.strict_encode(writer)?;
            writer = aux(opid, writer)?;
        }

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

    pub fn merge_articles(&mut self, new_articles: Articles) -> Result<(), StockError<MergeError>> {
        self.0.update_articles(|articles| {
            articles.merge(new_articles)?;
            Ok(())
        })
    }

    pub fn accept(&mut self, reader: &mut StrictReader<impl ReadRaw>) -> Result<(), AcceptError> {
        let magic_bytes = <[u8; 8]>::strict_decode(reader)?;
        if magic_bytes != LEDGER_MAGIC_NUMBER {
            return Err(DecodeError::DataIntegrityError(format!(
                "wrong contract issuer schema magic bytes {}",
                magic_bytes.to_hex()
            ))
            .into());
        }
        let version = <[u8; 2]>::strict_decode(reader)?;
        if version != LEDGER_VERSION {
            return Err(DecodeError::DataIntegrityError(format!(
                "unsupported contract issuer schema version {}",
                u16::from_be_bytes(version)
            ))
            .into());
        }
        let contract_id = ContractId::strict_decode(reader)?;

        let articles = Articles::strict_decode(reader)?;
        if articles.contract_id() != contract_id {
            return Err(AcceptError::Articles(MergeError::ContractMismatch));
        }

        self.merge_articles(articles).map_err(|e| match e {
            StockError::Inner(e) => AcceptError::Articles(e),
            StockError::Serialize(e) => AcceptError::Io(e),
        })?;

        loop {
            let op = match Operation::strict_decode(reader) {
                Ok(operation) => operation,
                Err(DecodeError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };
            self.apply_verify(op)?;
        }
        self.commit_transaction();
        Ok(())
    }

    pub fn rollback(&mut self, opids: impl IntoIterator<Item = Opid>) -> Result<(), SerializeError> {
        let mut chain = opids.into_iter().collect::<VecDeque<_>>();
        // Get all subsequent operations
        loop {
            let mut count = 0usize;
            for mut index in 0..chain.len() {
                let opid = chain[index];
                let op = self.0.operation(opid);
                for no in 0..op.destructible.len_u16() {
                    let addr = CellAddr::new(opid, no);
                    let Some(spent) = self.0.spent_by(addr) else { continue };
                    chain.push_front(spent);
                    count += 1;
                    index += 1;
                }
            }
            if count == 0 {
                break;
            }
        }

        for opid in chain {
            let transition = self.0.transition(opid);
            self.0.update_state(|state, schema| {
                state.rollback(transition, &schema.default_api, schema.custom_apis.keys(), &schema.types);
            })?;
        }
        Ok(())
    }

    pub fn forward(&mut self, opids: impl IntoIterator<Item = Opid>) -> Result<(), AcceptError> {
        let mut all = opids.into_iter().collect::<VecDeque<_>>();
        let mut queue = VecDeque::with_capacity(all.len());

        while let Some(opid) = all.pop_front() {
            let op = self.0.operation(opid);
            queue.push_front(op);
            let op = &queue[0];
            for prev in &op.reading {
                if all.contains(&prev.opid) {
                    all.push_front(prev.opid);
                }
            }
            for prev in &op.destroying {
                if all.contains(&prev.addr.opid) {
                    all.push_front(prev.addr.opid);
                }
            }
        }
        for op in queue {
            self.apply_verify(op)?;
        }
        self.commit_transaction();
        Ok(())
    }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_, S> {
        let builder = OpBuilder::new(self.contract_id(), self.0.articles().schema.call_id(method));
        DeedBuilder { builder, ledger: self }
    }

    pub fn call(&mut self, params: CallParams) -> Result<Opid, AcceptError> {
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
    pub fn apply_verify(&mut self, operation: Operation) -> Result<bool, AcceptError> {
        if operation.contract_id != self.contract_id() {
            return Err(AcceptError::Articles(MergeError::ContractMismatch));
        }

        let opid = operation.opid();

        let present = self.0.has_operation(opid);
        let schema = &self.0.articles().schema;
        if !present {
            let verified = schema
                .codex
                .verify(self.contract_id(), operation, &self.0.state().raw, schema)?;
            self.apply_internal(opid, verified, present)?;
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
    /// State invalidated by the operation in form of a [`Transition`].
    ///
    /// # Nota bene
    ///
    /// It is required to call [`Self::commit_transaction`] after all calls to this method.
    pub fn apply(&mut self, operation: VerifiedOperation) -> Result<Transition, SerializeError> {
        let opid = operation.opid();
        let present = self.0.has_operation(opid);
        self.apply_internal(opid, operation, present)
    }

    fn apply_internal(
        &mut self,
        opid: Opid,
        operation: VerifiedOperation,
        present: bool,
    ) -> Result<Transition, SerializeError> {
        if !present {
            self.0.add_operation(opid, operation.as_operation());
        }

        let op = operation.as_operation();
        for prevout in &op.destroying {
            self.0.add_spending(prevout.addr, opid);
        }

        let transition = self.0.update_state(|state, schema| {
            state.apply(operation, &schema.default_api, schema.custom_apis.keys(), &schema.types)
        })?;

        self.0.add_transition(opid, &transition);
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
    Articles(MergeError),

    #[from]
    Verify(CallError),

    #[from]
    Decode(DecodeError),

    #[from]
    Serialize(SerializeError),
}
