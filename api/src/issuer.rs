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

use aluvm::{Lib, LibId};
use amplify::confinement::TinyString;
use commit_verify::{CommitId, StrictHash};
use sonic_callreq::MethodName;
use strict_encoding::TypeName;
use strict_types::TypeSystem;
use ultrasonic::{CallId, Codex, CodexId, Identity, LibRepo};

use crate::{Api, SemanticError, Semantics, SigBlob, Versioned, LIB_NAME_SONIC};

/// Articles id is a versioned variant for the contract id.
pub type IssuerId = Versioned<CodexId>;

/// An issuer contains information required for the creation of a contract and interaction with an
/// existing contract.
///
/// # Invariance
///
/// The structure provides the following invariance guarantees:
/// - all the API codex matches the codex under which the contract was issued;
/// - all the API ids are unique;
/// - all custom APIs have unique names.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode)]
// We must not derive or implement StrictDecode for Issuer, since we cannot validate signature
// inside it
#[strict_type(lib = LIB_NAME_SONIC)]
pub struct Issuer {
    /// Codex data.
    codex: Codex,
    /// A dedicated substructure [`Semantics`] keeping shared parts of both [`Issuer`] and
    /// [`Articles`].
    semantics: Semantics,
    /// Signature of a developer (`codex.developer`) over the [`IssuerId`] for a standalone issuer;
    /// and from a contract issuer (`issue.meta.issuer`) for an issuer instance within a contract.
    sig: Option<SigBlob>,
}

impl Issuer {
    /// Construct issuer from a codex and its semantics.
    pub fn new(codex: Codex, semantics: Semantics) -> Result<Self, SemanticError> {
        semantics.check(&codex)?;
        Ok(Self { semantics, codex, sig: None })
    }

    /// Construct issuer from a codex and signed semantics.
    pub fn with<E>(
        codex: Codex,
        semantics: Semantics,
        sig: SigBlob,
        sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
    ) -> Result<Self, SemanticError> {
        let mut me = Self::new(codex, semantics)?;
        let id = me.issuer_id().commit_id();
        sig_validator(id, &me.codex.developer, &sig).map_err(|_| SemanticError::InvalidSignature)?;
        me.sig = Some(sig);
        Ok(me)
    }

    pub fn dismember(self) -> (Codex, Semantics) { (self.codex, self.semantics) }

    /// Compute an issuer id, which includes information about the codex id, API version and
    /// checksum.
    pub fn issuer_id(&self) -> IssuerId {
        IssuerId {
            id: self.codex.codex_id(),
            version: self.semantics.version,
            checksum: self.semantics.apis_checksum(),
        }
    }
    /// Compute a codex id.
    pub fn codex_id(&self) -> CodexId { self.codex.codex_id() }
    /// Get a reference to the underlying codex.
    pub fn codex(&self) -> &Codex { &self.codex }
    /// Get the name of the underlying codex.
    pub fn codex_name(&self) -> &TinyString { &self.codex.name }

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
    pub fn libs(&self) -> impl Iterator<Item = &Lib> { self.semantics.libs.iter() }

    /// Detect whether the issuer is signed.
    pub fn is_signed(&self) -> bool { self.sig.is_some() }

    /// Get a [`CallId`] for a method from the default API.
    ///
    /// # Panics
    ///
    /// If the method name is not known.
    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        self.semantics
            .default
            .verifier(method)
            .expect("calling to method absent in Codex API")
    }
}

impl LibRepo for Issuer {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> {
        self.semantics
            .libs
            .iter()
            .find(|lib| lib.lib_id() == lib_id)
    }
}

#[cfg(feature = "binfile")]
mod _fs {
    use std::io::{self, Read};
    use std::path::Path;

    use amplify::confinement::U24 as U24MAX;
    use binfile::BinFile;
    use commit_verify::{CommitId, StrictHash};
    use strict_encoding::{DecodeError, DeserializeError, StreamReader, StreamWriter, StrictDecode, StrictEncode};
    use ultrasonic::{Codex, Identity};

    use crate::{Issuer, Semantics, SigBlob};

    /// The magic number used in storing issuer as a binary file.
    pub const ISSUER_MAGIC_NUMBER: u64 = u64::from_be_bytes(*b"ISSUER  ");
    /// The issuer encoding version used in storing issuer as a binary file.
    pub const ISSUER_VERSION: u16 = 0;

    impl Issuer {
        pub fn load<E>(
            path: impl AsRef<Path>,
            sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
        ) -> Result<Self, DeserializeError> {
            // We use a manual implementation since we can't validate signature inside StrictDecode
            // implementation for the Issuer
            let file = BinFile::<ISSUER_MAGIC_NUMBER, ISSUER_VERSION>::open(path)?;
            let mut reader = StreamReader::new::<U24MAX>(file);

            let codex = Codex::strict_read(&mut reader)?;
            let semantics = Semantics::strict_read(&mut reader)?;
            semantics
                .check(&codex)
                .map_err(|e| DecodeError::DataIntegrityError(e.to_string()))?;

            let sig = Option::<SigBlob>::strict_read(&mut reader)?;
            let me = Self { codex, semantics, sig };

            if let Some(sig) = &me.sig {
                sig_validator(me.issuer_id().commit_id(), &me.codex.developer, sig)
                    .map_err(|_| DecodeError::DataIntegrityError(s!("invalid signature")))?;
            }

            match reader.unconfine().read_exact(&mut [0u8; 1]) {
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(me),
                Err(e) => Err(e.into()),
                Ok(()) => Err(DeserializeError::DataNotEntirelyConsumed),
            }
        }

        pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
            let file = BinFile::<ISSUER_MAGIC_NUMBER, ISSUER_VERSION>::create_new(path)?;
            let writer = StreamWriter::new::<U24MAX>(file);
            self.strict_write(writer)
        }
    }
}
#[cfg(feature = "binfile")]
pub use _fs::*;
