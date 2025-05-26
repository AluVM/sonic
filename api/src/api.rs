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

//! API defines how software can interface a contract.
//!
//! SONIC provides four types of actions for working with contract (ROVT):
//! 1. _Read_ the state of the contract;
//! 2. _Operate_: construct new operations performing contract state transitions;
//! 3. _Verify_ an existing operation under the contract Codex and generate transaction;
//! 4. _Transact_: apply or roll-back transactions to the contract state.
//!
//! API defines methods for human-based interaction with the contract for read and operate actions.
//! The "verify" part is implemented in the consensus layer (UltraSONIC), the "transact" part is
//! performed directly, so these two are not covered by an API.

use core::cmp::Ordering;
use core::fmt;
use core::fmt::{Debug, Display, Formatter};
use core::hash::{Hash, Hasher};
use core::num::ParseIntError;
use core::str::FromStr;

use aluvm::{Lib, LibId};
use amplify::confinement::{SmallOrdMap, SmallOrdSet, TinyOrdMap, TinyOrdSet, TinyString};
use amplify::num::u256;
use amplify::Bytes4;
use baid64::{Baid64ParseError, DisplayBaid64};
use commit_verify::{CommitEncode, CommitEngine, CommitId, CommitmentId, StrictHash};
use indexmap::{indexset, IndexMap, IndexSet};
use sonic_callreq::{CallState, MethodName, StateName};
use strict_encoding::TypeName;
use strict_types::{SemId, StrictDecode, StrictDumb, StrictEncode, StrictVal, TypeSystem};
use ultrasonic::{CallId, Codex, CodexId, StateData, StateValue};

use crate::{
    Aggregator, RawBuilder, RawConvertor, StateArithm, StateAtom, StateBuildError, StateBuilder, StateCalc,
    StateConvertError, StateConvertor, LIB_NAME_SONIC,
};

/// Create a versioned variant of a commitment ID (contract or codex), so information about a
/// specific API version is added.
///
/// Both contracts and codexes may have multiple API implementations, which may be versioned.
/// Issuers and articles include a specific version of the codex and contract APIs.
/// This structure provides the necessary information for the user about a specific API version
/// known and used by a system, so a user may avoid confusion when an API change due to upgrade
/// happens.
///
/// # See also
///
/// - [`ContractId`]
/// - [`CodexId`]
/// - [`crate::ArticlesId`]
/// - [`crate::IssuerId`]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = StrictHash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Versioned<Id: CommitmentId + StrictDumb + StrictEncode + StrictDecode> {
    /// An identifier of the contract or codex.
    pub id: Id,
    /// Version number of the API.
    pub version: u16,
    /// A checksum for the APIs from the Semantics structure.
    pub checksum: ApisChecksum,
}

impl<Id> Display for Versioned<Id>
where Id: CommitmentId + StrictDumb + StrictEncode + StrictDecode + Display + DisplayBaid64
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if Id::MNEMONIC {
            write!(f, "{:#}/{}#", self.id, self.version)?;
        } else {
            write!(f, "{}/{}#", self.id, self.version)?;
        }
        self.checksum.fmt_baid64(f)
    }
}

impl<Id> FromStr for Versioned<Id>
where Id: CommitmentId + StrictDumb + StrictEncode + StrictDecode + FromStr<Err = Baid64ParseError>
{
    type Err = ParseVersionedError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (id, remnant) = s
            .split_once('/')
            .ok_or_else(|| ParseVersionedError::NoVersion(s.to_string()))?;
        let (version, api_id) = remnant
            .split_once('#')
            .ok_or_else(|| ParseVersionedError::NoChecksum(s.to_string()))?;
        Ok(Self {
            id: id.parse().map_err(ParseVersionedError::Id)?,
            version: version.parse().map_err(ParseVersionedError::Version)?,
            checksum: api_id.parse().map_err(ParseVersionedError::Checksum)?,
        })
    }
}

/// Errors happening during parsing of a versioned contract or codex ID.
#[derive(Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum ParseVersionedError {
    /// the versioned id '{0}' misses the version component, which should be provided after a `/`
    /// sign.
    NoVersion(String),
    /// the versioned id '{0}' misses the API checksum component, which should be provided after a
    /// `#` sign.
    NoChecksum(String),
    /// invalid versioned identifier; {0}
    Id(Baid64ParseError),
    #[from]
    /// invalid versioned number; {0}
    Version(ParseIntError),
    /// invalid API checksum value; {0}
    Checksum(Baid64ParseError),
}

/// API checksum computed from a set of contract APIs present in [`Semantics`].
///
/// # Nota bene
///
/// This is not a unique identifier!
/// It is created just for UI, so users can easily visually distinguish different sets of APIs from
/// each other.
///
/// This type is not - and must not be used in any verification.
#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, BorrowSlice, Hex, Index, RangeOps)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct ApisChecksum(
    #[from]
    #[from([u8; 4])]
    Bytes4,
);

mod _baid4 {
    use core::fmt::{self, Display, Formatter};
    use core::str::FromStr;

    use amplify::ByteArray;
    use baid64::{Baid64ParseError, DisplayBaid64, FromBaid64Str};
    use commit_verify::{CommitmentId, DigestExt, Sha256};

    use super::*;

    impl DisplayBaid64<4> for ApisChecksum {
        const HRI: &'static str = "api";
        const CHUNKING: bool = false;
        const PREFIX: bool = false;
        const EMBED_CHECKSUM: bool = false;
        const MNEMONIC: bool = false;
        fn to_baid64_payload(&self) -> [u8; 4] { self.to_byte_array() }
    }
    impl FromBaid64Str<4> for ApisChecksum {}
    impl FromStr for ApisChecksum {
        type Err = Baid64ParseError;
        fn from_str(s: &str) -> Result<Self, Self::Err> { Self::from_baid64_str(s) }
    }
    impl Display for ApisChecksum {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { self.fmt_baid64(f) }
    }

    impl From<Sha256> for ApisChecksum {
        fn from(hasher: Sha256) -> Self {
            let hash = hasher.finish();
            Self::from_slice_checked(&hash[..4])
        }
    }

    impl CommitmentId for ApisChecksum {
        const TAG: &'static str = "urn:ubideco:sonic:apis#2025-05-25";
    }

    #[cfg(feature = "serde")]
    ultrasonic::impl_serde_str_bin_wrapper!(ApisChecksum, Bytes4);
}

/// A helper structure to store the contract semantics, made of a set of APIs, corresponding type
/// system, and libs, used by the codex.
///
/// A contract may have multiple APIs defined; this structure summarizes information about them.
/// The structure also holds a set of AluVM libraries for the codex and type system used by the
/// APIs.
#[derive(Clone, Eq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Semantics {
    /// Backward-compatible version number for the issuer.
    ///
    /// This version number is used to decide which contract APIs to apply if multiple
    /// contract APIs are available.
    pub version: u16,
    /// The default API.
    pub default: Api,
    /// The custom named APIs.
    ///
    /// The mechanism of the custom APIs allows a contract to have multiple implementations
    /// of the same interface.
    ///
    /// For instance, a contract may provide multiple tokens using different token names.
    pub custom: SmallOrdMap<TypeName, Api>,
    /// A set of zk-AluVM libraries called from the contract codex.
    pub codex_libs: SmallOrdSet<Lib>,
    /// A set of AluVM libraries called from the APIs.
    pub api_libs: SmallOrdSet<Lib>,
    /// The type system used by the contract APIs.
    pub types: TypeSystem,
}

impl PartialEq for Semantics {
    fn eq(&self, other: &Self) -> bool {
        self.default.codex_id == other.default.codex_id
            && self.version == other.version
            && self.commit_id() == other.commit_id()
    }
}
impl PartialOrd for Semantics {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for Semantics {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.default.codex_id.cmp(&other.default.codex_id) {
            Ordering::Equal => match self.version.cmp(&other.version) {
                Ordering::Equal => self.commit_id().cmp(&other.commit_id()),
                other => other,
            },
            other => other,
        }
    }
}

impl CommitEncode for Semantics {
    type CommitmentId = ApisChecksum;
    fn commit_encode(&self, e: &mut CommitEngine) {
        e.commit_to_serialized(&self.version);
        // We do not commit to the codex_libs since thea are not a part of APIs and are commit to inside the
        // codex. The fact that there are no other libs is verified in the Articles and Issuer constructors.
        let apis = SmallOrdMap::from_iter_checked(
            self.custom
                .iter()
                .map(|(name, api)| (name.clone(), api.api_id())),
        );
        e.commit_to_linear_map(&apis);
        let libs = SmallOrdSet::from_iter_checked(self.api_libs.iter().map(Lib::lib_id));
        e.commit_to_linear_set(&libs);
        e.commit_to_serialized(&self.types.id());
    }
}

impl Semantics {
    pub fn apis_checksum(&self) -> ApisChecksum { self.commit_id() }

    /// Iterates over all APIs, including default and named ones.
    pub fn apis(&self) -> impl Iterator<Item = &Api> { [&self.default].into_iter().chain(self.custom.values()) }

    /// Check whether this semantics object matches codex and the provided set of libraries for it.
    pub fn check(&self, codex: &Codex) -> Result<(), SemanticError> {
        let codex_id = codex.codex_id();

        let mut ids = bset![];
        for api in self.apis() {
            if api.codex_id != codex_id {
                return Err(SemanticError::CodexMismatch);
            }
            let api_id = api.api_id();
            if !ids.insert(api_id) {
                return Err(SemanticError::DuplicatedApi(api_id));
            }
        }

        // Check codex libs for redundancies and completeness
        let lib_map = self
            .codex_libs
            .iter()
            .map(|lib| (lib.lib_id(), lib))
            .collect::<IndexMap<_, _>>();

        let mut lib_ids = codex
            .verifiers
            .values()
            .map(|entry| entry.lib_id)
            .collect::<IndexSet<_>>();
        let mut i = 0usize;
        let mut count = lib_ids.len();
        while i < count {
            let id = lib_ids.get_index(i).expect("index is valid");
            let lib = lib_map.get(id).ok_or(SemanticError::MissedCodexLib(*id))?;
            lib_ids.extend(lib.libs.iter().copied());
            count = lib_ids.len();
            i += 1;
        }
        for id in lib_map.keys() {
            if !lib_ids.contains(id) {
                return Err(SemanticError::ExcessiveCodexLib(*id));
            }
        }

        // Check API libs for redundancies and completeness
        let lib_map = self
            .api_libs
            .iter()
            .map(|lib| (lib.lib_id(), lib))
            .collect::<IndexMap<_, _>>();

        let mut lib_ids = indexset![];
        for api in self.apis() {
            for agg in api.aggregators.values() {
                if let Aggregator::AluVM(entry) = agg {
                    lib_ids.insert(entry.lib_id);
                }
            }
            for glob in api.global.values() {
                if let StateConvertor::AluVM(entry) = glob.convertor {
                    lib_ids.insert(entry.lib_id);
                }
                if let StateBuilder::AluVM(entry) = glob.builder {
                    lib_ids.insert(entry.lib_id);
                }
                if let RawConvertor::AluVM(entry) = glob.raw_convertor {
                    lib_ids.insert(entry.lib_id);
                }
                if let RawBuilder::AluVM(entry) = glob.raw_builder {
                    lib_ids.insert(entry.lib_id);
                }
            }
            for owned in api.owned.values() {
                if let StateConvertor::AluVM(entry) = owned.convertor {
                    lib_ids.insert(entry.lib_id);
                }
                if let StateBuilder::AluVM(entry) = owned.builder {
                    lib_ids.insert(entry.lib_id);
                }
                if let StateBuilder::AluVM(entry) = owned.witness_builder {
                    lib_ids.insert(entry.lib_id);
                }
                if let StateArithm::AluVM(entry) = owned.arithmetics {
                    lib_ids.insert(entry.lib_id);
                }
            }
        }
        let mut i = 0usize;
        let mut count = lib_ids.len();
        while i < count {
            let id = lib_ids.get_index(i).expect("index is valid");
            let lib = lib_map.get(id).ok_or(SemanticError::MissedApiLib(*id))?;
            lib_ids.extend(lib.libs.iter().copied());
            count = lib_ids.len();
            i += 1;
        }
        for id in lib_map.keys() {
            if !lib_ids.contains(id) {
                return Err(SemanticError::ExcessiveApiLib(*id));
            }
        }

        Ok(())
    }
}

/// API is an interface implementation.
///
/// API should work without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data. Basically, one may think of API as a compiled interface hierarchy applied to
/// a specific codex.
///
/// API does not commit to an interface, since it can match multiple interfaces in the interface
/// hierarchy.
#[derive(Getters, Clone, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = StrictHash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", bound = ""))]
pub struct Api {
    /// Commitment to the codex under which the API is valid.
    #[getter(as_copy)]
    pub codex_id: CodexId,

    /// Interface standards to which the API conforms.
    pub conforms: TinyOrdSet<u16>,

    /// Name for the default API call and owned state name.
    pub default_call: Option<CallState>,

    /// State API defines how a structured global contract state is constructed out of (and
    /// converted into) UltraSONIC immutable memory cells.
    pub global: TinyOrdMap<StateName, GlobalApi>,

    /// State API defines how a structured owned contract state is constructed out of (and converted
    /// into) UltraSONIC destructible memory cells.
    pub owned: TinyOrdMap<StateName, OwnedApi>,

    /// Readers have access to the converted global `state` and can construct a derived state out of
    /// it.
    ///
    /// The typical examples when readers are used are to sum individual asset issues and compute
    /// the number of totally issued assets.
    pub aggregators: TinyOrdMap<MethodName, Aggregator>,

    /// Links between named transaction methods defined in the interface - and corresponding
    /// verifier call ids defined by the contract.
    ///
    /// NB: Multiple methods from the interface may call the came verifier.
    pub verifiers: TinyOrdMap<MethodName, CallId>,

    /// Maps error type reported by a contract verifier via `EA` value to an error description taken
    /// from the interfaces.
    pub errors: TinyOrdMap<u256, TinyString>,
}

impl PartialEq for Api {
    fn eq(&self, other: &Self) -> bool { self.cmp(other) == Ordering::Equal }
}
impl Eq for Api {}
impl PartialOrd for Api {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for Api {
    fn cmp(&self, other: &Self) -> Ordering { self.api_id().cmp(&other.api_id()) }
}
impl Hash for Api {
    fn hash<H: Hasher>(&self, state: &mut H) { self.api_id().hash(state); }
}

impl Api {
    pub fn api_id(&self) -> StrictHash { self.commit_id() }

    pub fn verifier(&self, method: impl Into<MethodName>) -> Option<CallId> {
        self.verifiers.get(&method.into()).copied()
    }

    pub fn convert_global(
        &self,
        data: &StateData,
        sys: &TypeSystem,
    ) -> Result<Option<(StateName, StateAtom)>, StateConvertError> {
        // Here we do not yet know which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeding
        // convertor.
        for (name, api) in &self.global {
            if let Some(verified) = api.convertor.convert(api.sem_id, data.value, sys)? {
                let unverified =
                    if let Some(raw) = data.raw.as_ref() { Some(api.raw_convertor.convert(raw, sys)?) } else { None };
                return Ok(Some((name.clone(), StateAtom { verified, unverified })));
            }
        }
        // This means this state is unrelated to this API
        Ok(None)
    }

    pub fn convert_owned(
        &self,
        value: StateValue,
        sys: &TypeSystem,
    ) -> Result<Option<(StateName, StrictVal)>, StateConvertError> {
        // Here we do not yet know which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeding
        // convertor.
        for (name, api) in &self.owned {
            if let Some(atom) = api.convertor.convert(api.sem_id, value, sys)? {
                return Ok(Some((name.clone(), atom)));
            }
        }
        // This means this state is unrelated to this API
        Ok(None)
    }

    #[allow(clippy::result_large_err)]
    pub fn build_immutable(
        &self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        sys: &TypeSystem,
    ) -> Result<StateData, StateBuildError> {
        let name = name.into();
        let api = self
            .global
            .get(&name)
            .ok_or(StateBuildError::UnknownStateName(name))?;
        let value = api.builder.build(api.sem_id, data, sys)?;
        let raw = raw.map(|raw| api.raw_builder.build(raw, sys)).transpose()?;
        Ok(StateData { value, raw })
    }

    #[allow(clippy::result_large_err)]
    pub fn build_destructible(
        &self,
        name: impl Into<StateName>,
        data: StrictVal,
        sys: &TypeSystem,
    ) -> Result<StateValue, StateBuildError> {
        let name = name.into();
        let api = self
            .owned
            .get(&name)
            .ok_or(StateBuildError::UnknownStateName(name))?;

        api.builder.build(api.sem_id, data, sys)
    }

    #[allow(clippy::result_large_err)]
    pub fn build_witness(
        &self,
        name: impl Into<StateName>,
        data: StrictVal,
        sys: &TypeSystem,
    ) -> Result<StateValue, StateBuildError> {
        let name = name.into();
        let api = self
            .owned
            .get(&name)
            .ok_or(StateBuildError::UnknownStateName(name))?;

        api.witness_builder.build(api.witness_sem_id, data, sys)
    }

    pub fn calculate(&self, name: impl Into<StateName>) -> Result<StateCalc, StateUnknown> {
        let name = name.into();
        let api = self.owned.get(&name).ok_or(StateUnknown(name))?;

        Ok(api.arithmetics.calculator())
    }
}

/// API for global (immutable, or append-only) state.
///
/// API covers two main functions: taking structured data from the user input and _building_ a valid
/// state included in a new contract operation - and taking contract state and _converting_ it
/// into a user-friendly form, as a structured data (which may be lately used by _readers_
/// performing aggregation of state into a collection-type object).
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct GlobalApi {
    /// Semantic type id for verifiable part of the state.
    pub sem_id: SemId,

    /// Whether the state is a published state.
    pub published: bool,

    /// Procedure which converts a state made of finite field elements [`StateValue`] into a
    /// structured type [`StrictVal`].
    pub convertor: StateConvertor,

    /// Procedure which builds a state in the form of field elements [`StateValue`] out of a
    /// structured type [`StrictVal`].
    pub builder: StateBuilder,

    /// Procedure which converts a state made of raw bytes [`RawState`] into a structured type
    /// [`StrictVal`].
    pub raw_convertor: RawConvertor,

    /// Procedure which builds a state in the form of raw bytes [`RawState`] out of a structured
    /// type [`StrictVal`].
    pub raw_builder: RawBuilder,
}

/// API for owned (destrictible, or read-once) state.
///
/// API covers two main functions: taking structured data from the user input and _building_ a valid
/// state included in a new contract operation - and taking contract state and _converting_ it
/// into a user-friendly form, as structured data. It also allows constructing a state for witness,
/// allowing destroying previously defined state.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct OwnedApi {
    /// Semantic type id for the structured converted state data.
    pub sem_id: SemId,

    /// State arithmetics engine used in constructing new contract operations.
    pub arithmetics: StateArithm,

    /// Procedure which converts a state made of finite field elements [`StateValue`] into a
    /// structured type [`StrictVal`].
    pub convertor: StateConvertor,

    /// Procedure which builds a state in the form of field elements [`StateValue`] out of a
    /// structured type [`StrictVal`].
    pub builder: StateBuilder,

    /// Semantic type id for the witness data.
    pub witness_sem_id: SemId,

    /// Procedure which converts structured data in the form of [`StrictVal`] into a witness made of
    /// finite field elements in the form of [`StateValue`] for the destroyed previous state (an
    /// input of an operation).
    pub witness_builder: StateBuilder,
}

/// Error indicating that an API was asked to convert a state which is not known to it.
#[derive(Clone, Eq, PartialEq, Debug, Display, Error, From)]
#[display("unknown state name '{0}'")]
pub struct StateUnknown(pub StateName);

/// Errors happening if it is attempted to construct an invalid semantic object [`Semantics`] or
/// upgrade it inside a contract issuer or articles.
#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum SemanticError {
    /// contract id for the merged contract articles doesn't match.
    ContractMismatch,

    /// codex id for the merged articles doesn't match.
    CodexMismatch,

    /// articles contain duplicated API {0} under a different name.
    DuplicatedApi(StrictHash),

    /// library {0} is used by the contract codex verifiers but absent from the articles.
    MissedCodexLib(LibId),

    /// library {0} is present in the contract articles but not used in the codex verifiers.
    ExcessiveCodexLib(LibId),

    /// library {0} is used by the contract APIs but absent from the articles.
    MissedApiLib(LibId),

    /// library {0} is present in the contract articles but not used in the APIs.
    ExcessiveApiLib(LibId),

    /// invalid signature over the contract articles.
    InvalidSignature,
}
