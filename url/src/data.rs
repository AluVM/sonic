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

use amplify::confinement::{TinyOrdMap, TinyString};
use chrono::{DateTime, Utc};
use hypersonic::{ContractId, DataCell, StateName};

/// Call request provides information for constructing [`hypersonic::CallParams`].
///
/// Request doesn't specify the used capabilities of the contract (blockchain, if any; type of
/// single-use seals) since each contract is strictly committed and can be used under one and just
/// one type of capabilities.
pub struct CallRequest {
    // pub pop: S::Cap,
    pub contract_id: Option<ContractId>,
    pub method: Option<TinyString>,
    pub define: TinyOrdMap<StateName, DataCell>,
    pub expiry: Option<DateTime<Utc>>,
}
