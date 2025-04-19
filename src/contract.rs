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

use alloc::collections::{BTreeSet, VecDeque};
use core::borrow::Borrow;
use std::io;

use sonic_callreq::MethodName;
use sonicapi::{MergeError, NamedState, OpBuilder, Schema};
use strict_encoding::{
    DecodeError, ReadRaw, SerializeError, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw,
};
use ultrasonic::{AuthToken, CallError, CellAddr, ContractId, Operation, Opid, VerifiedOperation};

use crate::deed::{CallParams, DeedBuilder};
use crate::{Articles, EffectiveState, Stock, StockError, Transition};

/// Stock is a contract with all its state and operations, supporting updates and rollbacks.
// We need this structure to hide internal persistence methods and not to expose them.
// We need the persistence trait (`Stock`) in order to allow different persistence storage
// implementations.
pub struct Contract<S: Stock>(pub(crate) S);

impl<S: Stock> Contract<S> {
    pub fn schema(&self) -> &Schema { &self.0.articles().schema }
    pub fn contract_id(&self) -> ContractId { self.0.articles().contract_id() }
    pub fn state(&self) -> &EffectiveState { self.0.state() }

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
        writer: StrictWriter<impl WriteRaw>,
    ) -> io::Result<()> {
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

    pub fn import(&mut self, reader: &mut StrictReader<impl ReadRaw>) -> Result<(), AcceptError> {
        let articles = Articles::strict_decode(reader)?;
        self.merge_articles(articles).map_err(|e| match e {
            StockError::Inner(e) => AcceptError::Articles(e),
            StockError::Serialize(e) => AcceptError::Serialize(e),
        })?;

        loop {
            let op = match Operation::strict_decode(reader) {
                Ok(operation) => operation,
                Err(DecodeError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };
            self.apply_verify(op)?;
        }
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
        Ok(())
    }

    pub fn start_deed(&mut self, method: impl Into<MethodName>) -> DeedBuilder<'_, S> {
        let builder = OpBuilder::new(self.contract_id(), self.0.articles().schema.call_id(method));
        DeedBuilder { builder, contract: self }
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

    /// # Returns
    ///
    /// Whether operation was already successfully included (`true`), or was already present in the
    /// stash.
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
