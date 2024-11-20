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

use amplify::confinement::SmallBlob;
use strict_types::SemId;

use super::state::StructData;
use super::{ApiVm, StateArithm, StateName, VmType};
use crate::LIB_NAME_SONARE;

#[derive(Clone, Debug)]
pub struct EmbeddedProc;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONARE, tags = repr, try_from_u8, into_u8)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[repr(u8)]
pub enum Source {
    #[strict_type(dumb)]
    FieldElements = 1,
    AssociatedData = 2,
}

impl ApiVm for EmbeddedProc {
    type Arithm = EmbeddedArithm;
    type ReaderSite = EmbeddedReaders;
    type AdaptorSite = EmbeddedAdaptors;

    fn vm_type(&self) -> VmType { VmType::Embedded }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONARE, tags = custom, dumb = Self::Const(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum EmbeddedReaders {
    #[strict_type(tag = 0)]
    Const(SmallBlob),

    #[strict_type(tag = 1)]
    Count(StateName),

    #[strict_type(tag = 2)]
    Sum(StateName),

    /// Count values which strict serialization is prefixed with a strict serialized argument
    #[strict_type(tag = 0x10)]
    CountPrefixed(StateName, SemId),
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONARE, tags = custom, dumb = Self::BytesFrom(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum EmbeddedAdaptors {
    #[strict_type(tag = 1)]
    BytesFrom(Source),

    #[strict_type(tag = 0x20)]
    Map { key: Source, val: Source },
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONARE, tags = repr, try_from_u8, into_u8)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[repr(u8)]
pub enum EmbeddedArithm {
    #[strict_type(dumb)]
    NonFungible = 0,
    Fungible = 1,
}

impl StateArithm for EmbeddedArithm {
    fn measure(&self, state: StructData) -> Option<u8> { todo!() }

    fn accumulate(&mut self, state: StructData) -> Option<()> { todo!() }

    fn lessen(&mut self, state: StructData) -> Option<()> { todo!() }

    fn diff(&self) -> Option<StructData> { todo!() }
}
