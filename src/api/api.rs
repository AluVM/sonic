// SONARE: Runtime environment for formally-verifiable distributed software
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

//! API defines how a contract can be interfaced by a software.
//!
//! SONARE provides four types of actions for working with contract (ROVT):
//! 1. _Read_ the state of the contract;
//! 2. _Operate_: construct new operations performing contract state transitions;
//! 3. _Verify_ an existing operation under the contract Codex and generate transaction;
//! 4. _Transact_: apply or roll-back transactions to the contract state.
//!
//! API defines methods for human-based interaction with the contract for read and operate actions.
//! The verify part is implemented in the consensus layer (UltraSONIC), the transact part is
//! performed directly, so these two are not covered by an API.

use amplify::confinement::{TinyOrdMap, TinyString};
use amplify::Bytes32;
use strict_encoding::VariantName;
use strict_types::SemId;
use ultrasonic::{CallId, ContractId, Ffv};

use super::ApiVmType;
use crate::state::StructData;

pub type StateName = VariantName;
pub type MethodName = VariantName;

pub type ApiId = Bytes32;

/// API is an interface implementation.
///
/// API should works without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data even if they are duplicated from the interfaces. Basically one may think of
/// API as a compiled interface hierarchy applied to a specific contract.
///
/// API doesn't commit to an interface ID, since it can match multiple interfaces in the interface
/// hierarchy.
pub struct Api<Vm: ApiVm> {
    pub version: Ffv,
    pub contract_id: ContractId,

    // TODO: Add developer etc.
    /// Virtual machine used by `state` and `readers`.
    ///
    /// NB: `verifiers` always use VM type defined by the contract itself (currently zk-AluVM).
    pub vm: ApiVmType,

    /// State API defines how specific state types (both append-only and destructible) are
    /// constructed out of (and converted into) UltraSONIC memory cells.
    pub state: TinyOrdMap<StateName, StateApi<Vm>>,

    /// Readers have access to the converted `state` and can construct a derived state out of it.
    ///
    /// The typical example when readers are used is to sum individual asset issues and compute the
    /// number of totally issued assets.
    pub readers: TinyOrdMap<MethodName, Vm::ReaderSite>,

    /// Links between named transaction methods defined in the interface - and corresponding
    /// verifier call ids defined by the contract.
    ///
    /// NB: Multiple methods from the interface may call to the came verifier.
    pub verifiers: TinyOrdMap<MethodName, CallId>,

    /// Maps error type reported by a contract verifier via `EA` value to an error description taken
    /// from the interfaces.
    pub errors: TinyOrdMap<u128, TinyString>,
}

pub struct StateApi<Vm: ApiVm> {
    pub sem_id: SemId,

    /// State arithmetics engine used in constructing new contract operations.
    pub arithmetics: Vm::Arithm,

    /// Procedures which convert structured type [`StructData`] into a state made of finite field
    /// elements [`StateData`].
    pub adaptor: Vm::AdaptorSite,

    /// Procedures which convert a state made of finite field elements [`StateData`] into a
    /// structured type [`StructData`].
    pub builder: Vm::AdaptorSite,
}

pub trait CallSite {}
impl<T> CallSite for T {}

pub trait ApiVm {
    const TYPE: ApiVmType;
    type Arithm: StateArithm;
    type ReaderSite: CallSite;
    type AdaptorSite: CallSite;
}

// TODO: Use Result's instead of Option
pub trait StateArithm {
    type Site: CallSite;

    /// Procedure which converts [`StructData`] corresponding to this type into a weight in range
    /// `0..256` representing how much this specific state fulfills certain state requirement.
    ///
    /// This is used in selecting state required to fulfill input for a provided contract
    /// [`Request`].
    fn measure(&self, state: StructData) -> Option<u8>;

    /// Procedure which is called on [`StateArithm`] to accumulate an input state.
    fn accumulate(&mut self, state: StructData) -> Option<()>;

    /// Procedure which is called on [`StateArithm`] to lessen an output state.
    fn lessen(&mut self, state: StructData) -> Option<()>;

    /// Procedure which is called on [`StateArithm`] to compute the difference between an input
    /// state and output state.
    fn diff(&self) -> Option<StructData>;
}
