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

use amplify::confinement::{ConfinedBlob, TinyOrdMap, TinyString, U16 as U16MAX};
use amplify::num::u256;
use amplify::Bytes32;
use commit_verify::{CommitId, ReservedBytes};
use sonic_callreq::{CallState, MethodName, StateName};
use strict_types::{SemId, StrictDecode, StrictDumb, StrictEncode, StrictVal, TypeName, TypeSystem};
use ultrasonic::{CallId, CodexId, Identity, StateData, StateValue};

use crate::embedded::EmbeddedProc;
use crate::{StateAtom, VmType, LIB_NAME_SONIC};

pub(super) const USED_FIEL_BYTES: usize = u256::BYTES as usize - 2;
pub(super) const TOTAL_BYTES: usize = USED_FIEL_BYTES * 3;

/// API is an interface implementation.
///
/// API should work without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data. Basically, one may think of API as a compiled interface hierarchy applied to
/// a specific codex.
///
/// API does not commit to an interface, since it can match multiple interfaces in the interface
/// hierarchy.
pub type Api = ApiInner<EmbeddedProc>;

/// The inner details of API implementation, generic over the used VM for the adaptors.
///
/// # Nota bene
///
/// Currently, only a single adaptor VM is supported: embedded procedures. This support is
/// guaranteed through the fact that the only implementation for the API commitment id
/// ([`ApiInner::api_id`]) is made for the `ApiInner<EmbeddedProc>` variant.
/// There are two reasons for that:
/// 1. It is impossible to construct a contract articles object, since the constructor verifies the
///    API id.
/// 2. It is impossible to use API without having API id, singe no valid signature over the contract
///    articles using that API cannot be produced by a developer.
#[derive(Clone, Getters, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ApiId)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", bound = ""))]
pub struct ApiInner<Vm: ApiVm> {
    /// Version of the API structure.
    #[getter(as_copy)]
    pub version: ReservedBytes<1>,

    /// A commitment to a specific VM used.
    #[getter(as_copy)]
    pub adaptor: ReservedBytes<1>,

    /// Commitment to the codex under which the API is valid.
    #[getter(as_copy)]
    pub codex_id: CodexId,

    /// Timestamp, which is used for versioning (later APIs have priority over new ones).
    #[getter(as_copy)]
    pub timestamp: i64,

    /// Developer identity string.
    pub developer: Identity,

    /// Interface standard to which the API conforms.
    pub conforms: Option<TypeName>,

    /// Name for the default API call and destructible state name.
    pub default_call: Option<CallState>,

    /// Reserved for future use.
    #[getter(skip)]
    pub reserved: ReservedBytes<8>,

    /// State API defines how a structured contract state is constructed out of (and converted into)
    /// UltraSONIC immutable memory cells.
    pub append_only: TinyOrdMap<StateName, AppendApi<Vm>>,

    /// State API defines how a structured contract state is constructed out of (and converted into)
    /// UltraSONIC destructible memory cells.
    pub destructible: TinyOrdMap<StateName, DestructibleApi<Vm>>,

    /// Readers have access to the converted global `state` and can construct a derived state out of
    /// it.
    ///
    /// The typical examples when readers are used are to sum individual asset issues and compute
    /// the number of totally issued assets.
    pub readers: TinyOrdMap<MethodName, Vm::Reader>,

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

    pub fn verifier(&self, method: impl Into<MethodName>) -> Option<CallId> {
        self.verifiers.get(&method.into()).copied()
    }

    pub fn convert_immutable(&self, data: &StateData, sys: &TypeSystem) -> Option<(StateName, StateAtom)> {
        for (name, adaptor) in &self.append_only {
            if let Some(atom) = adaptor.convert(data, sys) {
                return Some((name.clone(), atom));
            }
        }
        None
    }

    pub fn convert_destructible(&self, value: StateValue, sys: &TypeSystem) -> Option<(StateName, StrictVal)> {
        // Here we do not yet known which state we are using, since it is encoded inside the field element
        // of `StateValue`. Thus, we are trying all available convertors until they succeed, since the
        // convertors check the state type. Then, we use the state name associated with the succeeded
        // convertor.
        for (name, adaptor) in &self.destructible {
            if let Some(atom) = adaptor.convert(value, sys) {
                return Some((name.clone(), atom));
            }
        }
        None
    }

    pub fn build_immutable(
        &self,
        name: impl Into<StateName>,
        data: StrictVal,
        raw: Option<StrictVal>,
        sys: &TypeSystem,
    ) -> StateData {
        let name = name.into();
        self.append_only
            .get(&name)
            .expect("state name is unknown for the API")
            .build(data, raw, sys)
    }

    pub fn build_destructible(&self, name: impl Into<StateName>, data: StrictVal, sys: &TypeSystem) -> StateValue {
        let name = name.into();
        self.destructible
            .get(&name)
            .expect("state name is unknown for the API")
            .build(data, sys)
    }

    pub fn calculate(&self, name: impl Into<StateName>) -> Box<dyn StateCalc> {
        let name = name.into();
        self.destructible
            .get(&name)
            .expect("state name is unknown for the API")
            .arithmetics
            .calculator()
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

/// API for append-only state.
///
/// API covers two main functions: taking structured data from the user input and _building_ a valid
/// state included into a new contract operation - and taking contract state and _converting_ it
/// into a user-friendly form, as a structured data (which may be lately used by _readers_
/// performing aggregation of state into a collection-type objects).
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct AppendApi<Vm: ApiVm> {
    /// Semantic type id for verifiable part of the state.
    pub sem_id: SemId,
    /// Semantic type id for non-verifiable part of the state.
    pub raw_sem_id: SemId,

    pub published: bool,
    /// Procedures which convert a state made of finite field elements [`StateData`] into a
    /// structured type [`StructData`] and vice verse.
    pub adaptor: Vm::Adaptor,
}

impl<Vm: ApiVm> AppendApi<Vm> {
    pub fn convert(&self, data: &StateData, sys: &TypeSystem) -> Option<StateAtom> {
        self.adaptor
            .convert_immutable(self.sem_id, self.raw_sem_id, data, sys)
    }

    /// Build an immutable memory cell out of structured state.
    ///
    /// Since append-only state includes both field elements (verifiable part of the state) and
    /// optional structured data (non-verifiable, non-compressible part of the state) it takes
    /// two inputs of a structured state data, leaving the raw part unchanged.
    pub fn build(&self, value: StrictVal, raw: Option<StrictVal>, sys: &TypeSystem) -> StateData {
        let raw = raw.map(|raw| {
            let typed = sys
                .typify(raw, self.raw_sem_id)
                .expect("invalid strict value not matching semantic type information");
            sys.strict_serialize_value::<U16MAX>(&typed)
                .expect("strict value is too large")
                .into()
        });
        let value = self.adaptor.build_state(self.sem_id, value, sys);
        StateData { value, raw }
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct DestructibleApi<Vm: ApiVm> {
    pub sem_id: SemId,

    /// State arithmetics engine used in constructing new contract operations.
    pub arithmetics: Vm::Arithm,

    /// Procedures which convert a state made of finite field elements [`StateData`] into a
    /// structured type [`StructData`] and vice verse.
    pub adaptor: Vm::Adaptor,
}

impl<Vm: ApiVm> DestructibleApi<Vm> {
    pub fn convert(&self, value: StateValue, sys: &TypeSystem) -> Option<StrictVal> {
        self.adaptor.convert_destructible(self.sem_id, value, sys)
    }
    pub fn build(&self, value: StrictVal, sys: &TypeSystem) -> StateValue {
        self.adaptor.build_state(self.sem_id, value, sys)
    }
    pub fn arithmetics(&self) -> &Vm::Arithm { &self.arithmetics }
}

#[cfg(not(feature = "serde"))]
trait Serde {}
#[cfg(not(feature = "serde"))]
impl<T> Serde for T {}

#[cfg(feature = "serde")]
trait Serde: serde::Serialize + for<'de> serde::Deserialize<'de> {}
#[cfg(feature = "serde")]
impl<T> Serde for T where T: serde::Serialize + for<'de> serde::Deserialize<'de> {}

pub trait ApiVm {
    type Arithm: StateArithm;
    type Reader: StateReader;
    type Adaptor: StateAdaptor;

    fn vm_type(&self) -> VmType;
}

/// Reader constructs a composite state out of distinct values of all appendable state elements of
/// the same type.
#[allow(private_bounds)]
pub trait StateReader: Clone + Ord + Debug + StrictDumb + StrictEncode + StrictDecode + Serde {
    fn read<'s, I: IntoIterator<Item = &'s StateAtom>>(&self, state: impl Fn(&StateName) -> I) -> StrictVal;
}

/// Adaptors convert field elements into structured data and vise verse.
#[allow(private_bounds)]
pub trait StateAdaptor: Clone + Ord + Debug + StrictDumb + StrictEncode + StrictDecode + Serde {
    fn convert_immutable(
        &self,
        sem_id: SemId,
        raw_sem_id: SemId,
        data: &StateData,
        sys: &TypeSystem,
    ) -> Option<StateAtom>;
    fn convert_destructible(&self, sem_id: SemId, value: StateValue, sys: &TypeSystem) -> Option<StrictVal>;

    fn build_immutable(&self, value: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue;
    fn build_destructible(&self, value: ConfinedBlob<0, TOTAL_BYTES>) -> StateValue;

    fn build_state(&self, sem_id: SemId, value: StrictVal, sys: &TypeSystem) -> StateValue {
        let typed = sys
            .typify(value, sem_id)
            .expect("invalid strict value not matching semantic type information");
        let ser = sys
            .strict_serialize_value::<TOTAL_BYTES>(&typed)
            .expect("strict value is too large");
        self.build_immutable(ser)
    }
}

#[allow(private_bounds)]
pub trait StateArithm: Clone + Debug + StrictDumb + StrictEncode + StrictDecode + Serde {
    /// Calculator allows to perform calculations on the state (ordering and sorting, coin
    /// selection, change calculation).
    fn calculator(&self) -> Box<dyn StateCalc>;
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display, Error)]
#[display(doc_comments)]
pub enum StateCalcError {
    /// integer overflow during state computation.
    Overflow,

    /// state can't be computed.
    UncountableState,
}

pub trait StateCalc {
    /// Procedure which is called on [`StateCalc`] to accumulate an input state.
    fn accumulate(&mut self, state: &StrictVal) -> Result<(), StateCalcError>;

    /// Procedure which is called on [`StateCalc`] to lessen an output state.
    fn lessen(&mut self, state: &StrictVal) -> Result<(), StateCalcError>;

    /// Procedure which is called on [`StateCalc`] to compute the difference between an input
    /// state and output state.
    fn diff(&self) -> Result<Vec<StrictVal>, StateCalcError>;

    /// Detect whether the supplied state is enough to satisfy some target requirements.
    fn is_satisfied(&self, state: &StrictVal) -> bool;
}

impl StateCalc for Box<dyn StateCalc> {
    fn accumulate(&mut self, state: &StrictVal) -> Result<(), StateCalcError> { self.as_mut().accumulate(state) }

    fn lessen(&mut self, state: &StrictVal) -> Result<(), StateCalcError> { self.as_mut().lessen(state) }

    fn diff(&self) -> Result<Vec<StrictVal>, StateCalcError> { self.as_ref().diff() }

    fn is_satisfied(&self, state: &StrictVal) -> bool { self.as_ref().is_satisfied(state) }
}
