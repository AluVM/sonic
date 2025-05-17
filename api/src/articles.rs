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

use core::error::Error;
use std::io;

use aluvm::{Lib, LibId};
use amplify::confinement::{NonEmptyBlob, SmallOrdSet};
use amplify::hex::ToHex;
use amplify::Bytes32;
use commit_verify::{CommitId, CommitmentId, DigestExt, Sha256};
use sonic_callreq::MethodName;
use strict_encoding::{DecodeError, ReadRaw, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw};
use strict_types::TypeSystem;
use ultrasonic::{CallId, ContractId, Identity, Issue, LibRepo, Opid};

use crate::{Api, ApiId, LIB_NAME_SONIC};

pub const ARTICLES_MAGIC_NUMBER: [u8; 8] = *b"ARTICLES";
pub const ARTICLES_VERSION: [u8; 2] = [0x00, 0x01];

#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ArticlesId)]
struct ArticlesCommitment {
    pub contract_id: ContractId,
    pub default_api_id: ApiId,
    pub custom_api_ids: SmallOrdSet<ApiId>,
}

/// Articles contain the contract and all related codex and API information for interacting with it.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Articles {
    pub default_api: Api,
    pub custom_apis: SmallOrdSet<Api>,
    /// Signature from the contract issuer (`issue.meta.issuer`) over the articles id.
    pub sig: Option<SigBlob>,
    pub libs: SmallOrdSet<Lib>,
    pub types: TypeSystem,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub issue: Issue,
}

impl Articles {
    fn articles_id(&self) -> ArticlesId {
        let custom_api_ids = SmallOrdSet::from_iter_checked(self.custom_apis.iter().map(Api::api_id));
        ArticlesCommitment {
            contract_id: self.contract_id(),
            default_api_id: self.default_api.api_id(),
            custom_api_ids,
        }
        .commit_id()
    }

    pub fn contract_id(&self) -> ContractId { self.issue.contract_id() }

    pub fn genesis_opid(&self) -> Opid { self.issue.genesis_opid() }

    pub fn merge(&mut self, other: Self, sig_validator: impl SigValidator) -> Result<bool, MergeError> {
        if self.contract_id() != other.contract_id() {
            return Err(MergeError::ContractMismatch);
        }

        let ts1 = self
            .sig
            .as_ref()
            .and_then(|sig| {
                sig_validator
                    .validate_sig(self.articles_id().to_byte_array(), &self.issue.meta.issuer, sig)
                    .ok()
            })
            .unwrap_or_default();
        let Some(sig) = &other.sig else { return Ok(false) };
        let ts2 = sig_validator
            .validate_sig(other.articles_id().to_byte_array(), &other.issue.meta.issuer, sig)
            .map_err(|_| MergeError::InvalidSignature)?;

        if ts2 > ts1 {
            self.default_api = other.default_api;
            self.custom_apis = other.custom_apis;
            self.sig = other.sig;
            self.libs = other.libs;
            self.types = other.types;
        }

        Ok(true)
    }

    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        self.default_api
            .verifier(method)
            .expect("calling to method absent in Codex API")
    }

    pub fn decode(reader: &mut StrictReader<impl ReadRaw>) -> Result<Self, DecodeError> {
        let magic_bytes = <[u8; 8]>::strict_decode(reader)?;
        if magic_bytes != ARTICLES_MAGIC_NUMBER {
            return Err(DecodeError::DataIntegrityError(format!(
                "wrong contract articles magic bytes {}",
                magic_bytes.to_hex()
            )));
        }
        let version = <[u8; 2]>::strict_decode(reader)?;
        if version != ARTICLES_VERSION {
            return Err(DecodeError::DataIntegrityError(format!(
                "unsupported contract articles version {}",
                u16::from_be_bytes(version)
            )));
        }
        Self::strict_decode(reader)
    }

    pub fn encode(&self, mut writer: StrictWriter<impl WriteRaw>) -> io::Result<()> {
        // This is compatible with BinFile
        writer = ARTICLES_MAGIC_NUMBER.strict_encode(writer)?;
        // Version
        writer = ARTICLES_VERSION.strict_encode(writer)?;
        self.strict_encode(writer)?;
        Ok(())
    }
}

impl LibRepo for Articles {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> { self.libs.iter().find(|lib| lib.lib_id() == lib_id) }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, BorrowSlice, Hex, Index, RangeOps)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
struct ArticlesId(
    #[from]
    #[from([u8; 32])]
    Bytes32,
);

impl From<Sha256> for ArticlesId {
    fn from(hasher: Sha256) -> Self { hasher.finish().into() }
}

impl CommitmentId for ArticlesId {
    const TAG: &'static str = "urn:ubideco:sonic:articles#2025-05-18";
}

pub trait SigValidator {
    /// Validate the signature using the provided identity information.
    ///
    /// # Returns
    ///
    /// If successful, returns the timestamp of ths signature.
    fn validate_sig(&self, message: impl Into<[u8; 32]>, identity: &Identity, sig: &SigBlob)
        -> Result<u64, impl Error>;
}

#[derive(Wrapper, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, From, Display)]
#[wrapper(Deref, AsSlice, BorrowSlice, Hex)]
#[display(LowerHex)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct SigBlob(NonEmptyBlob<4096>);

impl Default for SigBlob {
    fn default() -> Self { SigBlob(NonEmptyBlob::with(0)) }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum MergeError {
    /// contract id for the merged contract articles doesn't match
    ContractMismatch,

    /// codex id for the merged schema doesn't match
    CodexMismatch,

    /// invalid signature over the contract articles.
    InvalidSignature,
}

#[cfg(feature = "std")]
mod _fs {
    use std::fs::File;
    use std::io::{self, Read};
    use std::path::Path;

    use amplify::confinement::U24 as U24MAX;
    use strict_encoding::{DeserializeError, StreamReader, StreamWriter, StrictReader, StrictWriter};

    use super::Articles;

    // TODO: Use BinFile
    impl Articles {
        pub fn load(path: impl AsRef<Path>) -> Result<Self, DeserializeError> {
            let file = File::open(path)?;
            let mut reader = StrictReader::with(StreamReader::new::<U24MAX>(file));
            let me = Self::decode(&mut reader)?;
            match reader.unbox().unconfine().read_exact(&mut [0u8; 1]) {
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(me),
                Err(e) => Err(e.into()),
                Ok(()) => Err(DeserializeError::DataNotEntirelyConsumed),
            }
        }

        pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
            let file = File::create(path)?;
            let writer = StrictWriter::with(StreamWriter::new::<U24MAX>(file));
            self.encode(writer)
        }
    }
}
