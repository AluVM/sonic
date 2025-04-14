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

use core::fmt::Display;
use core::str::FromStr;
use std::convert::Infallible;

use amplify::confinement::{ConfinedVec, TinyBlob};
use baid64::Baid64ParseError;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use strict_types::{StrictType, StrictVal, TypeName, VariantName};
use ultrasonic::{AuthToken, ContractId};

use crate::LIB_NAME_SONIC;

pub type StateName = VariantName;
pub type MethodName = VariantName;

/// Combination of a method name and an optional state name used in API requests.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase", bound = ""))]
pub struct CallState {
    pub method: MethodName,
    pub destructible: Option<StateName>,
}

impl CallState {
    pub fn new(method: impl Into<MethodName>) -> Self { Self { method: method.into(), destructible: None } }

    pub fn with(method: impl Into<MethodName>, destructible: impl Into<StateName>) -> Self {
        Self {
            method: method.into(),
            destructible: Some(destructible.into()),
        }
    }
}

/// Call request provides information for constructing [`hypersonic::CallParams`].
///
/// Request doesn't specify the used capabilities of the contract (blockchain, if any; type of
/// single-use seals) since each contract is strictly committed and can be used under one and just
/// one type of capabilities.
///
/// # URI form
///
/// Call request can be represented as a URI using `contract:` scheme in the following format:
///
/// ```text
/// contract:CONTRACT-ID/API/METHOD/STATE/AUTH/DATA+STON?expiry=DATETIME&lock=BASE64&endpoints=E1,
/// E2#CHECK
/// ```
///
/// NB: Parsing and producing URI form requires use of `uri` feature.
///
/// ## Path
///
/// Instead of Contract ID a string query against a set of contracts can be used; for instance,
/// describing contract capabilities.
///
/// Some path components of the URI may be skipped. In this case URI is parsed in the following way:
/// - 3-component path, starting with `/`, provides name of the used interface standard,
///   authentication token and state information;
/// - 3-component path, not starting with `/`, provides contract ID and auth token, and should use a
///   default method and name state from the contract default API;
/// - 4-component path - contract ID and state name are given in addition to the auth token, a
///   default method used from the contract default API;
/// - 5-component path - all parameters except API name are given.
///
/// ## Query
///
/// Supported URI query parameters are:
/// - `expiry`: ISO-8601 datetime string;
/// - `lock`: Base64-encoded lock script conditions;
/// - `endpoints`: comma-separated URLs with the endpoints for uploading a resulting
///   deeds/consignment stream.
///
/// ## Fragment
///
/// Optional fragment may be present and should represent a checksum value for the URI string
/// preceding the fragment.
#[derive(Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "T: serde::Serialize, A: serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>, A: serde::Deserialize<'de>"
    ))
)]
pub struct CallRequest<T = CallScope, A = AuthToken> {
    pub scope: T,
    pub api: Option<TypeName>,
    pub call: Option<CallState>,
    pub auth: A,
    pub data: Option<StrictVal>,
    pub lock: Option<TinyBlob>,
    pub expiry: Option<DateTime<Utc>>,
    pub endpoints: ConfinedVec<Endpoint, 0, 10>,
    pub unknown_query: IndexMap<String, String>,
}

impl<Q: Display + FromStr, A> CallRequest<CallScope<Q>, A> {
    pub fn unwrap_contract_with<E>(
        self,
        f: impl FnOnce(Q) -> Result<ContractId, E>,
    ) -> Result<CallRequest<ContractId, A>, E> {
        let id = match self.scope {
            CallScope::ContractId(id) => id,
            CallScope::ContractQuery(query) => f(query)?,
        };
        Ok(CallRequest {
            scope: id,
            api: self.api,
            call: self.call,
            auth: self.auth,
            data: self.data,
            lock: self.lock,
            expiry: self.expiry,
            endpoints: self.endpoints,
            unknown_query: self.unknown_query,
        })
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        try_from = "String",
        into = "String",
        bound(serialize = "Q: Display + FromStr + Clone", deserialize = "Q: Display + FromStr"),
    )
)]
pub enum CallScope<Q: Display + FromStr = String> {
    #[display(inner)]
    ContractId(ContractId),

    #[display("contract:{0}")]
    ContractQuery(Q),
}

impl<Q: Display + FromStr> FromStr for CallScope<Q> {
    type Err = Baid64ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match ContractId::from_str(s) {
            Err(err_contract_id) => {
                if let Some(query_str) = s.strip_prefix("contract:") {
                    Q::from_str(query_str)
                        .map(CallScope::ContractQuery)
                        .map_err(|_| err_contract_id)
                } else {
                    Err(err_contract_id)
                }
            }
            Ok(id) => Ok(Self::ContractId(id)),
        }
    }
}

impl<Q: Display + FromStr> TryFrom<String> for CallScope<Q> {
    type Error = Baid64ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> { Self::from_str(&value) }
}

impl<Q: Display + FromStr + Clone> From<CallScope<Q>> for String {
    fn from(value: CallScope<Q>) -> Self { value.to_string() }
}

#[derive(Clone, Eq, PartialEq, Debug, Display)]
#[display(inner)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub enum Endpoint {
    JsonRpc(String),
    RestHttp(String),
    WebSockets(String),
    Storm(String),
    UnspecifiedMeans(String),
}

impl FromStr for Endpoint {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        match () {
            _ if lower.starts_with("http+json-rpc://") || lower.starts_with("https+json-rpc://") => {
                Ok(Endpoint::JsonRpc(s.to_string()))
            }
            _ if lower.starts_with("http://") || lower.starts_with("https://") => Ok(Endpoint::RestHttp(s.to_string())),
            _ if lower.starts_with("ws://") || lower.starts_with("wss://") => Ok(Endpoint::WebSockets(s.to_string())),
            _ if lower.starts_with("storm://") => Ok(Endpoint::Storm(s.to_string())),
            _ => Ok(Endpoint::UnspecifiedMeans(s.to_string())),
        }
    }
}

impl TryFrom<String> for Endpoint {
    type Error = Infallible;

    fn try_from(value: String) -> Result<Self, Self::Error> { Self::from_str(&value) }
}

impl From<Endpoint> for String {
    fn from(value: Endpoint) -> Self { value.to_string() }
}
