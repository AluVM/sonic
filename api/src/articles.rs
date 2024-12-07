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

use strict_encoding::{StrictDeserialize, StrictSerialize, TypeName};
use ultrasonic::{Contract, ContractId};

use crate::sigs::ContentSigs;
use crate::{Api, Schema, LIB_NAME_SONIC};

/// Articles contain the contract and all related codex and API information for interacting with it.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Articles<const CAPS: u32> {
    pub contract: Contract<CAPS>,
    pub contract_sigs: ContentSigs,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub schema: Schema,
}

impl<const CAPS: u32> StrictSerialize for Articles<CAPS> {}
impl<const CAPS: u32> StrictDeserialize for Articles<CAPS> {}

impl<const CAPS: u32> Articles<CAPS> {
    pub fn contract_id(&self) -> ContractId { self.contract.contract_id() }

    pub fn api(&self, name: &TypeName) -> &Api { self.schema.api(name) }

    pub fn merge(&mut self, other: Self) -> Result<bool, MergeError> {
        if self.contract_id() != other.contract_id() {
            return Err(MergeError::ContractMismatch);
        }

        self.schema.merge(other.schema)?;
        self.contract_sigs.merge(other.contract_sigs);

        Ok(true)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum MergeError {
    /// contract id for the merged contract articles doesn't match
    ContractMismatch,

    /// codex id for the merged schema doesn't match
    CodexMismatch,
}

#[cfg(feature = "std")]
mod _fs {
    use std::path::Path;

    use strict_encoding::{DeserializeError, SerializeError, StrictDeserialize, StrictSerialize};

    use super::Articles;

    impl<const CAPS: u32> Articles<CAPS> {
        pub fn load(path: impl AsRef<Path>) -> Result<Self, DeserializeError> {
            Self::strict_deserialize_from_file::<{ usize::MAX }>(path)
        }

        pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SerializeError> {
            self.strict_serialize_to_file::<{ usize::MAX }>(path)
        }
    }
}
