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

use aluvm::LibSite;
use amplify::num::u256;
use strict_types::StrictVal;
use ultrasonic::AuthToken;

pub type StateTy = u256;

#[derive(Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct StateAtom {
    pub verified: StrictVal,
    pub unverified: Option<StrictVal>,
}

impl StateAtom {
    #[inline]
    pub fn new_verified(val: impl Into<StrictVal>) -> Self { Self { verified: val.into(), unverified: None } }

    #[inline]
    pub fn new_unverified(val: impl Into<StrictVal>) -> Self {
        Self { verified: StrictVal::Unit, unverified: Some(val.into()) }
    }

    #[inline]
    pub fn new(verified: impl Into<StrictVal>, unverified: impl Into<StrictVal>) -> Self {
        Self {
            verified: verified.into(),
            unverified: Some(unverified.into()),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct StructData {
    pub ty: StateTy,
    /// Transformed and typefied value extracted from [`ultrasonic::StatData`] by an ApiAdaptor.
    pub value: StrictVal,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct DataCell {
    pub data: StrictVal,
    pub auth: AuthToken,
    pub lock: Option<LibSite>,
}

impl DataCell {
    #[inline]
    pub fn new(data: impl Into<StrictVal>, auth: impl Into<AuthToken>) -> Self {
        Self { data: data.into(), auth: auth.into(), lock: None }
    }
}
