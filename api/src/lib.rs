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

// TODO: Activate once StrictEncoding will be no_std
// #![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    unsafe_code,
    dead_code,
    // TODO: Complete documentation
    // missing_docs,
    unused_variables,
    unused_mut,
    unused_imports,
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case
)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[macro_use]
extern crate core;
extern crate alloc;

#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

#[cfg(feature = "serde")]
#[macro_use]
extern crate serde;

mod api;
mod issuer;
mod articles;
mod builders;
mod state;

pub use api::{
    Api, ApisChecksum, GlobalApi, OwnedApi, ParseVersionedError, SemanticError, Semantics, StateUnknown, Versioned,
};
pub use articles::{Articles, ArticlesId, SigBlob};
pub use builders::{
    Builder, BuilderRef, CoreParams, IssueParams, IssuerSpec, NamedState, OpBuilder, OpBuilderRef, VersionRange,
};
pub use issuer::{Issuer, IssuerId, ISSUER_MAGIC_NUMBER, ISSUER_VERSION};
pub use sonic_callreq::*;
pub use state::*;
pub use ultrasonic::*;
