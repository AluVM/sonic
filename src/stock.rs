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

use core::error::Error;

use amplify::MultiError;
use sonicapi::SemanticError;
use ultrasonic::{CallError, CellAddr, ContractName, Operation, Opid};

use crate::{Articles, EffectiveState, Transition};

/// Stock is a persistence API for keeping and accessing contract data.
///
/// Contract data include:
/// - contract [`Articles`];
/// - contract [`EffectiveState`], dynamically computed;
/// - all known contract [`Operations`] ("stash"), including the ones which may not be included into
///   a state or be a part of a contract history;
/// - a trace of the most recent execution of each of the [`Operations`] in the stash ("trace");
/// - an information which operations reference (use as input, "spend") other operation outputs.
///
/// Trace and spending information is used in contract rollback and forward operations, which lead
/// to a re-computation of a contract state (but leave stash and trace data unaffected).
pub trait Stock {
    /// Persistence configuration type.
    type Conf;
    /// Error type for persistence errors.
    type Error: Error;

    /// Creates a new contract from the provided articles, creating its persistence using a given
    /// implementation-specific configuration.
    ///
    /// # Panics
    ///
    /// This call must not panic, and instead must return an error.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY perform any I/O operations.
    fn new(articles: Articles, state: EffectiveState, conf: Self::Conf) -> Result<Self, Self::Error>
    where Self: Sized;

    /// Loads a contract from persistence using the provided configuration.
    ///
    /// # Panics
    ///
    /// This call must not panic, and instead must return an error.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY perform any I/O operations.
    fn load(conf: Self::Conf) -> Result<Self, Self::Error>
    where Self: Sized;

    /// Returns a copy of the config object used during the stock construction.
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations.
    fn config(&self) -> Self::Conf;

    /// Provides contract [`Articles`].
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    fn articles(&self) -> &Articles;

    /// Provides contract [`EffectiveState`].
    ///
    /// # Blocking I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    fn state(&self) -> &EffectiveState;

    /// Detects whether an operation with a given `opid` participates in the current state.
    fn is_valid(&self, opid: Opid) -> bool;

    fn mark_valid(&mut self, opid: Opid);
    fn mark_invalid(&mut self, opid: Opid);

    /// Detects whether an operation with a given `opid` is known to the contract.
    ///
    /// # Nota bene
    ///
    /// Does not include genesis operation id.
    ///
    /// Positive response does not indicate that the operation participates in the current contract
    /// state or in a current valid contract history, which may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Contract::rollback`]),
    /// as well as re-included later with forwards (see [`Contract::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    fn has_operation(&self, opid: Opid) -> bool;

    /// Returns an operation ([`Operation`]) with a given `opid` from the set of known contract
    /// operations ("stash").
    ///
    /// # Nota bene
    ///
    /// Does not include genesis operation.
    ///
    /// If the method returns an operation, this does not indicate that the operation participates
    /// in the current contract state or in a current valid contract history, which/ may be
    /// exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Contract::rollback`]),
    /// as well as re-included later with forwards (see [`Contract::forward`]). In both cases
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
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST panic if there is no operation
    /// matching the provided `opid`.
    fn operation(&self, opid: Opid) -> Operation;

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
    /// Operations may be excluded from the history due to rollbacks (see [`Contract::rollback`]),
    /// as well as re-included later with forwards (see [`Contract::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// The method MUST NOT panic
    ///
    /// # Blocking I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST iterate over all operations
    /// ever provided via [`Self::add_operation`].
    fn operations(&self) -> impl Iterator<Item = (Opid, Operation)>;

    /// Returns a state transition ([`Transition`]) with a given `opid` from the set of known
    /// contract state transition ("trace").
    ///
    /// # Nota bene
    ///
    /// If the method returns a state transition, this doesn't indicate that the corresponding
    /// operation participates in the current contract state or in a current valid contract
    /// history, which may be exported.
    ///
    /// State transitions may be excluded from the history due to rollbacks (see
    /// [`Contract::rollback`]), as well as re-included later with forwards (see
    /// [`Contract::forward`]). In both cases corresponding state transitions are kept in the
    /// contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// If an `opid` is not present in the contract trace.
    ///
    /// To avoid panics, always call the method after calling `has_operation`.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST panic if there is no operation
    /// matching the provided `opid`.
    fn transition(&self, opid: Opid) -> Transition;

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
    /// [`Contract::rollback`]), as well as re-included later with forwards (see
    /// [`Contract::forward`]). In both cases corresponding state transitions are kept in the
    /// contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// The method MUST NOT panic
    ///
    /// # Blocking I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST iterate over all state
    /// transitions that were ever provided via [`Self::add_transition`].
    fn trace(&self) -> impl Iterator<Item = (Opid, Transition)>;

    /// Returns an id of an operation reading a provided address (operation global state
    /// output).
    ///
    /// # Nota bene
    ///
    /// This method is internally used in computing operation descendants and must not be accessed
    /// from outside.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always return a
    /// non-empty iterator for all `addr` which were at least once provided via
    /// [`Self::add_reading`] as an `addr` argument.
    fn read_by(&self, addr: CellAddr) -> impl Iterator<Item = Opid>;

    /// Returns an id of an operation spending a provided address (operation owned state output).
    ///
    /// # Nota bene
    ///
    /// This method is internally used in computing operation descendants and must not be accessed
    /// from outside.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always return a
    /// non-`None` for all `addr` which were at least once provided via [`Self::add_spending`]
    /// as a `spent` argument.
    fn spent_by(&self, addr: CellAddr) -> Option<Opid>;

    /// Updates articles with a newer version inside a callback method.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always persist an
    /// updated state after calling the callback `f` method.
    fn update_articles(
        &mut self,
        f: impl FnOnce(&mut Articles) -> Result<bool, SemanticError>,
    ) -> Result<bool, MultiError<SemanticError, Self::Error>>;

    /// Updates contract effective state inside a callback method.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always persist an
    /// updated state after calling the callback `f` method.
    fn update_state<R>(&mut self, f: impl FnOnce(&mut EffectiveState, &Articles) -> R) -> Result<R, Self::Error>;

    /// Adds operation to the contract data.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - immediately store the operation data;
    /// - panic, if the operation with the same `opid` is already known but differs from the
    ///   provided operation.
    ///
    /// They SHOULD:
    /// - perform a no-operation if the provided operation with the same `opid` is already known and
    ///   the `operation` itself matches the known data for it;
    /// - NOT verify that the `operation` is matching the provided `opid` since this MUST BE
    ///   guaranteed by a caller.
    fn add_operation(&mut self, opid: Opid, operation: &Operation);

    /// Adds state transition caused by an operation with `opid` to the contract data.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - immediately store the transition data;
    /// - panic, if a transition for the same `opid` is already known but differs from the provided
    ///   transition.
    ///
    /// They SHOULD:
    /// - perform a no-operation if the provided transition for the same `opid` is already known and
    ///   the `transition` itself matches the known data for it.
    fn add_transition(&mut self, opid: Opid, transition: &Transition);

    /// Registers a given operation global output (`addr`) to be read (used as an input) in
    /// operation `reader`.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - add the `reader` to the list of readers who had accessed the address.
    fn add_reading(&mut self, addr: CellAddr, reader: Opid);

    /// Registers a given operation owned output (`spent`) to be spent (used as an input) in
    /// operation `spender`.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - silently update `spender` if the provided `spent` cell address was previously spent by a
    ///   different operation.
    fn add_spending(&mut self, spent: CellAddr, spender: Opid);

    /// Commits newly added spending info.
    ///
    /// # Blocking I/O
    ///
    /// This call MAY BE blocking.
    fn commit_transaction(&mut self);
}

#[derive(Clone, PartialEq, Eq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum IssueError {
    /// unable to issue a new contract '{0}' due to invalid genesis data. Specifically, {1}
    Genesis(ContractName, CallError),
}
