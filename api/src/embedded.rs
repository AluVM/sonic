// SONIC: Toolchain for formally-verifiable distributed contracts
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

use amplify::confinement::{SmallBlob, U16 as U16MAX};
use strict_encoding::{StrictDecode, StrictEncode};
use strict_types::{SemId, StrictVal, TypeSystem};
use ultrasonic::{StateData, StateValue};

use crate::{ApiVm, StateAdaptor, StateArithm, StateName, StateTy, StructData, VmType, LIB_NAME_SONIC};

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
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Const(strict_dumb!()))]
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

    #[strict_type(tag = 0x20)]
    List(StateName, SemId),

    #[strict_type(tag = 0x21)]
    Set(StateName, SemId),

    #[strict_type(tag = 0x22)]
    Map { name: StateName, key: SemId, val: SemId },
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct EmbeddedImmutable(pub StateTy);

impl EmbeddedImmutable {
    const USED_FIEL_BYTES: usize = u128::BITS as usize / 8 - 1;
    const TOTAL_BYTES: usize = Self::USED_FIEL_BYTES * 3;

    fn convert_value(&self, sem_id: SemId, value: StateValue, sys: &TypeSystem) -> Option<StrictVal> {
        // State type doesn't match
        let ty = value.get(0)?.0;
        if ty != self.0 {
            return None;
        }

        let mut buf = [0u8; Self::TOTAL_BYTES];
        let mut i = 1u8;
        while let Some(el) = value.get(i) {
            let from = Self::USED_FIEL_BYTES * i as usize;
            let to = from + Self::USED_FIEL_BYTES;
            buf[from..to].copy_from_slice(&el.0.to_le_bytes());
            i += 1;
        }
        debug_assert_eq!(i, 4);

        let val = sys.strict_deserialize_type(sem_id, &buf).ok()?;
        Some(val.unbox())
    }

    fn build_value(&self, sem_id: SemId, value: StrictVal, sys: &TypeSystem) -> StateValue {
        let typed = sys
            .typify(value, sem_id)
            .expect("invalid strict value not matching semantic type information");
        let ser = sys
            .strict_serialize_value::<{ Self::TOTAL_BYTES }>(&typed)
            .expect("strict value is too large")
            .release();

        let mut elems = Vec::with_capacity(4);
        elems.push(self.0);
        for chunk in ser.chunks(Self::USED_FIEL_BYTES) {
            let mut buf = [0u8; u128::BITS as usize / 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            elems.push(u128::from_le_bytes(buf));
        }

        StateValue::from(elems)
    }
}

impl StateAdaptor for EmbeddedImmutable {
    fn convert_immutable(&self, sem_id: SemId, data: &StateData, sys: &TypeSystem) -> Option<StrictVal> {
        // TODO: Do something with raw
        self.convert_value(sem_id, data.value, sys)
    }

    fn convert_destructible(&self, sem_id: SemId, value: StateValue, sys: &TypeSystem) -> Option<StrictVal> {
        self.convert_value(sem_id, value, sys)
    }

    fn build_immutable(&self, sem_id: SemId, value: StrictVal, raw: Option<StrictVal>, sys: &TypeSystem) -> StateData {
        let value = self.build_value(sem_id, value, sys);
        let raw = raw.map(|raw| {
            let typed = sys
                .typify(raw, sem_id)
                .expect("invalid strict value not matching semantic type information");
            sys.strict_serialize_value::<U16MAX>(&typed)
                .expect("strict value is too large")
                .into()
        });
        StateData { value, raw }
    }

    fn build_destructible(&self, sem_id: SemId, value: StrictVal, sys: &TypeSystem) -> StateValue {
        self.build_value(sem_id, value, sys)
    }
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
    fn measure(&self, state: StructData) -> Option<u8> { todo!() }

    fn accumulate(&mut self, state: StructData) -> Option<()> { todo!() }

    fn lessen(&mut self, state: StructData) -> Option<()> { todo!() }

    fn diff(&self) -> Option<StructData> { todo!() }
}
