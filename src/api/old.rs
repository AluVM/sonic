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

mod adaptor;
mod interface;
mod contract;

use aluvm::regs::Reg;
use aluvm::LibSite;
use amplify::confinement::{SmallOrdMap, TinyOrdMap, TinyOrdSet, TinyString, TinyVec};
use strict_encoding::Ident;
use strict_types::SemId;

pub type CallName = Ident;
pub type OwnedStateName = Ident;
pub type FreeStateName = Ident;
pub type ErrorName = Ident;
pub type InputName = Ident;
pub type IfaceName = Ident;

pub struct OpAbi {
    pub spent: TinyOrdSet<OwnedStateName>,
    pub input: TinyOrdMap<InputName, SemId>,
    // Unlike the free input above, assignments input is always a map from a single-use seal to the data type
    pub assignments: TinyOrdMap<InputName, SemId>,
}

pub struct ReaderAbi {
    pub script: Option<LibSite>,
    pub return_ty: SemId,
    pub return_reg: Reg,
}

pub struct IfaceDecl {
    pub name: IfaceName,
    pub inherited: TinyVec<IfaceDecl>,
    pub extension: Interface,
}

pub struct Interface {
    // Readers have access to all the contract state, including known unspent assignments and all
    // free state.
    pub readers: SmallOrdMap<CallName, ReaderAbi>,
    pub errors: SmallOrdMap<ErrorName, TinyString>,
    pub ops: SmallOrdMap<CallName, OpAbi>,
}

/// Adaptor converts strict type to a field element - and vice verse.
pub struct AdaptorCall {
    pub script: LibSite,
    pub strict_reg: Reg,
    pub fiel_array_start: Reg,
}

// Type adaptors must ship with the type library
pub struct TypeAdaptor {
    pub sem_id: SemId,
    pub strict_to_fiel: AdaptorCall,
    pub fiel_to_strict: AdaptorCall,
}

pub struct Implementation {
    pub interface: IfaceId,

    // this should be a bijection, meaning both keys and values must be used only once
    pub errors: SmallOrdMap<u16, ErrorName>,

    // Here we map to semantic ids, not to a field elements. We must
    pub free_state: TinyOrdMap<FreeStateName, SemId>,
    pub owned_state: TinyOrdMap<OwnedStateName, SemId>,
    pub ops: SmallOrdMap<CallName, OpAbi>,
}
