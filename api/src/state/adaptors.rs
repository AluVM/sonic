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

use std::io;

use amplify::confinement::ConfinedBlob;
use amplify::num::u256;
use sonic_callreq::StateName;
use strict_encoding::{SerializeError, StreamReader};
use strict_types::{decode, typify, SemId, StrictVal, TypeSystem};
use ultrasonic::StateValue;

use crate::{StateTy, LIB_NAME_SONIC};

pub(super) const USED_FIEL_BYTES: usize = u256::BYTES as usize - 2;
pub(super) const TOTAL_BYTES: usize = USED_FIEL_BYTES * 3;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::TypedEncoder(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateConvertor {
    #[strict_type(tag = 0x00)]
    TypedEncoder(StateTy),
    // In the future we can add more adaptors:
    // - doing more compact encoding (storing type in bits, not a full field element);
    // - using just a specific range of field element bits, not a full value - such that multiple APIs may read
    //   different parts of the same data;
    // - using a Turing complete grammar with some VM (AluVM? RISC-V? WASM?).
}

impl StateConvertor {
    pub fn convert(
        &self,
        sem_id: SemId,
        value: StateValue,
        sys: &TypeSystem,
    ) -> Result<Option<StrictVal>, StateConvertError> {
        match self {
            Self::TypedEncoder(ty) => typed_convert(*ty, sem_id, value, sys),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::TypedEncoder(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateBuilder {
    #[strict_type(tag = 0x00)]
    TypedEncoder(StateTy),
    // In the future we can add more adaptors:
    // - doing more compact encoding (storing type in bits, not a full field element);
    // - using a Turing complete grammar with some VM (AluVM? RISC-V? WASM?).
}

impl StateBuilder {
    pub fn build(&self, sem_id: SemId, value: StrictVal, sys: &TypeSystem) -> Result<StateValue, StateBuildError> {
        let typed = sys.typify(value, sem_id)?;
        let ser = sys.strict_serialize_value::<TOTAL_BYTES>(&typed)?;
        Ok(match self {
            Self::TypedEncoder(ty) => typed_build(*ty, ser),
        })
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error, From)]
#[display(inner)]
pub enum StateBuildError {
    #[display("unknown state name '{0}'")]
    UnknownStateName(StateName),

    #[from]
    Typify(typify::Error),

    #[from(io::Error)]
    #[display("state data is too large to be encoded")]
    TooLarge,

    #[from]
    Serialize(SerializeError),
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error, From)]
pub enum StateConvertError {
    #[display("unknown state name '{0}'")]
    UnknownStateName(StateName),

    #[from]
    #[display(inner)]
    Decode(decode::Error),

    #[display("state value is not fully consumed")]
    NotEntirelyConsumed,

    #[display("state has not data")]
    UnitState,
}

fn typed_convert(
    ty: StateTy,
    sem_id: SemId,
    value: StateValue,
    sys: &TypeSystem,
) -> Result<Option<StrictVal>, StateConvertError> {
    let from_ty = value.get(0).ok_or(StateConvertError::UnitState)?.to_u256();
    // State type does not match
    if from_ty != ty {
        return Ok(None);
    }

    let mut buf = [0u8; TOTAL_BYTES];
    let mut i = 1u8;
    while let Some(el) = value.get(i) {
        let from = USED_FIEL_BYTES * (i - 1) as usize;
        let to = USED_FIEL_BYTES * i as usize;
        buf[from..to].copy_from_slice(&el.to_u256().to_le_bytes()[..USED_FIEL_BYTES]);
        i += 1;
    }
    debug_assert!(i <= 4);

    let mut cursor = StreamReader::cursor::<TOTAL_BYTES>(buf);
    // We do not check here that we have reached the end of the buffer, since it may be filled with
    // zeros up to the field element length.
    let mut val = sys.strict_read_type(sem_id, &mut cursor)?.unbox();

    loop {
        if let StrictVal::Tuple(ref mut vec) = val {
            if vec.len() == 1 {
                val = vec.remove(0);
                continue;
            }
        }
        break;
    }

    Ok(Some(val))
}

fn typed_build(ty: StateTy, ser: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue {
    let mut elems = Vec::with_capacity(4);
    elems.push(ty);
    for chunk in ser.chunks(USED_FIEL_BYTES) {
        let mut buf = [0u8; u256::BYTES as usize];
        buf[..chunk.len()].copy_from_slice(chunk);
        elems.push(u256::from_le_bytes(buf));
    }

    StateValue::from_iter(elems)
}
