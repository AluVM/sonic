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
use fluent_uri::Uri;
use indexmap::map::Entry;
use indexmap::IndexMap;
use percent_encoding::{percent_decode, utf8_percent_encode, AsciiSet, CONTROLS};
use strict_types::{InvalidRString, StrictVal};

use crate::{CallRequest, CallState, Endpoint};

const URI_SCHEME: &str = "contract";
const LOCK: &str = "lock";
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
    pub fn has_query(&self) -> bool {
        !self.unknown_query.is_empty() || self.expiry.is_some() || self.lock.is_some() || !self.endpoints.is_empty()
    }
}

impl<T, A> Display for CallRequest<T, A>
where
    T: Display,
    A: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "contract:{}@{:-}/", self.layer1, self.scope)?;
        if let Some(api) = &self.api {
            write!(f, "{api}/")?;
        }
        if let Some(call) = &self.call {
            write!(f, "{}/", call.method)?;
            if let Some(state) = &call.owned {
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
            write!(f, "{ENDPOINTS}=")?;
            let mut iter = self.endpoints.iter().peekable();
            while let Some(endpoint) = iter.next() {
                write!(f, "{}", utf8_percent_encode(&endpoint.to_string(), QUERY_ENCODE))?;
                if iter.peek().is_some() {
                    write!(f, "{ENDPOINT_SEP}")?;
                }
            }
        }

        let mut iter = self.unknown_query.iter().peekable();
        while let Some((key, value)) = iter.next() {
            write!(f, "{}={}", utf8_percent_encode(key, QUERY_ENCODE), utf8_percent_encode(value, QUERY_ENCODE))?;
            if iter.peek().is_some() {
                f.write_str("&")?;
            }
        }
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
    type Err = ParseError<T::Err, A::Err>;

    /// # Special conditions
    ///
    /// If a URI contains more than 10 endpoints, endpoints from number 10 are ignored.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uri = Uri::parse(s)?;

        let scheme = uri.scheme();
        if scheme.as_str() != URI_SCHEME {
            return Err(ParseError::SchemeInvalid(scheme.to_string()));
        }

        let path = uri.path();
        if path.is_absolute() || uri.authority().is_some() {
            return Err(ParseError::Authority);
        }

        let mut path = path.split('/').collect::<VecDeque<_>>();

        let scope = path.pop_front().ok_or(ParseError::ScopeMissed)?.as_str();
        let (layer1, scope) = scope.split_once('@').ok_or(ParseError::NoLayer1)?;
        let layer1 = layer1.parse().map_err(|_| ParseError::Layer1)?;
        let scope = scope.parse().map_err(ParseError::Scope)?;

        let empty = path.pop_back().ok_or(ParseError::PathNoAuth)?;
        if !empty.is_empty() {
            return Err(ParseError::PathLastNoEmpty);
        }

        let value_auth = path.pop_back().ok_or(ParseError::PathNoAuth)?.as_str();
        let (data, auth) =
            if let Some((data, auth)) = value_auth.split_once('@') { (Some(data), auth) } else { (None, value_auth) };
        let data = data.map(|data| {
            u64::from_str(data)
                .map(StrictVal::num)
                .unwrap_or_else(|_| StrictVal::str(data))
        });
        let auth = auth.parse().map_err(ParseError::AuthInvalid)?;

        let api = path
            .pop_front()
            .map(|s| s.as_str().parse())
            .transpose()
            .map_err(ParseError::ApiInvalid)?;
        let method = path.pop_front();
        let state = path.pop_front();
        let mut call = None;
        if let Some(method) = method {
            let method = method.as_str().parse().map_err(ParseError::MethodInvalid)?;
            let owned = if let Some(state) = state {
                Some(state.as_str().parse().map_err(ParseError::StateInvalid)?)
            } else {
                None
            };
            call = Some(CallState { method, owned });
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
                    match query_params.entry(key) {
                        Entry::Occupied(mut prev) => {
                            prev.insert(format!("{},{value}", prev.get()));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    }
                } else {
                    return Err(ParseError::QueryParamInvalid(p.to_string()));
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
                    .map_err(ParseError::LockInvalidEncoding)?;
                TinyBlob::try_from(lock).map_err(|_| ParseError::LockTooLong)
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
            .filter(|endpoint| endpoint != &Endpoint::UnspecifiedMeans(s!("")))
            .take(10)
            .collect::<Vec<_>>();
        let endpoints = ConfinedVec::from_checked(endpoints);

        Ok(Self {
            scope,
            layer1,
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
pub enum ParseError<E1: Error, E2: Error> {
    #[from]
    #[display(inner)]
    Uri(fluent_uri::error::ParseError),

    /// invalid contract call request URI scheme '{0}'.
    SchemeInvalid(String),

    /// contract call request must not contain any URI authority data, including empty one.
    Authority,

    #[display(inner)]
    Scope(E1),

    /// absent information about layer 1
    NoLayer1,

    /// unrecognized layer 1 identifier
    Layer1,

    /// contract call request scope (first path component) is missed.
    ScopeMissed,

    /// contract call request path must end with `/`
    PathLastNoEmpty,

    /// contract call request URI misses the beneficiary authority token.
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

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use amplify::confinement::Confined;
    use chrono::TimeZone;
    use indexmap::indexmap;
    use ultrasonic::{AuthToken, ContractId};

    use super::*;

    #[test]
    fn short() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, None);
        assert_eq!(req.call, None);
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn api() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, None);
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn method() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::new("transfer")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn state() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn lock() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/?lock=A64CDrfmG483";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, Some(TinyBlob::from_checked(vec![3, 174, 2, 14, 183, 230, 27, 143, 55])));
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn expiry() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/?expiry=2021-05-20T08:32:48+00:00";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, Some(Utc.with_ymd_and_hms(2021, 5, 20, 8, 32, 48).unwrap()));
        assert_eq!(req.endpoints, none!());
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn endpoints() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
             5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/?\
             endpoints=http://127.0.0.1:8080,\
             https+json-rpc://127.0.0.1:8081,\
             wss://127.0.0.1:8081,\
             storm://127.0.0.1:8082,some_bullshit";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(
            req.endpoints,
            Confined::from_iter_checked([
                Endpoint::RestHttp("http://127.0.0.1:8080".to_owned()),
                Endpoint::JsonRpc("https+json-rpc://127.0.0.1:8081".to_owned()),
                Endpoint::WebSockets("wss://127.0.0.1:8081".to_owned()),
                Endpoint::Storm("storm://127.0.0.1:8082".to_owned()),
                Endpoint::UnspecifiedMeans("some_bullshit".to_owned())
            ])
        );
        assert!(req.unknown_query.is_empty());

        let req = CallRequest::<ContractId, AuthToken>::from_str(
            "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
             5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/?\
             endpoints=http://127.0.0.1:8080,\
             https+json-rpc://127.0.0.1:8081&\
             endpoints=wss://127.0.0.1:8081,\
             storm://127.0.0.1:8082&endpoints=some_bullshit",
        )
            .unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(
            req.endpoints,
            Confined::from_iter_checked([
                Endpoint::RestHttp("http://127.0.0.1:8080".to_owned()),
                Endpoint::JsonRpc("https+json-rpc://127.0.0.1:8081".to_owned()),
                Endpoint::WebSockets("wss://127.0.0.1:8081".to_owned()),
                Endpoint::Storm("storm://127.0.0.1:8082".to_owned()),
                Endpoint::UnspecifiedMeans("some_bullshit".to_owned())
            ])
        );
        assert!(req.unknown_query.is_empty());
    }

    #[test]
    fn unknown_query() {
        let s = "contract:tb@qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw/RGB20/transfer/amount/10@at:\
                 5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA/?sats=40&bull=shit&other=x";
        let req = CallRequest::<ContractId, AuthToken>::from_str(s).unwrap();
        assert_eq!(s, req.to_string());

        assert_eq!(
            req.scope,
            ContractId::from_str("contract:qKpMlzOe-Imn6ysZ-a8JjG2p-WHWvaFm-BWMiPi3-_LvnfRw").unwrap()
        );
        assert_eq!(req.data, Some(StrictVal::num(10u64)));
        assert_eq!(req.auth, AuthToken::from_str("at:5WIb5EMY-RCLbO3Wq-hGdddRP4-IeCQzP1y-S5H_UKzd-ViYmlA").unwrap());
        assert_eq!(req.api, Some(tn!("RGB20")));
        assert_eq!(req.call, Some(CallState::with("transfer", "amount")));
        assert_eq!(req.lock, None);
        assert_eq!(req.expiry, None);
        assert_eq!(req.endpoints, none!());
        assert_eq!(
            req.unknown_query,
            indexmap! { s!("sats") => s!("40"), s!("bull") => s!("shit"), s!("other") => s!("x") }
        );
    }
}
