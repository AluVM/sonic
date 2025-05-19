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

//! API defines how a contract can be interfaced by software.
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
use core::fmt::Debug;
use core::hash::{Hash, Hasher};

use amplify::confinement::{TinyOrdMap, TinyString};
use amplify::num::u256;
use amplify::Bytes32;
use commit_verify::{CommitId, ReservedBytes};
use sonic_callreq::{CallState, MethodName, StateName};
use strict_types::{SemId, StrictDecode, StrictDumb, StrictEncode, StrictVal, TypeName, TypeSystem};
use ultrasonic::{CallId, CodexId, Identity, StateData, StateValue};

use crate::{
    RawBuilder, RawConvertor, StateAggregator, StateArithm, StateAtom, StateBuildError, StateBuilder, StateCalc,
    StateConvertError, StateConvertor, LIB_NAME_SONIC,
};

/// API is an interface implementation.
///
/// API should work without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data. Basically, one may think of API as a compiled interface hierarchy applied to
/// a specific codex.
///
/// API does not commit to an interface, since it can match multiple interfaces in the interface
/// hierarchy.
#[derive(Clone, Getters, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ApiId)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", bound = ""))]
pub struct Api {
    /// Version of the API structure.
    #[getter(as_copy)]
    pub version: ReservedBytes<1>,

    /// Commitment to the codex under which the API is valid.
    #[getter(as_copy)]
    pub codex_id: CodexId,

    /// Developer identity string.
    pub developer: Identity,

    /// Interface standard to which the API conforms.
    pub conforms: Option<TypeName>,

    /// Name for the default API call and destructible state name.
    pub default_call: Option<CallState>,

    /// State API defines how a structured contract state is constructed out of (and converted into)
    /// UltraSONIC immutable memory cells.
    pub immutable: TinyOrdMap<StateName, ImmutableApi>,

    /// State API defines how a structured contract state is constructed out of (and converted into)
    /// UltraSONIC destructible memory cells.
    pub destructible: TinyOrdMap<StateName, DestructibleApi>,

    /// Readers have access to the converted global `state` and can construct a derived state out of
    /// it.
    ///
    /// The typical examples when readers are used are to sum individual asset issues and compute
    /// the number of totally issued assets.
    pub aggregators: TinyOrdMap<MethodName, StateAggregator>,

    /// Links between named transaction methods defined in the interface - and corresponding
    /// verifier call ids defined by the contract.
    ///
    /// NB: Multiple methods from the interface may call the came verifier.
    pub verifiers: TinyOrdMap<MethodName, CallId>,

    /// Maps error type reported by a contract verifier via `EA` value to an error description taken
    /// from the interfaces.
    pub errors: TinyOrdMap<u256, TinyString>,

    /// Reserved for future use.
    #[getter(skip)]
    pub reserved: ReservedBytes<8>,
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
    pub fn api_id(&self) -> ApiId { self.commit_id() }

    pub fn verifier(&self, method: impl Into<MethodName>) -> Option<CallId> {
        self.verifiers.get(&method.into()).copied()
    }

    pub fn convert_immutable(
        &self,
        data: &StateData,
        sys: &TypeSystem,
    ) -> Result<Option<(StateName, StateAtom)>, StateConvertError> {
        // Here we do not yet know which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeding
        // convertor.
        for (name, api) in &self.immutable {
            if let Some(verified) = api.convertor.convert(api.sem_id, data.value, sys)? {
                let unverified =
                    if let Some(raw) = data.raw.as_ref() { Some(api.raw_convertor.convert(raw, sys)?) } else { None };
                return Ok(Some((name.clone(), StateAtom { verified, unverified })));
            }
        }
        // This means this state is unrelated to this API
        Ok(None)
    }

    pub fn convert_destructible(
        &self,
        value: StateValue,
        sys: &TypeSystem,
    ) -> Result<Option<(StateName, StrictVal)>, StateConvertError> {
        // Here we do not yet know which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeding
        // convertor.
        for (name, api) in &self.destructible {
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
            .immutable
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
            .destructible
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
            .destructible
            .get(&name)
            .ok_or(StateBuildError::UnknownStateName(name))?;

        api.witness_builder.build(api.witness_sem_id, data, sys)
    }

    pub fn calculate(&self, name: impl Into<StateName>) -> Result<StateCalc, StateUnknown> {
        let name = name.into();
        let api = self.destructible.get(&name).ok_or(StateUnknown(name))?;

        Ok(api.arithmetics.calculator())
    }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, BorrowSlice, Hex, Index, RangeOps)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct ApiId(
    #[from]
    #[from([u8; 32])]
    Bytes32,
);

mod _baid4 {
    use core::fmt::{self, Display, Formatter};
    use core::str::FromStr;

    use amplify::ByteArray;
    use baid64::{Baid64ParseError, DisplayBaid64, FromBaid64Str};
    use commit_verify::{CommitmentId, DigestExt, Sha256};

    use super::*;

    impl DisplayBaid64 for ApiId {
        const HRI: &'static str = "api";
        const CHUNKING: bool = true;
        const PREFIX: bool = false;
        const EMBED_CHECKSUM: bool = false;
        const MNEMONIC: bool = true;
        fn to_baid64_payload(&self) -> [u8; 32] { self.to_byte_array() }
    }
    impl FromBaid64Str for ApiId {}
    impl FromStr for ApiId {
        type Err = Baid64ParseError;
        fn from_str(s: &str) -> Result<Self, Self::Err> { Self::from_baid64_str(s) }
    }
    impl Display for ApiId {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { self.fmt_baid64(f) }
    }

    impl From<Sha256> for ApiId {
        fn from(hasher: Sha256) -> Self { hasher.finish().into() }
    }

    impl CommitmentId for ApiId {
        const TAG: &'static str = "urn:ubideco:sonic:api#2024-11-20";
    }

    #[cfg(feature = "serde")]
    ultrasonic::impl_serde_str_bin_wrapper!(ApiId, Bytes32);
}

/// API for immutable (append-only) state.
///
/// API covers two main functions: taking structured data from the user input and _building_ a valid
/// state included in a new contract operation - and taking contract state and _converting_ it
/// into a user-friendly form, as a structured data (which may be lately used by _readers_
/// performing aggregation of state into a collection-type object).
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ImmutableApi {
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

/// API for destructible (read-once) state.
///
/// API covers two main functions: taking structured data from the user input and _building_ a valid
/// state included in a new contract operation - and taking contract state and _converting_ it
/// into a user-friendly form, as structured data. It also allows constructing a state for witness,
/// allowing destroying previously defined state.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct DestructibleApi {
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
