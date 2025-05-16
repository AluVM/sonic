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

use amplify::hex::ToHex;
use strict_encoding::{
    DecodeError, ReadRaw, StrictDecode, StrictEncode, StrictReader, StrictWriter, TypeName, WriteRaw,
};
use ultrasonic::{ContractId, Issue, Opid};

use crate::sigs::ContentSigs;
use crate::{Api, Schema, LIB_NAME_SONIC};

pub const ARTICLES_MAGIC_NUMBER: [u8; 8] = *b"ARTICLES";
pub const ARTICLES_VERSION: [u8; 2] = [0x00, 0x01];

/// Articles contain the contract and all related codex and API information for interacting with it.
#[derive(Clone, Eq, PartialEq, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct Articles {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub schema: Schema,
    pub contract_sigs: ContentSigs,
    pub issue: Issue,
}

impl Articles {
    pub fn contract_id(&self) -> ContractId { self.issue.contract_id() }

    pub fn genesis_opid(&self) -> Opid { self.issue.genesis_opid() }

    pub fn api(&self, name: &TypeName) -> &Api { self.schema.api(name) }

    pub fn merge(&mut self, other: Self) -> Result<bool, MergeError> {
        if self.contract_id() != other.contract_id() {
            return Err(MergeError::ContractMismatch);
        }

        self.schema.merge(other.schema)?;
        self.contract_sigs.merge(other.contract_sigs);

        Ok(true)
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

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum MergeError {
    /// contract id for the merged contract articles doesn't match
    ContractMismatch,

    /// codex id for the merged schema doesn't match
    CodexMismatch,
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
