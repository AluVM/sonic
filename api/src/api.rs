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

//! API defines how a contract can be interfaced by a software.
//!
//! SONARE provides four types of actions for working with contract (ROVT):
//! 1. _Read_ the state of the contract;
//! 2. _Operate_: construct new operations performing contract state transitions;
//! 3. _Verify_ an existing operation under the contract Codex and generate transaction;
//! 4. _Transact_: apply or roll-back transactions to the contract state.
//!
//! API defines methods for human-based interaction with the contract for read and operate actions.
//! The verify part is implemented in the consensus layer (UltraSONIC), the transact part is
//! performed directly, so these two are not covered by an API.

use core::cmp::Ordering;
use core::fmt::Debug;
use core::hash::{Hash, Hasher};

use amplify::confinement::{TinyOrdMap, TinyString};
use amplify::Bytes32;
use commit_verify::{CommitId, ReservedBytes};
use serde::Serialize;
use strict_types::{SemId, StrictDecode, StrictDumb, StrictEncode, TypeName, VariantName};
use ultrasonic::{CallId, CodexId, Identity};

use crate::embedded::EmbeddedProc;
use crate::{StructData, VmType, LIB_NAME_SONIC};

pub type StateName = VariantName;
pub type MethodName = VariantName;

#[derive(Clone, Debug, From)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ApiId)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Embedded(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum Api {
    #[from]
    #[strict_type(tag = 1)]
    Embedded(ApiInner<EmbeddedProc>),

    #[from]
    #[strict_type(tag = 2)]
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
    fn cmp(&self, other: &Self) -> Ordering { self.api_id().cmp(&other.api_id()) }
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
}

/// API is an interface implementation.
///
/// API should work without requiring runtime to have corresponding interfaces; it should provide
/// all necessary data. Basically one may think of API as a compiled interface hierarchy applied to
/// a specific codex.
///
/// API doesn't commit to an interface ID, since it can match multiple interfaces in the interface
/// hierarchy.
#[derive(Clone, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", bound = ""))]
pub struct ApiInner<Vm: ApiVm> {
    /// Version of the API structure.
    pub version: ReservedBytes<2>,

    /// Commitment to the codex under which the API is valid.
    pub codex_id: CodexId,

    /// Incremental version number for the API.
    pub api_version: u16,

    /// API name. Each codex must have a default API with no name.
    pub name: Option<TypeName>,

    /// Developer identity string.
    pub developer: Identity,

    /// State API defines how structured contract state is constructed out of (and converted into)
    /// UltraSONIC immutable memory cells.
    pub append_only: TinyOrdMap<StateName, AppendApi<Vm>>,

    /// State API defines how structured contract state is constructed out of (and converted into)
    /// UltraSONIC destructible memory cells.
    pub destructible: TinyOrdMap<StateName, DestructibleApi<Vm>>,

    /// Readers have access to the converted `state` and can construct a derived state out of it.
    ///
    /// The typical examples when readers are used is to sum individual asset issues and compute the
    /// number of totally issued assets.
    pub readers: TinyOrdMap<MethodName, Vm::ReaderSite>,

    /// Links between named transaction methods defined in the interface - and corresponding
    /// verifier call ids defined by the contract.
    ///
    /// NB: Multiple methods from the interface may call to the came verifier.
    pub verifiers: TinyOrdMap<MethodName, CallId>,

    /// Maps error type reported by a contract verifier via `EA` value to an error description taken
    /// from the interfaces.
    pub errors: TinyOrdMap<u128, TinyString>,
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
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom, dumb = Self::Single(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum CollectionType {
    #[strict_type(tag = 1)]
    Single(SemId),

    #[strict_type(tag = 0x10)]
    List(SemId),

    #[strict_type(tag = 0x11)]
    Set(SemId),

    #[strict_type(tag = 0x20)]
    Map { key: SemId, val: SemId },
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct AppendApi<Vm: ApiVm> {
    pub published: bool,

    pub collection: CollectionType,

    /// Procedures which convert a state made of finite field elements [`StateData`] into a
    /// structured type [`StructData`].
    pub adaptor: Vm::AdaptorSite,

    /// Procedures which convert structured type [`StructData`] into a state made of finite field
    /// elements [`StateData`].
    pub builder: Vm::AdaptorSite,
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
    /// structured type [`StructData`].
    pub adaptor: Vm::AdaptorSite,

    /// Procedures which convert structured type [`StructData`] into a state made of finite field
    /// elements [`StateData`].
    pub builder: Vm::AdaptorSite,
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

    type ReaderSite: Clone + Ord + Debug + StrictDumb + StrictEncode + StrictDecode + Serde;
    type AdaptorSite: Clone + Ord + Debug + StrictDumb + StrictEncode + StrictDecode + Serde;

    fn vm_type(&self) -> VmType;
}

// TODO: Use Result's instead of Option
pub trait StateArithm: Clone + Debug + StrictDumb + StrictEncode + StrictDecode + Serde {
    /// Procedure which converts [`StructData`] corresponding to this type into a weight in range
    /// `0..256` representing how much this specific state fulfills certain state requirement.
    ///
    /// This is used in selecting state required to fulfill input for a provided contract
    /// [`Request`].
    fn measure(&self, state: StructData) -> Option<u8>;

    /// Procedure which is called on [`StateArithm`] to accumulate an input state.
    fn accumulate(&mut self, state: StructData) -> Option<()>;

    /// Procedure which is called on [`StateArithm`] to lessen an output state.
    fn lessen(&mut self, state: StructData) -> Option<()>;

    /// Procedure which is called on [`StateArithm`] to compute the difference between an input
    /// state and output state.
    fn diff(&self) -> Option<StructData>;
}
