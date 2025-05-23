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

use aluvm::{Lib, LibId};
use amplify::confinement::{NonEmptyBlob, SmallOrdMap, SmallOrdSet};
use amplify::Bytes32;
use commit_verify::{CommitId, CommitmentId, DigestExt, Sha256};
use sonic_callreq::MethodName;
use strict_encoding::TypeName;
use strict_types::TypeSystem;
use ultrasonic::{CallId, Codex, CodexId, ContractId, Genesis, Identity, Issue, LibRepo, Opid};

use crate::{Api, ApiId, LIB_NAME_SONIC};

pub const ARTICLES_MAGIC_NUMBER: [u8; 8] = *b"ARTICLES";
pub const ARTICLES_VERSION: [u8; 2] = [0x00, 0x01];

#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = ArticlesId)]
pub struct ArticlesCommitment {
    pub contract_id: ContractId,
    pub default_api_id: ApiId,
    pub custom_api_ids: SmallOrdMap<TypeName, ApiId>,
}

/// A helper structure to store all API-related data.
///
/// A contract may have multiple APIs defined. All of them a summarized in this structure.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ApiDescriptor {
    pub default: Api,
    pub custom: SmallOrdMap<TypeName, Api>,
    pub libs: SmallOrdSet<Lib>,
    pub types: TypeSystem,
    /// Signature from the contract issuer (`issue.meta.issuer`) over the articles' id.
    pub sig: Option<SigBlob>,
}

impl ApiDescriptor {
    pub fn all(&self) -> impl Iterator<Item = &Api> { [&self.default].into_iter().chain(self.custom.values()) }
}

/// Articles contain the contract and all related codex and API information for interacting with it.
///
/// # Invariance
///
/// The structure provides the following invariance garantees:
/// - all the API codex matches the codex under which the contract was issued;
/// - all the API ids are unique;
/// - the only type of API adapter VM which can be used is [`crate::embedded::EmbeddedProc`] (see
///   [`crate::Api`] for more details).
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictDecode, StrictEncode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct Articles {
    apis: ApiDescriptor,
    issue: Issue,
}

impl Articles {
    fn articles_id(&self) -> ArticlesId {
        let custom_api_ids = SmallOrdMap::from_iter_checked(
            self.apis
                .custom
                .iter()
                .map(|(name, api)| (name.clone(), api.api_id())),
        );
        ArticlesCommitment {
            contract_id: self.contract_id(),
            default_api_id: self.apis.default.api_id(),
            custom_api_ids,
        }
        .commit_id()
    }

    pub fn contract_id(&self) -> ContractId { self.issue.contract_id() }
    pub fn codex_id(&self) -> CodexId { self.issue.codex_id() }
    pub fn genesis_opid(&self) -> Opid { self.issue.genesis_opid() }

    pub fn apis(&self) -> &ApiDescriptor { &self.apis }
    pub fn default_api(&self) -> &Api { &self.apis.default }
    pub fn custom_apis(&self) -> impl Iterator<Item = (&TypeName, &Api)> { self.apis.custom.iter() }
    pub fn types(&self) -> &TypeSystem { &self.apis.types }

    pub fn issue(&self) -> &Issue { &self.issue }
    pub fn codex(&self) -> &Codex { &self.issue.codex }
    pub fn genesis(&self) -> &Genesis { &self.issue.genesis }

    pub fn with(apis: ApiDescriptor, issue: Issue) -> Result<Self, ArticlesError> {
        let mut ids = bset![];
        for api in apis.all() {
            if api.codex_id != issue.codex_id() {
                return Err(ArticlesError::CodexMismatch);
            }
            let api_id = api.api_id();
            if !ids.insert(api_id) {
                return Err(ArticlesError::DuplicatedApi(api_id));
            }
        }

        Ok(Self { apis, issue })
    }

    pub fn merge(&mut self, other: Self, sig_validator: impl SigValidator) -> Result<bool, ArticlesError> {
        if self.contract_id() != other.contract_id() {
            return Err(ArticlesError::ContractMismatch);
        }

        let ts1 = self
            .apis
            .sig
            .as_ref()
            .and_then(|sig| {
                sig_validator
                    .validate_sig(self.articles_id().to_byte_array(), &self.issue.meta.issuer, sig)
                    .ok()
            })
            .unwrap_or_default();
        let Some(sig) = &other.apis.sig else { return Ok(false) };
        let ts2 = sig_validator
            .validate_sig(other.articles_id().to_byte_array(), &other.issue.meta.issuer, sig)
            .map_err(|_| ArticlesError::InvalidSignature)?;

        if ts2 > ts1 {
            self.apis = other.apis;
        }

        Ok(true)
    }

    /// Get a [`CallId`] for a method from the default API.
    ///
    /// # Panics
    ///
    /// If the method name is not known.
    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        let method = method.into();
        let name = method.to_string();
        self.apis
            .default
            .verifier(method)
            .unwrap_or_else(|| panic!("requesting a method `{name}` absent in the contract API"))
    }
}

impl LibRepo for Articles {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> { self.apis.libs.iter().find(|lib| lib.lib_id() == lib_id) }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, BorrowSlice, Hex, Index, RangeOps)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct ArticlesId(
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
pub enum ArticlesError {
    /// contract id for the merged contract articles doesn't match.
    ContractMismatch,

    /// codex id for the merged articles doesn't match.
    CodexMismatch,

    /// articles contain duplicated API {0} under a different name.
    DuplicatedApi(ApiId),

    /// invalid signature over the contract articles.
    InvalidSignature,
}
