// SONIC: Toolchain for formally-verifiable distributed contracts
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

use core::fmt::{self, Display, Formatter};

use baid64::base64::alphabet::Alphabet;
use baid64::base64::engine::{GeneralPurpose, GeneralPurposeConfig};
use baid64::base64::Engine;
use baid64::BAID64_ALPHABET;
use indexmap::IndexMap;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::CallRequest;

const LOCK: &str = "expiry";
const EXPIRY: &str = "expiry";
const ENDPOINTS: &str = "endpoints";
const ENDPOINT_SEP: char = ',';
const QUERY_ENCODE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b']')
    .add(b'&')
    .add(b'=');

/// Information parsed from a URL representation of SONIC contract call request [`CallRequest`].
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CallUri {
    pub request: CallRequest,
    pub unknown_query: IndexMap<String, String>,
}

impl CallUri {
    pub fn new(request: CallRequest) -> Self { Self { request, unknown_query: IndexMap::new() } }

    pub fn has_query(&self) -> bool {
        !self.unknown_query.is_empty() || self.request.expiry.is_some() || self.request.lock.is_some()
    }
}

impl Display for CallUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(contract_id) = &self.request.contract_id {
            write!(f, "{contract_id}")?;
        } else {
            f.write_str("contract:")?;
        }

        f.write_str("/")?;
        if let Some(api) = &self.request.api {
            write!(f, "{api}/")?;
        }
        if let Some(call) = &self.request.call {
            write!(f, "{}/", call.call_id)?;
            if let Some(state) = &call.destructible {
                write!(f, "{state}/")?;
            }
        }
        write!(f, "{}/{}", self.request.auth, utf8_percent_encode(&self.request.data.to_string(), QUERY_ENCODE))?;

        if self.has_query() {
            f.write_str("?")?;
        }

        if let Some(lock) = &self.request.lock {
            let alphabet = Alphabet::new(BAID64_ALPHABET).expect("invalid Baid64 alphabet");
            let engine = GeneralPurpose::new(&alphabet, GeneralPurposeConfig::new());
            write!(f, "{LOCK}={}", engine.encode(lock.to_vec()))?;
        }
        if let Some(expiry) = &self.request.expiry {
            write!(f, "{EXPIRY}={}", expiry.to_rfc3339())?;
        }
        if !self.request.endpoints.is_empty() {
            write!(f, "{ENDPOINTS}")?;
            let mut iter = self.request.endpoints.iter();
            while let Some(endpoint) = iter.next() {
                write!(f, "{}", utf8_percent_encode(&endpoint.to_string(), QUERY_ENCODE))?;
                if iter.by_ref().peekable().peek().is_some() {
                    write!(f, "{ENDPOINT_SEP}")?;
                }
            }
        }

        let mut iter = self.unknown_query.iter();
        while let Some((key, value)) = iter.next() {
            write!(f, "{}={}", utf8_percent_encode(key, QUERY_ENCODE), utf8_percent_encode(value, QUERY_ENCODE))?;
            if iter.by_ref().peekable().peek().is_some() {
                f.write_str("&")?;
            }
        }
        // TODO: Compute checksum and add as a fragment
        Ok(())
    }
}
