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

use alloc::collections::btree_map;

use amplify::confinement::{NonEmptyBlob, NonEmptyOrdMap};
use commit_verify::StrictHash;
use strict_encoding::StrictDumb;
use ultrasonic::Identity;

use crate::LIB_NAME_SONIC;

pub trait SigValidator {
    fn validate_sig(&self, identity: &Identity, sig: SigBlob) -> bool;
}

pub struct DumbValidator;
impl SigValidator for DumbValidator {
    fn validate_sig(&self, _: &Identity, _: SigBlob) -> bool { false }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, Default)]
#[display(lowercase)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = repr, into_u8, try_from_u8)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[repr(u8)]
pub enum TrustLevel {
    Malicious = 0x10,
    #[default]
    Unknown = 0x20,
    Untrusted = 0x40,
    Trusted = 0x80,
    Ultimate = 0xC0,
}

impl TrustLevel {
    pub fn should_accept(self) -> bool { self >= Self::Unknown }
    pub fn should_use(self) -> bool { self >= Self::Trusted }
    pub fn must_use(self) -> bool { self >= Self::Ultimate }
}

#[derive(Wrapper, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, From, Display)]
#[wrapper(Deref, AsSlice, BorrowSlice, Hex)]
#[display(LowerHex)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = StrictHash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct SigBlob(NonEmptyBlob<4096>);

impl Default for SigBlob {
    fn default() -> Self { SigBlob(NonEmptyBlob::with(0)) }
}

#[derive(Wrapper, WrapperMut, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug, From)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ContentSigs(NonEmptyOrdMap<Identity, SigBlob, 10>);

impl StrictDumb for ContentSigs {
    fn strict_dumb() -> Self { Self(NonEmptyOrdMap::with_key_value(strict_dumb!(), SigBlob::default())) }
}

impl IntoIterator for ContentSigs {
    type Item = (Identity, SigBlob);
    type IntoIter = btree_map::IntoIter<Identity, SigBlob>;

    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}
