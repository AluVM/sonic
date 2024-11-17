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

use aluvm::LibSite;

use super::{ApiVm, StateArithm, VmType};
use crate::state::StructData;

impl ApiVm for aluvm::Vm {
    const TYPE: VmType = VmType::AluVM;
    type Arithm = AluVMArithm;
    type ReaderSite = LibSite;
    type AdaptorSite = LibSite;
}

pub struct AluVMArithm {
    pub vm: Option<aluvm::Vm>,
    pub accumulate: LibSite,
    pub lessen: LibSite,
    pub diff: LibSite,
}

impl StateArithm for AluVMArithm {
    type Site = LibSite;

    fn measure(&self, state: StructData) -> Option<u8> { todo!() }

    fn accumulate(&mut self, state: StructData) -> Option<()> { todo!() }

    fn lessen(&mut self, state: StructData) -> Option<()> { todo!() }

    fn diff(&self) -> Option<StructData> { todo!() }
}
