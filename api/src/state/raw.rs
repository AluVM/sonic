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

use aluvm::LibSite;
use amplify::confinement::{SmallBlob, U24 as U24MAX};
use strict_encoding::StreamReader;
use strict_types::{SemId, StrictVal, TypeSystem};
use ultrasonic::RawData;

use crate::{StateBuildError, StateConvertError, LIB_NAME_SONIC};

pub const TOTAL_RAW_BYTES: usize = U24MAX;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::StrictDecode(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum RawConvertor {
    /// Convert raw bytes using strict encoding.
    #[strict_type(tag = 0x00)]
    StrictDecode(SemId),
    // In the future we can add more adaptors:
    // - using just a specific range of raw bytes, not a full value - such that multiple APIs may read different parts
    //   of the same data;
    /// Execute a custom function.
    // AluVM is reserved for the future. We need it here to avoid breaking changes.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl RawConvertor {
    pub fn convert(&self, raw: &RawData, sys: &TypeSystem) -> Result<StrictVal, StateConvertError> {
        match self {
            Self::StrictDecode(sem_id) => strict_convert(*sem_id, raw, sys),
            Self::AluVM(_) => Err(StateConvertError::Unsupported),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::StrictEncode(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum RawBuilder {
    /// Convert strict value into raw bytes using strict encoding.
    #[strict_type(tag = 0x00)]
    StrictEncode(SemId),

    /// Execute a custom function.
    // AluVM is reserved for the future. We need it here to avoid breaking changes.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl RawBuilder {
    #[allow(clippy::result_large_err)]
    pub fn build(&self, val: StrictVal, sys: &TypeSystem) -> Result<RawData, StateBuildError> {
        match self {
            Self::StrictEncode(sem_id) => strict_build(*sem_id, val, sys),
            Self::AluVM(_) => Err(StateBuildError::Unsupported),
        }
    }
}

fn strict_convert(sem_id: SemId, raw: &RawData, sys: &TypeSystem) -> Result<StrictVal, StateConvertError> {
    let mut reader = StreamReader::cursor::<TOTAL_RAW_BYTES>(&raw[..]);
    let mut val = sys.strict_read_type(sem_id, &mut reader)?.unbox();

    if reader.into_cursor().position() != raw[..].len() as u64 {
        return Err(StateConvertError::NotEntirelyConsumed);
    }

    loop {
        if let StrictVal::Tuple(ref mut vec) = val {
            if vec.len() == 1 {
                val = vec.remove(0);
                continue;
            }
        }
        break;
    }

    Ok(val)
}

#[allow(clippy::result_large_err)]
fn strict_build(sem_id: SemId, val: StrictVal, sys: &TypeSystem) -> Result<RawData, StateBuildError> {
    let mut data = SmallBlob::new();

    let typed_val = sys.typify(val, sem_id)?;
    sys.strict_write_value(&typed_val, &mut data)?;

    Ok(RawData::from(data))
}
