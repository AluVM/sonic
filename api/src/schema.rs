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

use aluvm::{Lib, LibId};
use amplify::confinement::{SmallOrdMap, SmallOrdSet, TinyOrdMap};
use commit_verify::ReservedBytes;
use strict_encoding::{StrictDeserialize, StrictSerialize, TypeName};
use strict_types::TypeSystem;
use ultrasonic::{CallId, Codex, LibRepo};

use crate::sigs::ContentSigs;
use crate::{Annotations, Api, MergeError, MethodName, LIB_NAME_SONIC};

/// Schema contains information required for creation of a contract.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Schema {
    pub codex: Codex,
    pub default_api: Api,
    pub default_api_sigs: ContentSigs,
    pub custom_apis: SmallOrdMap<Api, ContentSigs>,
    pub libs: SmallOrdSet<Lib>,
    pub types: TypeSystem,
    pub codex_sigs: ContentSigs,
    pub annotations: TinyOrdMap<Annotations, ContentSigs>,
    pub reserved: ReservedBytes<8>,
}

impl StrictSerialize for Schema {}
impl StrictDeserialize for Schema {}

impl LibRepo for Schema {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> { self.libs.iter().find(|lib| lib.lib_id() == lib_id) }
}

impl Schema {
    pub fn new(codex: Codex, api: Api, libs: impl IntoIterator<Item = Lib>, types: TypeSystem) -> Self {
        // TODO: Ensure default API is unnamed?
        Schema {
            codex,
            default_api: api,
            default_api_sigs: none!(),
            custom_apis: none!(),
            libs: SmallOrdSet::from_iter_checked(libs),
            types,
            codex_sigs: none!(),
            annotations: none!(),
            reserved: zero!(),
        }
    }

    pub fn api(&self, name: &TypeName) -> &Api {
        self.custom_apis
            .keys()
            .find(|api| api.name() == Some(name))
            .expect("API is not known")
    }

    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        self.default_api
            .verifier(method)
            .expect("unknown issue method absent in Codex API")
    }

    pub fn merge(&mut self, other: Self) -> Result<bool, MergeError> {
        if self.codex.codex_id() != other.codex.codex_id() {
            return Err(MergeError::CodexMismatch);
        }
        self.codex_sigs.merge(other.codex_sigs);

        if self.default_api != other.default_api {
            let _ = self
                .custom_apis
                .insert(other.default_api, other.default_api_sigs);
        } else {
            self.default_api_sigs.merge(other.default_api_sigs);
        }

        for (api, other_sigs) in other.custom_apis {
            let Ok(entry) = self.custom_apis.entry(api) else {
                continue;
            };
            entry.or_default().merge(other_sigs);
        }

        // NB: We must not fail here, since otherwise it opens an attack vector on invalidating valid
        // consignments by adding too many libs
        // TODO: Return warnings instead
        let _ = self.libs.extend(other.libs);
        let _ = self.types.extend(other.types);

        for (annotation, other_sigs) in other.annotations {
            let Ok(entry) = self.annotations.entry(annotation) else {
                continue;
            };
            entry.or_default().merge(other_sigs);
        }

        Ok(true)
    }
}

#[cfg(feature = "std")]
mod _fs {
    use std::path::Path;

    use strict_encoding::{DeserializeError, SerializeError, StrictDeserialize, StrictSerialize};

    use super::Schema;

    impl Schema {
        pub fn load(path: impl AsRef<Path>) -> Result<Self, DeserializeError> {
            Self::strict_deserialize_from_file::<{ usize::MAX }>(path)
        }

        pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SerializeError> {
            self.strict_serialize_to_file::<{ usize::MAX }>(path)
        }
    }
}
