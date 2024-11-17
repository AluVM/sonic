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

//! In-memory contract state supports contracts with up to:
//! - 256 state types;
//! - 2^16 global state (append-only state) values;
//! - 2^16 owned state (destructible state) known values ("UTXOs");
//! - 256 interfaces per contract.
//!
//! If your use case requires supporting larger-scale contracts please consider using other
//! persistence solution adapted for enterprise needs.

use amplify::confinement::{SmallOrdMap, SmallVec, TinyOrdMap};
use sonare::api::ApiId;
use sonare::state::{StateTy, StructData};
use ultrasonic::{CellAddr, StateCell, StateData};

/// The state as it is defined in the contract. Accessed during the validation.
pub struct RawState {
    pub append_only: SmallOrdMap<CellAddr, StateData>,
    pub destructible: SmallOrdMap<CellAddr, StateCell>,
}

/// State converted with API adaptors.
pub struct ConvertedState {
    pub append_only: SmallOrdMap<CellAddr, StructData>,
    pub destructible: SmallOrdMap<CellAddr, StructData>,
}

/// Index for retrieving state by type.
pub struct StateIndex {
    pub append_only: TinyOrdMap<StateTy, SmallVec<CellAddr>>,
    pub destructible: TinyOrdMap<StateTy, SmallVec<CellAddr>>,
}

pub struct MemState {
    // /// Logger object which  is used to report errors.
    // pub logger: Logger,
    /// Raw state used in validation of new operations.
    pub raw: RawState,

    /// State data converted using specific APIs.
    ///
    /// When more API adaptors are added, these values are either lazy computed - or computed in a
    /// background task.
    pub converted: TinyOrdMap<ApiId, ConvertedState>,

    /// Index for resolving state types into values.
    pub index: StateIndex,
}

impl MemState {}
