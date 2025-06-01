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

use aluvm::LibSite;
use amplify::confinement::{Confined, ConfinedBlob};
use amplify::num::u256;
use sonic_callreq::StateName;
use strict_encoding::{SerializeError, StreamReader};
use strict_types::value::{EnumTag, StrictNum};
use strict_types::{decode, typify, Cls, SemId, StrictVal, Ty, TypeSystem};
use ultrasonic::StateValue;

use crate::{fe256, StateTy, LIB_NAME_SONIC};

pub(super) const USED_FIEL_BYTES: usize = u256::BYTES as usize - 2;
pub(super) const MAX_BYTES: usize = USED_FIEL_BYTES * 3;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::TypedEncoder(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateConvertor {
    #[strict_type(tag = 0x00)]
    Unit,

    #[strict_type(tag = 0x10)]
    TypedEncoder(StateTy),

    #[strict_type(tag = 0x11)]
    TypedFieldEncoder(StateTy),
    // In the future we can add more adaptors:
    // - doing more compact encoding (storing state type in bits, not using a full field element);
    // - using just a specific range of field element bits, not a full value - such that multiple APIs may read
    //   different parts of the same data;
    /// Execute a custom function.
    // AluVM is reserved for the future. We need it here to avoid breaking changes.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl StateConvertor {
    pub fn convert(
        &self,
        sem_id: SemId,
        value: StateValue,
        sys: &TypeSystem,
    ) -> Result<Option<StrictVal>, StateConvertError> {
        match self {
            Self::Unit if StateValue::None == value => Ok(Some(StrictVal::Unit)),
            Self::Unit => Err(StateConvertError::UnitState),
            Self::TypedEncoder(ty) => typed_convert(*ty, sem_id, value, sys),
            Self::TypedFieldEncoder(ty) => typed_field_convert(*ty, sem_id, value, sys),
            Self::AluVM(_) => Err(StateConvertError::Unsupported),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::TypedEncoder(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateBuilder {
    #[strict_type(tag = 0x00)]
    Unit,

    #[strict_type(tag = 0x10)]
    TypedEncoder(StateTy),

    #[strict_type(tag = 0x11)]
    TypedFieldEncoder(StateTy),
    // In the future we can add more adaptors:
    // - doing more compact encoding (storing state type in bits, not using a full field element);
    /// Execute a custom function.
    // AluVM is reserved for the future. We need it here to avoid breaking changes.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl StateBuilder {
    #[allow(clippy::result_large_err)]
    pub fn build(&self, sem_id: SemId, value: StrictVal, sys: &TypeSystem) -> Result<StateValue, StateBuildError> {
        let typed = sys.typify(value.clone(), sem_id)?;
        Ok(match self {
            Self::Unit if typed.as_val() == &StrictVal::Unit => StateValue::None,
            Self::Unit => return Err(StateBuildError::InvalidUnit),
            Self::TypedEncoder(ty) => {
                let ser = sys.strict_serialize_value::<MAX_BYTES>(&typed)?;
                typed_build(*ty, ser)
            }
            Self::TypedFieldEncoder(ty) => typed_field_build(*ty, value)?,
            Self::AluVM(_) => return Err(StateBuildError::Unsupported),
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

    #[display("state data ({0:?}) have an unsupported type for the encoding")]
    UnsupportedValue(StrictVal),

    #[from]
    Serialize(SerializeError),

    #[display("the provided value doesn't match the required unit type")]
    InvalidUnit,

    #[display("AluVM is not yet supported for a state builder.")]
    Unsupported,
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

    #[display("state has no data")]
    UnitState,

    #[display("unknown type {0}")]
    TypeUnknown(SemId),

    #[display("type of class {0} is not supported by field-based convertor")]
    TypeClassUnsupported(Cls),

    #[display("number of fields doesn't match the number of fields in the type")]
    TypeFieldCountMismatch,

    #[display("AluVM is not yet supported for a state conversion.")]
    Unsupported,
}

// Simplify newtype-like tuples
fn reduce_tuples(mut val: StrictVal) -> StrictVal {
    loop {
        if let StrictVal::Tuple(ref mut vec) = val {
            if vec.len() == 1 {
                val = vec.remove(0);
                continue;
            }
        }
        return val;
    }
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

    let mut buf = [0u8; MAX_BYTES];
    let mut i = 1u8;
    while let Some(el) = value.get(i) {
        let from = USED_FIEL_BYTES * (i - 1) as usize;
        let to = USED_FIEL_BYTES * i as usize;
        buf[from..to].copy_from_slice(&el.to_u256().to_le_bytes()[..USED_FIEL_BYTES]);
        i += 1;
    }
    let used_bytes = USED_FIEL_BYTES * (i - 1) as usize;
    debug_assert!(i <= 4);
    debug_assert!(used_bytes <= MAX_BYTES);

    let mut cursor = StreamReader::cursor::<MAX_BYTES>(&buf[..used_bytes]);
    let mut val = sys.strict_read_type(sem_id, &mut cursor)?.unbox();

    // We check here that we have reached the end of the buffer data,
    // and the rest of the elements are zeros.
    let cursor = cursor.unconfine();
    let position = cursor.position() as usize;
    let data = cursor.into_inner();
    for item in data.iter().take(used_bytes).skip(position) {
        if *item != 0 {
            return Err(StateConvertError::NotEntirelyConsumed);
        }
    }

    val = reduce_tuples(val);

    Ok(Some(val))
}

fn typed_field_convert(
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

    let ty = sys
        .get(sem_id)
        .ok_or(StateConvertError::TypeUnknown(sem_id))?;
    let fields = match ty {
        Ty::Tuple(fields) => fields.iter().copied().collect::<Vec<SemId>>(),
        Ty::Struct(fields) => fields.iter().map(|f| f.ty).collect::<Vec<SemId>>(),
        _ => return Err(StateConvertError::TypeClassUnsupported(ty.cls())),
    };

    if fields.len() != value.into_iter().count() - 1 {
        return Err(StateConvertError::TypeFieldCountMismatch);
    }

    let mut items = vec![];
    for (el, sem_id) in value.into_iter().skip(1).zip(fields.into_iter()) {
        let mut cursor = StreamReader::cursor::<MAX_BYTES>(el.to_u256().to_le_bytes());
        let val = sys.strict_read_type(sem_id, &mut cursor)?.unbox();
        items.push(val);
    }

    let mut val = match ty {
        Ty::Tuple(_) => StrictVal::Tuple(items),
        Ty::Struct(fields) => StrictVal::Struct(
            fields
                .iter()
                .zip(items)
                .map(|(f, val)| (f.name.clone(), reduce_tuples(val)))
                .collect(),
        ),
        _ => unreachable!(),
    };

    // Simplify tuples with a single element
    val = reduce_tuples(val);

    Ok(Some(val))
}

fn typed_build(ty: StateTy, ser: ConfinedBlob<0, MAX_BYTES>) -> StateValue {
    let mut elems = Vec::with_capacity(4);
    elems.push(ty);
    for chunk in ser.chunks(USED_FIEL_BYTES) {
        let mut buf = [0u8; u256::BYTES as usize];
        buf[..chunk.len()].copy_from_slice(chunk);
        elems.push(u256::from_le_bytes(buf));
    }

    StateValue::from_iter(elems)
}

#[allow(clippy::result_large_err)]
fn typed_field_build(ty: StateTy, val: StrictVal) -> Result<StateValue, StateBuildError> {
    let mut elems = Vec::with_capacity(4);
    elems.push(ty);

    Ok(match val {
        StrictVal::Unit => StateValue::Single { first: fe256::from(ty) },
        StrictVal::Number(StrictNum::Uint(i)) => StateValue::Double { first: fe256::from(ty), second: fe256::from(i) },
        StrictVal::String(s) if s.len() < MAX_BYTES => {
            typed_build(ty, Confined::from_iter_checked(s.as_bytes().iter().cloned()))
        }
        StrictVal::Bytes(b) if b.len() < MAX_BYTES => typed_build(ty, Confined::from_checked(b.0)),
        StrictVal::Struct(fields) if fields.len() <= 3 => typed_field_build_items(ty, fields.into_values())?,
        StrictVal::Enum(EnumTag::Ord(tag)) => StateValue::Double { first: fe256::from(ty), second: fe256::from(tag) },
        StrictVal::List(items) | StrictVal::Set(items) | StrictVal::Tuple(items) if items.len() <= 3 => {
            typed_field_build_items(ty, items)?
        }
        _ => return Err(StateBuildError::UnsupportedValue(val)),
    })
}

#[allow(clippy::result_large_err)]
fn typed_field_build_items(
    ty: StateTy,
    vals: impl IntoIterator<Item = StrictVal>,
) -> Result<StateValue, StateBuildError> {
    let mut items = Vec::with_capacity(4);
    items.push(ty);
    for val in vals {
        if let Some(val) = typed_field_build_item(val)? {
            items.push(val);
        }
    }
    Ok(StateValue::from_iter(items))
}

#[allow(clippy::result_large_err)]
fn typed_field_build_item(val: StrictVal) -> Result<Option<u256>, StateBuildError> {
    Ok(match val {
        StrictVal::Unit => None,
        StrictVal::Tuple(items) if items.len() == 1 => typed_field_build_item(items[0].clone())?,
        StrictVal::Number(StrictNum::Uint(i)) => Some(u256::from(i)),
        StrictVal::String(s) if s.len() < USED_FIEL_BYTES => {
            let mut buf = [0u8; u256::BYTES as usize];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            Some(u256::from_le_bytes(buf))
        }
        StrictVal::Bytes(b) if b.len() < USED_FIEL_BYTES => {
            let mut buf = [0u8; u256::BYTES as usize];
            buf[..b.len()].copy_from_slice(&b.0);
            Some(u256::from_le_bytes(buf))
        }
        StrictVal::Enum(EnumTag::Ord(tag)) => Some(u256::from(tag)),
        _ => return Err(StateBuildError::UnsupportedValue(val)),
    })
}

#[cfg(test)]
mod tests {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use strict_types::stl::std_stl;
    use strict_types::{LibBuilder, SymbolicSys, SystemBuilder, TypeLib};

    use super::*;

    pub const LIB_NAME_TEST: &str = "Test";

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(lowercase)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_TEST, tags = repr, try_from_u8, into_u8)]
    #[repr(u8)]
    pub enum Vote {
        #[strict_type(dumb)]
        Contra = 0,
        Pro = 1,
    }

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(inner)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_TEST)]
    pub struct VoteId(u64);

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(inner)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_TEST)]
    pub struct PartyId(u64);

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From, Display)]
    #[display("Participant #{party_id} voted {vote} in voting #{vote_id}")]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_TEST)]
    pub struct CastVote {
        pub vote_id: VoteId,
        pub vote: Vote,
        pub party_id: PartyId,
    }

    pub fn stl() -> TypeLib {
        LibBuilder::with(libname!(LIB_NAME_TEST), [std_stl().to_dependency_types()])
            .transpile::<CastVote>()
            .compile()
            .expect("invalid Test type library")
    }

    #[derive(Debug)]
    pub struct Types(SymbolicSys);

    impl Types {
        pub fn new() -> Self {
            Self(
                SystemBuilder::new()
                    .import(std_stl())
                    .unwrap()
                    .import(stl())
                    .unwrap()
                    .finalize()
                    .unwrap(),
            )
        }

        pub fn type_system(&self) -> TypeSystem {
            let stdtypes = std_stl().types;
            let types = stl().types;
            let types = stdtypes
                .into_iter()
                .chain(types)
                .map(|(tn, ty)| ty.sem_id_named(&tn));
            self.0.as_types().extract(types).unwrap()
        }

        pub fn get(&self, name: &'static str) -> SemId {
            *self
                .0
                .resolve(name)
                .unwrap_or_else(|| panic!("type '{name}' is absent in RGB21 type library"))
        }
    }

    fn typed_roundtrip(name: &'static str, src: StateValue, dst: StrictVal) {
        let types = Types::new();

        let ty = types.get(name);
        let val = StateConvertor::TypedEncoder(u256::ONE)
            .convert(ty, src, &types.type_system())
            .unwrap()
            .unwrap();
        assert_eq!(val, dst);

        let res = StateBuilder::TypedEncoder(u256::ONE)
            .build(ty, dst, &types.type_system())
            .unwrap();
        assert_eq!(res, src);
    }

    fn typed_field_roundtrip(name: &'static str, src1: StateValue, dst: StrictVal, src2: StrictVal) {
        let types = Types::new();

        let ty = types.get(name);
        let val = StateConvertor::TypedFieldEncoder(u256::ONE)
            .convert(ty, src1, &types.type_system())
            .unwrap()
            .unwrap();
        assert_eq!(val, dst);

        let res = StateBuilder::TypedFieldEncoder(u256::ONE)
            .build(ty, src2, &types.type_system())
            .unwrap();
        assert_eq!(res, src1);
    }

    #[test]
    fn typed() {
        typed_roundtrip(
            "Std.Bool",
            StateValue::Double { first: fe256::from(1u8), second: fe256::from(1u8) },
            svenum!("true"),
        );
    }

    #[test]
    #[should_panic(expected = "Decode(Decode(Io(Kind(UnexpectedEof))))")]
    fn typed_convert_lack() {
        let types = Types::new();
        StateConvertor::TypedEncoder(u256::ONE)
            .convert(types.get("Std.Bool"), StateValue::Single { first: fe256::from(1u8) }, &types.type_system())
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "NotEntirelyConsumed")]
    fn typed_convert_excess() {
        let types = Types::new();
        StateConvertor::TypedEncoder(u256::ONE)
            .convert(
                types.get("Std.Bool"),
                StateValue::Triple {
                    first: fe256::from(1u8),
                    second: fe256::from(1u8),
                    third: fe256::from(1u8),
                },
                &types.type_system(),
            )
            .unwrap();
    }

    #[test]
    fn typed_field() {
        typed_field_roundtrip(
            "Test.CastVote",
            StateValue::Quadruple {
                first: fe256::from(1u8),
                second: fe256::from(3u8),
                third: fe256::from(1u8),
                fourth: fe256::from(5u8),
            },
            ston!(voteId 3u8, vote svenum!("pro"), partyId 5u8),
            ston!(voteId 3u8, vote svenum!(1), partyId 5u8),
        );
    }

    #[test]
    #[should_panic(expected = "TypeClassUnsupported(Enum)")]
    fn typed_field_convert_enum() {
        let types = Types::new();
        let val = StateConvertor::TypedFieldEncoder(u256::ONE)
            .convert(
                types.get("Std.Bool"),
                StateValue::Double { first: fe256::from(1u8), second: fe256::from(1u8) },
                &types.type_system(),
            )
            .unwrap();
        assert_eq!(val, Some(svenum!("true")));
    }

    #[test]
    #[should_panic(expected = "TypeFieldCountMismatch")]
    fn typed_field_convert_lack() {
        let types = Types::new();
        StateConvertor::TypedFieldEncoder(u256::ONE)
            .convert(types.get("Test.CastVote"), StateValue::Single { first: fe256::from(1u8) }, &types.type_system())
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "TypeFieldCountMismatch")]
    fn typed_field_convert_excess() {
        let types = Types::new();
        StateConvertor::TypedFieldEncoder(u256::ONE)
            .convert(
                types.get("Test.PartyId"),
                StateValue::Triple {
                    first: fe256::from(1u8),
                    second: fe256::from(1u8),
                    third: fe256::from(1u8),
                },
                &types.type_system(),
            )
            .unwrap();
    }

    #[test]
    #[should_panic(
        expected = r#"Decode(Decode(EnumTagNotKnown("semid:kr1DHi~j-YSw4n54-o9KnZ9Q-Dlo0pWP-_V9U5oh-Wlzfemk#break-secret-delphi", 5)))"#
    )]
    fn typed_field_convert_invalid() {
        let types = Types::new();
        StateConvertor::TypedFieldEncoder(u256::ONE)
            .convert(
                types.get("Test.CastVote"),
                StateValue::Quadruple {
                    first: fe256::from(1u8),
                    second: fe256::from(1u8),
                    third: fe256::from(5u8),
                    fourth: fe256::from(1u8),
                },
                &types.type_system(),
            )
            .unwrap();
    }
}
