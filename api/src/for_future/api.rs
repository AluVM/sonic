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

//! This is reserved for the future, when a multiple VM-based type of adaptors will be supported.
//! Then, the `Api` structure should replace the one from the `src/api.rs`, and the former should be
//! renamed into an `ApiInner`. Also, a `adaptor` field from it should be removed.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// API is an interface implementation.
///
/// API should work without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data. Basically, one may think of API as a compiled interface hierarchy applied to
/// a specific codex.
///
/// API doesn't commit to an interface, since it can match multiple interfaces in the interface
/// hierarchy.
#[derive(Clone, Debug, From)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ApiId)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Embedded(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[non_exhaustive]
pub enum Api {
    #[from]
    #[strict_type(tag = 0)]
    Embedded(ApiInner<EmbeddedProc>),

    #[from]
    #[strict_type(tag = 1)]
    Alu(ApiInner<aluvm::Vm>),
}

impl PartialEq for Api {
    fn eq(&self, other: &Self) -> bool { self.cmp(other) == Ordering::Equal }
}
impl Eq for Api {}
impl PartialOrd for Api {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for Api {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.api_id() == other.api_id() {
            Ordering::Equal
        } else {
            self.timestamp().cmp(&other.timestamp())
        }
    }
}
impl Hash for Api {
    fn hash<H: Hasher>(&self, state: &mut H) { self.api_id().hash(state); }
}

impl Api {
    pub fn api_id(&self) -> ApiId { self.commit_id() }

    pub fn vm_type(&self) -> VmType {
        match self {
            Api::Embedded(_) => VmType::Embedded,
            Api::Alu(_) => VmType::AluVM,
        }
    }

    pub fn codex_id(&self) -> CodexId {
        match self {
            Api::Embedded(api) => api.codex_id,
            Api::Alu(api) => api.codex_id,
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            Api::Embedded(api) => api.timestamp,
            Api::Alu(api) => api.timestamp,
        }
    }

    pub fn conforms(&self) -> Option<&TypeName> {
        match self {
            Api::Embedded(api) => api.conforms.as_ref(),
            Api::Alu(api) => api.conforms.as_ref(),
        }
    }

    pub fn developer(&self) -> &Identity {
        match self {
            Api::Embedded(api) => &api.developer,
            Api::Alu(api) => &api.developer,
        }
    }

    pub fn default_call(&self) -> Option<&CallState> {
        match self {
            Api::Embedded(api) => api.default_call.as_ref(),
            Api::Alu(api) => api.default_call.as_ref(),
        }
    }

    pub fn verifier(&self, method: impl Into<MethodName>) -> Option<CallId> {
        let method = method.into();
        match self {
            Api::Embedded(api) => api.verifiers.get(&method),
            Api::Alu(api) => api.verifiers.get(&method),
        }
        .copied()
    }

    pub fn readers(&self) -> Box<dyn Iterator<Item = &MethodName> + '_> {
        match self {
            Api::Embedded(api) => Box::new(api.readers.keys()),
            Api::Alu(api) => Box::new(api.readers.keys()),
        }
    }

    pub fn read<'s, I: IntoIterator<Item = &'s StateAtom>>(
        &self,
        name: &StateName,
        state: impl Fn(&StateName) -> I,
    ) -> StrictVal {
        match self {
            Api::Embedded(api) => api.read(name, state),
            Api::Alu(api) => api.read(name, state),
        }
    }

    pub fn convert_immutable(&self, data: &StateData, sys: &TypeSystem) -> Option<(StateName, StateAtom)> {
        match self {
            Api::Embedded(api) => api.convert_immutable(data, sys),
            Api::Alu(api) => api.convert_immutable(data, sys),
        }
    }

    pub fn convert_destructible(&self, value: StateValue, sys: &TypeSystem) -> Option<(StateName, StrictVal)> {
        // Here we do not yet known which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeded
        // convertor.
        match self {
            Api::Embedded(api) => api.convert_destructible(value, sys),
            Api::Alu(api) => api.convert_destructible(value, sys),
        }
    }

    pub fn build_immutable(
        &self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        sys: &TypeSystem,
    ) -> StateData {
        match self {
            Api::Embedded(api) => api.build_immutable(name, data, raw, sys),
            Api::Alu(api) => api.build_immutable(name, data, raw, sys),
        }
    }

    pub fn build_destructible(&self, name: impl Into<StateName>, data: StrictVal, sys: &TypeSystem) -> StateValue {
        let name = name.into();
        match self {
            Api::Embedded(api) => api
                .destructible
                .get(&name)
                .expect("state name is unknown for the API")
                .build(data, sys),
            /*Api::Alu(api) => api
            .destructible
            .get(&name)
            .expect("state name is unknown for the API")
            .build(data, sys),*/
        }
    }

    pub fn calculate(&self, name: impl Into<StateName>) -> Box<dyn StateCalc> {
        let name = name.into();
        match self {
            Api::Embedded(api) => api
                .destructible
                .get(&name)
                .expect("state name is unknown for the API")
                .arithmetics
                .calculator(),
            /*#[allow(clippy::let_unit_value)]
            Api::Alu(api) => api
                .destructible
                .get(&name)
                .expect("state name is unknown for the API")
                .arithmetics
                .calculator(),*/
        }
    }
}
