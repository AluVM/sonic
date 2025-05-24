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

use std::io;

use aluvm::{Lib, LibId};
use amplify::confinement::SmallOrdSet;
use amplify::hex::ToHex;
use sonic_callreq::MethodName;
use strict_encoding::{DecodeError, ReadRaw, StrictDecode, StrictEncode, StrictReader, StrictWriter, WriteRaw};
use strict_types::TypeSystem;
use ultrasonic::{CallId, Codex, LibRepo};

use crate::{Api, SigBlob, LIB_NAME_SONIC};

pub const ISSUER_MAGIC_NUMBER: [u8; 8] = *b"COISSUER";
pub const ISSUER_VERSION: [u8; 2] = [0x00, 0x01];

/// An issuer contains information required for the creation of a contract.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Issuer {
    pub codex: Codex,
    /// Backward-compatible version number for the issuer.
    ///
    /// This version number is used to decide which contract APIs to apply if multiple
    /// contract APIs are available.
    pub version: u16,
    pub api: Api,
    pub libs: SmallOrdSet<Lib>,
    pub types: TypeSystem,
    /// Signature of a developer (`codex.developer`) over the [`IssuerId`].
    pub sig: Option<SigBlob>,
}

impl LibRepo for Issuer {
    fn get_lib(&self, lib_id: LibId) -> Option<&Lib> { self.libs.iter().find(|lib| lib.lib_id() == lib_id) }
}

impl Issuer {
    pub fn new(version: u16, codex: Codex, api: Api, libs: impl IntoIterator<Item = Lib>, types: TypeSystem) -> Self {
        Issuer {
            version,
            codex,
            api,
            libs: SmallOrdSet::from_iter_checked(libs),
            types,
            sig: none!(),
        }
    }

    /// Get a [`CallId`] for a method from the default API.
    ///
    /// # Panics
    ///
    /// If the method name is not known.
    pub fn call_id(&self, method: impl Into<MethodName>) -> CallId {
        self.api
            .verifier(method)
            .expect("calling to method absent in Codex API")
    }

    pub fn decode(reader: &mut StrictReader<impl ReadRaw>) -> Result<Self, DecodeError> {
        let magic_bytes = <[u8; 8]>::strict_decode(reader)?;
        if magic_bytes != ISSUER_MAGIC_NUMBER {
            return Err(DecodeError::DataIntegrityError(format!(
                "wrong contract issuer magic bytes {}",
                magic_bytes.to_hex()
            )));
        }
        let version = <[u8; 2]>::strict_decode(reader)?;
        if version != ISSUER_VERSION {
            return Err(DecodeError::DataIntegrityError(format!(
                "unsupported contract issuer version {}",
                u16::from_be_bytes(version)
            )));
        }
        Self::strict_decode(reader)
    }

    pub fn encode(&self, mut writer: StrictWriter<impl WriteRaw>) -> io::Result<()> {
        // This is compatible with BinFile
        writer = ISSUER_MAGIC_NUMBER.strict_encode(writer)?;
        // Version
        writer = ISSUER_VERSION.strict_encode(writer)?;
        self.strict_encode(writer)?;
        Ok(())
    }
}

#[cfg(feature = "binfile")]
mod _fs {
    use std::io::{self, Read};
    use std::path::Path;

    use amplify::confinement::U24 as U24MAX;
    use binfile::BinFile;
    use strict_encoding::{DeserializeError, StreamReader, StreamWriter, StrictReader, StrictWriter};

    use crate::Issuer;

    pub const ISSUER_MAGIC_NUMBER: u64 = u64::from_be_bytes(*b"COISSUER");
    pub const ISSUER_VERSION: u16 = 0;

    impl Issuer {
        pub fn load(path: impl AsRef<Path>) -> Result<Self, DeserializeError> {
            let file = BinFile::<ISSUER_MAGIC_NUMBER, ISSUER_VERSION>::open(path)?;
            let mut reader = StrictReader::with(StreamReader::new::<U24MAX>(file));
            let me = Self::decode(&mut reader)?;
            match reader.unbox().unconfine().read_exact(&mut [0u8; 1]) {
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(me),
                Err(e) => Err(e.into()),
                Ok(()) => Err(DeserializeError::DataNotEntirelyConsumed),
            }
        }

        pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
            let file = BinFile::<ISSUER_MAGIC_NUMBER, ISSUER_VERSION>::create_new(path)?;
            let writer = StrictWriter::with(StreamWriter::new::<U24MAX>(file));
            self.encode(writer)
        }
    }
}
