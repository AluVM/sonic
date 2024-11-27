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

use strict_encoding::{StrictDeserialize, StrictSerialize, TypeName};
use ultrasonic::{Capabilities, Contract, ContractId};

use crate::sigs::ContentSigs;
use crate::{Api, Schema, LIB_NAME_SONIC};

/// Articles contain the contract and all related codex and API information for interacting with it.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Articles<C: Capabilities> {
    pub contract: Contract<C>,
    pub contract_sigs: ContentSigs,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub schema: Schema,
}

impl<C: Capabilities> StrictSerialize for Articles<C> {}
impl<C: Capabilities> StrictDeserialize for Articles<C> {}

impl<C: Capabilities> Articles<C> {
    pub fn contract_id(&self) -> ContractId { self.contract.contract_id() }

    pub fn api(&self, name: &TypeName) -> &Api { self.schema.api(name) }
}

#[cfg(feature = "std")]
mod _fs {
    use std::path::Path;

    use strict_encoding::{SerializeError, StrictSerialize};
    use ultrasonic::Capabilities;

    use super::Articles;

    // TODO: Compute/verify state on load from file

    impl<C: Capabilities> Articles<C> {
        pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SerializeError> {
            self.strict_serialize_to_file::<{ usize::MAX }>(path)
        }
    }
}
