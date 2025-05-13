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

use core::str::FromStr;

use amplify::confinement::ConfinedBlob;
use amplify::num::u256;
use strict_encoding::{StreamReader, StrictDecode, StrictEncode};
use strict_types::typify::TypedVal;
use strict_types::value::StrictNum;
use strict_types::{SemId, StrictVal, TypeSystem};
use ultrasonic::{StateData, StateValue};

use crate::api::{TOTAL_BYTES, USED_FIEL_BYTES};
use crate::{
    ApiVm, StateAdaptor, StateArithm, StateAtom, StateCalc, StateCalcError, StateName, StateReader, StateTy, VmType,
    LIB_NAME_SONIC,
};

#[derive(Clone, Debug)]
pub struct EmbeddedProc;

impl ApiVm for EmbeddedProc {
    type Arithm = EmbeddedArithm;
    type Reader = EmbeddedReaders;
    type Adaptor = EmbeddedImmutable;

    fn vm_type(&self) -> VmType { VmType::Embedded }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Count(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum EmbeddedReaders {
    // #[strict_type(tag = 0)]
    // Const(StrictVal),
    #[strict_type(tag = 1)]
    Count(StateName),

    /// Sum over verifiable field-element-based part of state.
    ///
    /// If any of the verifiable state is absent or not in a form of unsigned integer, it is treated
    /// as zero.
    #[strict_type(tag = 2)]
    SumV(StateName),

    /*
    /// Count values which verifiable field-element part binary representation is prefixed with a
    /// given byte string.
    #[strict_type(tag = 0x10)]
    CountPrefixedV(StateName, TinyBlob),
     */
    /// Convert a verified state under the same state type into a vector.
    #[strict_type(tag = 0x20)]
    ListV(StateName),

    /// Convert a verified state under the same state type into a sorted set.
    #[strict_type(tag = 0x22)]
    SetV(StateName),

    /// Map from a field-based element state to a non-verifiable structured state
    #[strict_type(tag = 0x30)]
    MapV2U(StateName),
}

impl StateReader for EmbeddedReaders {
    fn read<'s, I: IntoIterator<Item = &'s StateAtom>>(&self, state: impl Fn(&StateName) -> I) -> StrictVal {
        match self {
            //EmbeddedReaders::Const(val) => val.clone(),
            EmbeddedReaders::Count(name) => {
                let count = state(name).into_iter().count();
                svnum!(count as u64)
            }
            EmbeddedReaders::SumV(name) => {
                let sum = state(name)
                    .into_iter()
                    .map(|atom| match &atom.verified {
                        StrictVal::Number(StrictNum::Uint(val)) => *val,
                        _ => 0u64,
                    })
                    .sum::<u64>();
                svnum!(sum)
            }
            EmbeddedReaders::ListV(name) => StrictVal::List(
                state(name)
                    .into_iter()
                    .map(|atom| atom.verified.clone())
                    .collect(),
            ),
            EmbeddedReaders::SetV(name) => {
                let mut set = Vec::new();
                for atom in state(name) {
                    if !set.contains(&atom.verified) {
                        set.push(atom.verified.clone());
                    }
                }
                StrictVal::Set(set)
            }
            EmbeddedReaders::MapV2U(name) => {
                let mut map = Vec::new();
                for atom in state(name) {
                    let Some(val) = &atom.unverified else { continue };
                    if map.iter().any(|(key, _)| &atom.verified == key) {
                        continue;
                    }
                    map.push((atom.verified.clone(), val.clone()));
                }
                StrictVal::Map(map)
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct EmbeddedImmutable(pub StateTy);

impl EmbeddedImmutable {
    fn convert_value(&self, sem_id: SemId, value: StateValue, sys: &TypeSystem) -> Option<StrictVal> {
        // State type doesn't match
        let ty = value.get(0)?.to_u256();
        if ty != self.0 {
            return None;
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
        let mut val = sys.strict_read_type(sem_id, &mut cursor).ok()?.unbox();

        loop {
            if let StrictVal::Tuple(ref mut vec) = val {
                if vec.len() == 1 {
                    val = vec.remove(0);
                    continue;
                }
            }
            break;
        }

        Some(val)
    }

    fn build_value(&self, ser: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue {
        let mut elems = Vec::with_capacity(4);
        elems.push(self.0);
        for chunk in ser.chunks(USED_FIEL_BYTES) {
            let mut buf = [0u8; u256::BYTES as usize];
            buf[..chunk.len()].copy_from_slice(chunk);
            elems.push(u256::from_le_bytes(buf));
        }

        StateValue::from_iter(elems)
    }
}

impl StateAdaptor for EmbeddedImmutable {
    fn convert_immutable(
        &self,
        sem_id: SemId,
        raw_sem_id: SemId,
        data: &StateData,
        sys: &TypeSystem,
    ) -> Option<StateAtom> {
        let verified = self.convert_value(sem_id, data.value, sys)?;
        let unverified = data
            .raw
            .as_ref()
            .and_then(|raw| sys.strict_deserialize_type(raw_sem_id, raw.as_ref()).ok())
            .map(TypedVal::unbox);
        Some(StateAtom { verified, unverified })
    }

    fn convert_destructible(&self, sem_id: SemId, value: StateValue, sys: &TypeSystem) -> Option<StrictVal> {
        self.convert_value(sem_id, value, sys)
    }

    fn build_immutable(&self, value: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue { self.build_value(value) }

    fn build_destructible(&self, value: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue { self.build_value(value) }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = repr, try_from_u8, into_u8)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[repr(u8)]
pub enum EmbeddedArithm {
    #[strict_type(dumb)]
    NonFungible = 0,
    Fungible = 1,
}

impl StateArithm for EmbeddedArithm {
    fn calculator(&self) -> Box<dyn StateCalc> {
        match self {
            EmbeddedArithm::NonFungible => Box::new(EmbeddedCalc::NonFungible(empty!())),
            EmbeddedArithm::Fungible => Box::new(EmbeddedCalc::Fungible(StrictVal::Number(StrictNum::Uint(0)))),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum EmbeddedCalc {
    NonFungible(Vec<StrictVal>),
    Fungible(StrictVal),
}

impl StateCalc for EmbeddedCalc {
    fn accumulate(&mut self, state: &StrictVal) -> Result<(), StateCalcError> {
        match self {
            EmbeddedCalc::NonFungible(states) => {
                states.push(state.clone());
                Ok(())
            }
            EmbeddedCalc::Fungible(value) => {
                let (val, add) = match (state, value) {
                    // TODO: Remove unsafe once rust supports `if let` guards
                    (StrictVal::String(s), StrictVal::Number(StrictNum::Uint(val))) if u64::from_str(s).is_ok() => {
                        let add = unsafe { u64::from_str(s).unwrap_unchecked() };
                        (val, add)
                    }
                    (StrictVal::Number(StrictNum::Uint(add)), StrictVal::Number(StrictNum::Uint(val))) => (val, *add),
                    _ => return Err(StateCalcError::UncountableState),
                };
                *val = val.checked_add(add).ok_or(StateCalcError::Overflow)?;
                Ok(())
            }
        }
    }

    fn lessen(&mut self, state: &StrictVal) -> Result<(), StateCalcError> {
        match self {
            EmbeddedCalc::NonFungible(states) => {
                if let Some(pos) = states.iter().position(|s| s == state) {
                    states.remove(pos);
                    Ok(())
                } else {
                    Err(StateCalcError::UncountableState)
                }
            }
            EmbeddedCalc::Fungible(value) => {
                let (val, dec) = match (state, value) {
                    // TODO: Remove unsafe once rust supports `if let` guards
                    (StrictVal::String(s), StrictVal::Number(StrictNum::Uint(val))) if u64::from_str(s).is_ok() => {
                        let dec = unsafe { u64::from_str(s).unwrap_unchecked() };
                        (val, dec)
                    }
                    (StrictVal::Number(StrictNum::Uint(dec)), StrictVal::Number(StrictNum::Uint(val))) => (val, *dec),
                    _ => return Err(StateCalcError::UncountableState),
                };
                if dec > *val {
                    return Err(StateCalcError::Overflow);
                }
                *val -= dec;
                Ok(())
            }
        }
    }

    fn diff(&self) -> Result<Vec<StrictVal>, StateCalcError> {
        Ok(match self {
            EmbeddedCalc::NonFungible(items) => items.clone(),
            EmbeddedCalc::Fungible(value) => match value {
                StrictVal::Number(StrictNum::Uint(val)) => {
                    if val.eq(&u64::MIN) {
                        vec![]
                    } else {
                        vec![value.clone()]
                    }
                }
                _ => return Err(StateCalcError::UncountableState),
            },
        })
    }

    fn is_satisfied(&self, target: &StrictVal) -> bool {
        match self {
            EmbeddedCalc::NonFungible(items) => items.contains(target),
            EmbeddedCalc::Fungible(value) => {
                if value == target {
                    true
                } else if let StrictVal::Number(StrictNum::Uint(val)) = value {
                    if let StrictVal::Number(StrictNum::Uint(tgt)) = target {
                        val >= tgt
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn vm_type() {
        assert_eq!(EmbeddedProc.vm_type(), VmType::Embedded);
    }

    #[test]
    fn verified_readers() {
        let state = [
            StateAtom::new_verified(5u64),
            StateAtom::new_verified(1u64),
            StateAtom::new_verified(2u64),
            StateAtom::new_verified(3u64),
            StateAtom::new_verified(4u64),
            StateAtom::new_verified(5u64),
        ];

        let adaptor = EmbeddedReaders::Count(vname!("test1"));
        assert_eq!(
            adaptor.read(|name| {
                assert_eq!(name.as_str(), "test1");
                state.iter()
            }),
            svnum!(6u64)
        );

        let adaptor = EmbeddedReaders::SumV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svnum!(5u64 + 1 + 2 + 3 + 4 + 5));

        let adaptor = EmbeddedReaders::ListV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svlist!([5u64, 1u64, 2u64, 3u64, 4u64, 5u64]));

        let adaptor = EmbeddedReaders::SetV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svset!([5u64, 1u64, 2u64, 3u64, 4u64]));

        let adaptor = EmbeddedReaders::MapV2U(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), StrictVal::Map(none!()));
    }

    #[test]
    fn unverified_readers() {
        let state = [
            StateAtom::new_unverified(5u64),
            StateAtom::new_unverified(1u64),
            StateAtom::new_unverified(2u64),
            StateAtom::new_unverified(3u64),
            StateAtom::new_unverified(4u64),
            StateAtom::new_unverified(5u64),
        ];

        let adaptor = EmbeddedReaders::Count(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svnum!(6u64));

        let adaptor = EmbeddedReaders::SumV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svnum!(0u64));

        let adaptor = EmbeddedReaders::ListV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svlist!([(), (), (), (), (), ()]));

        let adaptor = EmbeddedReaders::SetV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svset!([()]));

        let adaptor = EmbeddedReaders::MapV2U(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), StrictVal::Map(vec![(StrictVal::Unit, svnum!(5u64)),]));
    }

    #[test]
    fn pair_readers() {
        let state = [
            StateAtom::new(5u64, "state 1"),
            StateAtom::new(1u64, "state 2"),
            StateAtom::new(2u64, "state 3"),
            StateAtom::new(3u64, "state 4"),
            StateAtom::new(4u64, "state 5"),
            StateAtom::new(5u64, "state 6"),
        ];

        let adaptor = EmbeddedReaders::Count(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svnum!(6u64));

        let adaptor = EmbeddedReaders::SumV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svnum!(5u64 + 1 + 2 + 3 + 4 + 5));

        let adaptor = EmbeddedReaders::ListV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svlist!([5u64, 1u64, 2u64, 3u64, 4u64, 5u64]));

        let adaptor = EmbeddedReaders::SetV(vname!("test"));
        assert_eq!(adaptor.read(|_| { state.iter() }), svset!([5u64, 1u64, 2u64, 3u64, 4u64]));

        let adaptor = EmbeddedReaders::MapV2U(vname!("test"));
        assert_eq!(
            adaptor.read(|_| { state.iter() }),
            StrictVal::Map(vec![
                (svnum!(5u64), svstr!("state 1")),
                (svnum!(1u64), svstr!("state 2")),
                (svnum!(2u64), svstr!("state 3")),
                (svnum!(3u64), svstr!("state 4")),
                (svnum!(4u64), svstr!("state 5"))
            ])
        );
    }
}
