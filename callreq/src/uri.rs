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

use alloc::collections::VecDeque;
use core::error::Error;
use core::fmt::{self, Display, Formatter};
use core::str::FromStr;

use amplify::confinement::{ConfinedVec, TinyBlob};
use baid64::base64::alphabet::Alphabet;
use baid64::base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use baid64::base64::{DecodeError, Engine};
use baid64::BAID64_ALPHABET;
use chrono::{DateTime, Utc};
use fluent_uri::error::ParseError;
use fluent_uri::Uri;
use indexmap::IndexMap;
use percent_encoding::{percent_decode, utf8_percent_encode, AsciiSet, CONTROLS};
use strict_types::{InvalidRString, StrictVal};

use crate::{CallRequest, CallState, Endpoint};

const URI_SCHEME: &str = "contract";
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

impl<T, A> CallRequest<T, A> {
    pub fn has_query(&self) -> bool { !self.unknown_query.is_empty() || self.expiry.is_some() || self.lock.is_some() }
}

impl<T, A> Display for CallRequest<T, A>
where
    T: Display,
    A: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}/", &self.scope)?;
        f.write_str("/")?;
        if let Some(api) = &self.api {
            write!(f, "{api}/")?;
        }
        if let Some(call) = &self.call {
            write!(f, "{}/", call.method)?;
            if let Some(state) = &call.destructible {
                write!(f, "{state}/")?;
            }
        }

        if let Some(data) = &self.data {
            write!(f, "{}@", utf8_percent_encode(&data.to_string(), QUERY_ENCODE))?;
        }
        write!(f, "{}/", self.auth)?;

        if self.has_query() {
            f.write_str("?")?;
        }

        if let Some(lock) = &self.lock {
            let alphabet = Alphabet::new(BAID64_ALPHABET).expect("invalid Baid64 alphabet");
            let engine = GeneralPurpose::new(&alphabet, GeneralPurposeConfig::new().with_encode_padding(false));
            write!(f, "{LOCK}={}", engine.encode(lock))?;
        }
        if let Some(expiry) = &self.expiry {
            write!(f, "{EXPIRY}={}", expiry.to_rfc3339())?;
        }
        if !self.endpoints.is_empty() {
            write!(f, "{ENDPOINTS}")?;
            let mut iter = self.endpoints.iter();
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

impl<T, A> FromStr for CallRequest<T, A>
where
    T: FromStr,
    A: FromStr,
    T::Err: Error,
    A::Err: Error,
{
    type Err = CallReqParseError<T::Err, A::Err>;

    /// # Special conditions
    ///
    /// If a URI contains more than 10 endpoints, endpoints from number 10 are ignored.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uri = Uri::parse(s)?;

        let scheme = uri.scheme();
        if scheme.as_str() != URI_SCHEME {
            return Err(CallReqParseError::SchemeInvalid(scheme.to_string()));
        }

        let path = uri.path();
        if path.is_absolute() || uri.authority().is_some() {
            return Err(CallReqParseError::Authority);
        }

        let mut path = path.split('/').collect::<VecDeque<_>>();

        let scope = path
            .pop_front()
            .ok_or(CallReqParseError::ScopeMissed)?
            .as_str()
            .parse()
            .map_err(CallReqParseError::Scope)?;

        let value_auth = path
            .pop_back()
            .ok_or(CallReqParseError::PathNoAuth)?
            .as_str();
        let (data, auth) =
            if let Some((data, auth)) = value_auth.split_once('@') { (Some(data), auth) } else { (None, (value_auth)) };
        let data = data.map(StrictVal::str);
        let auth = auth.parse().map_err(CallReqParseError::AuthInvalid)?;

        let api = path
            .pop_front()
            .map(|s| s.as_str().parse())
            .transpose()
            .map_err(CallReqParseError::ApiInvalid)?;
        let method = path.pop_front();
        let state = path.pop_front();
        let mut call = None;
        if let Some(method) = method {
            let method = method
                .as_str()
                .parse()
                .map_err(CallReqParseError::MethodInvalid)?;
            let destructible = if let Some(state) = state {
                Some(
                    state
                        .as_str()
                        .parse()
                        .map_err(CallReqParseError::StateInvalid)?,
                )
            } else {
                None
            };
            call = Some(CallState { method, destructible });
        }

        let mut query_params: IndexMap<String, String> = IndexMap::new();
        if let Some(q) = uri.query() {
            let params = q.split('&');
            for p in params {
                if let Some((k, v)) = p.split_once('=') {
                    let key = percent_decode(k.as_str().as_bytes())
                        .decode_utf8_lossy()
                        .to_string();
                    let value = percent_decode(v.as_str().as_bytes())
                        .decode_utf8_lossy()
                        .to_string();
                    query_params.insert(key, value);
                } else {
                    return Err(CallReqParseError::QueryParamInvalid(p.to_string()));
                }
            }
        }

        let lock = query_params
            .shift_remove(LOCK)
            .map(|lock| {
                let alphabet = Alphabet::new(BAID64_ALPHABET).expect("invalid Baid64 alphabet");
                let engine = GeneralPurpose::new(
                    &alphabet,
                    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::RequireNone),
                );
                let lock = engine
                    .decode(lock.as_bytes())
                    .map_err(CallReqParseError::LockInvalidEncoding)?;
                TinyBlob::try_from(lock).map_err(|_| CallReqParseError::LockTooLong)
            })
            .transpose()?;

        let expiry = query_params
            .shift_remove(EXPIRY)
            .map(|expiry| DateTime::parse_from_rfc3339(expiry.as_str()).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?;

        let endpoints = query_params
            .shift_remove(ENDPOINTS)
            .unwrap_or_default()
            .split(ENDPOINT_SEP)
            .map(Endpoint::from_str)
            .map(Result::unwrap)
            .take(10)
            .collect::<Vec<_>>();
        let endpoints = ConfinedVec::from_checked(endpoints);

        Ok(Self {
            scope,
            api,
            call,
            auth,
            data,
            lock,
            expiry,
            endpoints,
            unknown_query: query_params,
        })
    }
}

#[derive(Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum CallReqParseError<E1: Error, E2: Error> {
    #[from]
    #[display(inner)]
    Uri(ParseError),

    /// invalid contract call request URI scheme '{0}'.
    SchemeInvalid(String),

    /// contract call request must not contain any URI authority data, including empty one.
    Authority,

    #[display(inner)]
    Scope(E1),

    /// contract call request scope (first path component) is missed.
    ScopeMissed,

    /// contract call request URI misses beneficiary authority token.
    PathNoAuth,

    /// invalid beneficiary authentication token - {0}.
    AuthInvalid(E2),

    /// invalid API name - {0}.
    ApiInvalid(InvalidRString),

    /// invalid call method name - {0}.
    MethodInvalid(InvalidRString),

    /// invalid state method name - {0}.
    StateInvalid(InvalidRString),

    /// invalid lock data encoding - {0}.
    LockInvalidEncoding(DecodeError),

    /// Lock data conditions are too long (they must not exceed 256 bytes).
    LockTooLong,

    #[from]
    /// invalid expity time - {0}.
    ExpiryInvalid(chrono::ParseError),

    /// invalid query parameter {0}.
    QueryParamInvalid(String),
}
