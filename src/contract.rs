// SONIC: Toolchain for formally-verifiable distributed contracts
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

use commit_verify::ReservedBytes;
use strict_encoding::{StrictDecode, StrictDumb, StrictEncode, TypeName};
use ultrasonic::{Codex, ContractId, Identity, Operation, ProofOfPubl};

use crate::LIB_NAME_SONIC;

#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ContractId)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Contract<PoP: ProofOfPubl> {
    pub version: ReservedBytes<2>,
    pub meta: ContractMeta<PoP>,
    pub codex: Codex,
    pub initial: Operation,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ContractMeta<PoP: ProofOfPubl> {
    pub proof_of_publ: PoP,
    // aligning to 16 byte edge
    #[cfg_attr(feature = "serde", serde(skip))]
    pub reserved: ReservedBytes<10>,
    pub salt: u64,
    pub timestamp: i64,
    // ^^ above is a fixed-size contract header of 32 bytes
    pub name: ContractName,
    pub issuer: Identity,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display)]
#[display("~")]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct Private(ReservedBytes<4, 0xFF>);
impl From<Private> for [u8; 4] {
    fn from(_: Private) -> Self { [0xFF; 4] }
}
impl ProofOfPubl for Private {}

pub type ContractPrivate = Contract<Private>;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(untagged))]
pub enum ContractName {
    #[strict_type(tag = 0, dumb)]
    #[display("~")]
    Unnamed,

    #[strict_type(tag = 1)]
    #[display(inner)]
    Named(TypeName),
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display)]
#[display(inner)]
pub enum ContractRef {
    Id(ContractId),
    Name(TypeName),
    // Mnemonic(),
}
