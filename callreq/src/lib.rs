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

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

//! _Request_ (or _transaction request_) is a specification on constructing a transaction for a
//! SONARE contract.
//!
//! # URL Representation
//!
//! `contract:[//USER@NODE:PORT/]CONTRACT_ID[/API][/METHOD[/STATE]]/[VALUE][?ARGS]`
//!
//! A contract calls are URIs and URLS, which may have multiple forms (depending on the backend).
//! Here are the examples for the `castVote` call for the DAO contract from the examples directory:
//! - Using SONARE runtime:
//!   `contract:DAO.indsc.org/castVote?voting=id&with=(id,preimage)&next=(id,hash)&vote=pro`
//! - Using a server providing SONIC API:
//!   `contract://any.sonicapi.node/DAO.indsc.org/castVote?voting=id&with=(id,preimage)&next=(id,
//!   hash)&vote=pro`
//! - Using a server providing HTTP REST SONIC API: `https://contract@any.sonicapi.node/DAO.indsc.org/castVote?voting=id&with=(id,preimage)&next=(id,hash)&vote=pro`
//! - Using a websocket connection:
//!   `wws://contract@any.sonicapi.node/DAO.indsc.org/castVote?voting=id&with=(id,preimage)&
//!   next=(id,hash)&vote=pro`
//! - Using a Storm node server which contains SONARE runtime:
//!   `storm://any.storm.node/contract:DAO.indsc.org/castVote?voting=id&with=(id,preimage)&next=(id,
//!   hash)&vote=pro`

extern crate alloc;
#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_encoding;

#[cfg(feature = "serde")]
#[macro_use]
extern crate serde;
extern crate core;

mod data;
#[cfg(feature = "uri")]
mod uri;
mod builder;

pub use data::{CallRequest, CallScope, CallState, Endpoint, MethodName, StateName};

pub const LIB_NAME_SONIC: &str = "SONIC";
