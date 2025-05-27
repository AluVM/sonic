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

#![allow(unused_braces)]

use core::fmt;
use core::fmt::{Display, Formatter};
use core::str::FromStr;

use aluvm::{Lib, LibId};
use amplify::confinement::NonEmptyBlob;
use amplify::Wrapper;
use baid64::DisplayBaid64;
use commit_verify::{CommitEncode, CommitId, StrictHash};
use sonic_callreq::MethodName;
use strict_encoding::TypeName;
use strict_types::TypeSystem;
use ultrasonic::{
    CallId, Codex, CodexId, ContractId, ContractMeta, ContractName, Genesis, Identity, Issue, LibRepo, Opid,
};

use crate::{Api, ApisChecksum, ParseVersionedError, SemanticError, Semantics, LIB_NAME_SONIC};

/// Articles id is a versioned variant for the contract id, which includes information about a
/// specific API version.
///
/// Contracts may have multiple API implementations, which may be versioned.
/// Articles include a specific version of the contract APIs.
/// This structure provides the necessary information for the user about a specific API version
/// known and used by a system, so a user may avoid confusion when an API change due to upgrade
/// happens.
///
/// # See also
///
/// - [`ContractId`]
/// - [`crate::IssuerId`]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[derive(CommitEncode)]
#[commit_encode(strategy = strict, id = StrictHash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ArticlesId {
    /// An identifier of the contract.
    pub contract_id: ContractId,
    /// Version number of the API.
    pub version: u16,
    /// A checksum for the APIs from the Semantics structure.
    pub checksum: ApisChecksum,
}

impl Display for ArticlesId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}#", self.contract_id, self.version)?;
        self.checksum.fmt_baid64(f)
    }
}

impl FromStr for ArticlesId {
    type Err = ParseVersionedError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (id, remnant) = s
            .split_once('/')
            .ok_or_else(|| ParseVersionedError::NoVersion(s.to_string()))?;
        let (version, api_id) = remnant
            .split_once('#')
            .ok_or_else(|| ParseVersionedError::NoChecksum(s.to_string()))?;
        Ok(Self {
            contract_id: id.parse().map_err(ParseVersionedError::Id)?,
            version: version.parse().map_err(ParseVersionedError::Version)?,
            checksum: api_id.parse().map_err(ParseVersionedError::Checksum)?,
        })
    }
}

/// Articles contain the contract and all related codex and API information for interacting with it.
///
/// # Invariance
///
/// The structure provides the following invariance guarantees:
/// - all the API codex matches the codex under which the contract was issued;
/// - all the API ids are unique;
/// - all custom APIs have unique names;
/// - the signature, if present, is a valid sig over the [`ArticlesId`].
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode)]
// We must not derive or implement StrictDecode for Issuer, since we cannot validate signature
// inside it
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct Articles {
    /// We can't use [`Issuer`] here since we will duplicate the codex between it and the [`Issue`].
    /// Thus, a dedicated substructure [`Semantics`] is introduced, which keeps a shared part of
    /// both [`Issuer`] and [`Articles`].
    semantics: Semantics,
    /// Signature from the contract issuer (`issue.meta.issuer`) over the articles' id.
    ///
    /// NB: it must precede the issue, which contains genesis!
    /// Since genesis is read with a stream-supporting procedure later.
    sig: Option<SigBlob>,
    /// The contract issue.
    issue: Issue,
}

impl Articles {
    /// Construct articles from a signed contract semantic and the contract issue under that
    /// semantics.
    pub fn with<E>(
        semantics: Semantics,
        issue: Issue,
        sig: Option<SigBlob>,
        sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
    ) -> Result<Self, SemanticError> {
        semantics.check(&issue.codex)?;
        let mut me = Self { semantics, issue, sig: None };
        let id = me.articles_id().commit_id();
        if let Some(sig) = &sig {
            sig_validator(id, &me.issue.meta.issuer, sig).map_err(|_| SemanticError::InvalidSignature)?;
        }
        me.sig = sig;
        Ok(me)
    }

    /// Compute an article id, which includes information about the contract id, API version and
    /// checksum.
    pub fn articles_id(&self) -> ArticlesId {
        ArticlesId {
            contract_id: self.issue.contract_id(),
            version: self.semantics.version,
            checksum: self.semantics.apis_checksum(),
        }
    }
    /// Compute a contract id.
    pub fn contract_id(&self) -> ContractId { self.issue.contract_id() }
    /// Compute a codex id.
    pub fn codex_id(&self) -> CodexId { self.issue.codex_id() }
    /// Compute a genesis opid.
    pub fn genesis_opid(&self) -> Opid { self.issue.genesis_opid() }

    /// Get a reference to the contract semantic.
    pub fn semantics(&self) -> &Semantics { &self.semantics }
    /// Get a reference to the default API.
    pub fn default_api(&self) -> &Api { &self.semantics.default }
    /// Get an iterator over the custom APIs.
    pub fn custom_apis(&self) -> impl Iterator<Item = (&TypeName, &Api)> { self.semantics.custom.iter() }
    /// Get a reference to the type system.
    pub fn types(&self) -> &TypeSystem { &self.semantics.types }
    /// Iterates over all APIs, including the default and the named ones.
    pub fn apis(&self) -> impl Iterator<Item = &Api> { self.semantics.apis() }
    /// Iterates over all codex libraries.
    pub fn codex_libs(&self) -> impl Iterator<Item = &Lib> { self.semantics.codex_libs.iter() }

    /// Get a reference to the contract issue information.
    pub fn issue(&self) -> &Issue { &self.issue }
    /// Get a reference to the contract codex.
    pub fn codex(&self) -> &Codex { &self.issue.codex }
    /// Get a reference to the contract genesis.
    pub fn genesis(&self) -> &Genesis { &self.issue.genesis }
    /// Get a reference to the contract meta-information.
    pub fn contract_meta(&self) -> &ContractMeta { &self.issue.meta }
    /// Get a reference to the contract name.
    pub fn contract_name(&self) -> &ContractName { &self.issue.meta.name }

    /// Get a reference to a signature over the contract semantics.
    pub fn sig(&self) -> &Option<SigBlob> { &self.sig }
    /// Detect whether the articles are signed.
    pub fn is_signed(&self) -> bool { self.sig.is_some() }

    /// Upgrades contract APIs if a newer version is available.
    ///
    /// # Returns
    ///
    /// Whether the upgrade has happened, i.e. `other` represents a valid later version of the APIs.
    pub fn upgrade_apis(&mut self, other: Self) -> Result<bool, SemanticError> {
        if self.contract_id() != other.contract_id() {
            return Err(SemanticError::ContractMismatch);
        }

        Ok(match (&self.sig, &other.sig) {
            (None, None) | (Some(_), Some(_)) if other.semantics.version > self.semantics.version => {
                self.semantics = other.semantics;
                true
            }
            (None, Some(_)) => {
                self.semantics = other.semantics;
                true
            }
            _ => false, // No upgrade
        })
    }

    /// Get a [`CallId`] for a method from the default API.
    ///
    /// # Panics
    ///
    /// If the method name is not known.
    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        let method = method.into();
        let name = method.to_string();
        self.semantics
            .default
            .verifier(method)
            .unwrap_or_else(|| panic!("requesting a method `{name}` absent in the contract API"))
    }
}

impl LibRepo for Articles {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> {
        self.semantics
            .codex_libs
            .iter()
            .find(|lib| lib.lib_id() == lib_id)
    }
}

/// A signature blob.
///
/// Helps to abstract from a specific signing algorithm.
#[derive(Wrapper, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, From, Display)]
#[wrapper(Deref, AsSlice, BorrowSlice, Hex)]
#[display(LowerHex)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, dumb = { Self(NonEmptyBlob::with(0)) })]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct SigBlob(NonEmptyBlob<4096>);
