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

use core::error::Error as StdError;

use sonicapi::{MergeError, Schema};
use strict_encoding::SerializeError;
use ultrasonic::{CellAddr, Operation, Opid};

use crate::{Articles, EffectiveState, Transition};

/// Persistence API for keeping and accessing the contract data.
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
// TODO: Consider returning large objects by reference
pub trait Stock {
    /// Provides contract [`Articles`].
    ///
    /// # I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    fn articles(&self) -> &Articles;

    /// Provides contract [`EffectiveState`].
    ///
    /// # I/O
    ///
    /// This call MUST NOT perform any I/O operations and MUST BE a non-blocking.
    fn state(&self) -> &EffectiveState;

    /// Detects whether an operation with a given `opid` is known to the contract.
    ///
    /// # Nota bene
    ///
    /// Positive response doesn't indicate that the operation participates in the current contract
    /// state or in a current valid contract history, which may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Contract::rollback`]),
    /// as well as re-included later with forwards (see [`Contract::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    fn has_operation(&self, opid: Opid) -> bool;

    /// Returns an operation ([`Operation`]) with a given `opid` from the set of known contract
    /// operations ("stash").
    ///
    /// # Nota bene
    ///
    /// If the method returns an operation, this doesn't indicate that the operation participates in
    /// the current contract state or in a current valid contract history, which/ may be exported.
    ///
    /// Operations may be excluded from the history due to rollbacks (see [`Contract::rollback`]),
    /// as well as re-included later with forwards (see [`Contract::forward`]). In both cases
    /// they are kept in the contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// If an `opid` is not present in the contract stash.
    ///
    /// In order to avoid panics always call the method after calling `has_operation`.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST panic if there is no operation
    /// matching the provided `opid`.
    fn operation(&self, opid: Opid) -> Operation;

    /// Returns an iterator over all operations known to the contract (i.e. the complete contract
    /// stash).
    ///
    /// # Nota bene
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
    /// # I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST iterate over all operations
    /// which were ever provided via [`Self::add_operation`].
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
    /// In order to avoid panics always call the method after calling `has_operation`.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST panic if there is no operation
    /// matching the provided `opid`.
    fn transition(&self, opid: Opid) -> Transition;

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
    /// [`Contract::rollback`]), as well as re-included later with forwards (see
    /// [`Contract::forward`]). In both cases corresponding state transitions are kept in the
    /// contract storage ("stash") and remain accessible to this method.
    ///
    /// # Panics
    ///
    /// The method MUST NOT panic
    ///
    /// # I/O
    ///
    /// The iterator provided in return may be a blocking iterator.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST iterate over all state
    /// transitions which were ever provided via [`Self::add_transition`].
    fn trace(&self) -> impl Iterator<Item = (Opid, Transition)>;

    /// Returns an id of an operation spending a provided address (operation destructible state
    /// output).
    ///
    /// # Nota bene
    ///
    /// This method is internally used in rollback procedure, and must not be accessed from outside.
    ///
    /// # I/O
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
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always persist an
    /// updated state after calling the callback `f` method.
    fn update_articles(
        &mut self,
        f: impl FnOnce(&mut Articles) -> Result<(), MergeError>,
    ) -> Result<(), StockError<MergeError>>;

    /// Updates contract effective state inside a callback method.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST guarantee to always persist an
    /// updated state after calling the callback `f` method.
    fn update_state<R>(&mut self, f: impl FnOnce(&mut EffectiveState, &Schema) -> R) -> Result<R, SerializeError>;

    /// Adds operation to the contract data.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - immediately store the operation data;
    /// - panic, if the operation with the same `opid` is already known, but differs from the
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
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - immediately store the transition data;
    /// - panic, if a transition for the same `opid` is already known, but differs from the provided
    ///   transition.
    ///
    /// They SHOULD:
    /// - perform a no-operation if the provided transition for the same `opid` is already known and
    ///   the `transition` itself matches the known data for it.
    fn add_transition(&mut self, opid: Opid, transition: &Transition);

    /// Registers given operation output (`spent`) to be spent (used as an input) in operation
    /// `spender`.
    ///
    /// # I/O
    ///
    /// This call MAY BE blocking.
    ///
    /// # Implementation instructions
    ///
    /// Specific persistence providers implementing this method MUST:
    /// - immediately store the new spending information;
    /// - silently update `spender` if the provided `spent` cell address were previously spent by a
    ///   different operation.
    fn add_spending(&mut self, spent: CellAddr, spender: Opid);
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum StockError<E: StdError> {
    Inner(E),

    #[from]
    Serialize(SerializeError),
}
