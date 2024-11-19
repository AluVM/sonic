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

use super::alu::AluVMArithm;
use super::embedded::{EmbeddedAdaptors, EmbeddedArithm, EmbeddedProc, EmbeddedReaders};
use super::{ApiVm, StateArithm, StructData, VmType};

pub enum UniVm {
    Embedded(EmbeddedProc),
    AluVM(aluvm::Vm),
}

impl ApiVm for UniVm {
    type Arithm = UniArithm;
    type ReaderSite = UniReader;
    type AdaptorSite = UniAdaptor;

    fn vm_type(&self) -> VmType {
        match self {
            UniVm::Embedded(vm) => vm.vm_type(),
            UniVm::AluVM(vm) => vm.vm_type(),
        }
    }
}

pub enum UniArithm {
    Embedded(EmbeddedArithm),
    AluVM(AluVMArithm),
}

impl StateArithm for UniArithm {
    fn measure(&self, state: StructData) -> Option<u8> {
        match self {
            UniArithm::Embedded(a) => a.measure(state),
            UniArithm::AluVM(a) => a.measure(state),
        }
    }

    fn accumulate(&mut self, state: StructData) -> Option<()> {
        match self {
            UniArithm::Embedded(a) => a.accumulate(state),
            UniArithm::AluVM(a) => a.accumulate(state),
        }
    }

    fn lessen(&mut self, state: StructData) -> Option<()> {
        match self {
            UniArithm::Embedded(a) => a.lessen(state),
            UniArithm::AluVM(a) => a.lessen(state),
        }
    }

    fn diff(&self) -> Option<StructData> {
        match self {
            UniArithm::Embedded(a) => a.diff(),
            UniArithm::AluVM(a) => a.diff(),
        }
    }
}

pub enum UniReader {
    Embedded(EmbeddedReaders),
    AluVM(LibSite),
}

pub enum UniAdaptor {
    Embedded(EmbeddedAdaptors),
    AluVM(LibSite),
}
