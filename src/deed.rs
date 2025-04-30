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

use std::collections::BTreeMap;

use aluvm::LibSite;
use sonic_callreq::StateName;
use sonicapi::{CoreParams, OpBuilder};
use strict_types::StrictVal;
use ultrasonic::{AuthToken, CellAddr, Opid};

use crate::{AcceptError, Ledger, Stock};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CallParams {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub core: CoreParams,
    pub using: BTreeMap<CellAddr, StrictVal>,
    pub reading: Vec<CellAddr>,
}

pub struct DeedBuilder<'c, S: Stock> {
    pub(super) builder: OpBuilder,
    pub(super) ledger: &'c mut Ledger<S>,
}

impl<S: Stock> DeedBuilder<'_, S> {
    pub fn reading(mut self, addr: CellAddr) -> Self {
        self.builder = self.builder.access(addr);
        self
    }

    pub fn using(mut self, addr: CellAddr, witness: StrictVal) -> Self {
        self.builder = self.builder.destroy(addr, witness);
        self
    }

    pub fn append(mut self, name: impl Into<StateName>, data: StrictVal, raw: Option<StrictVal>) -> Self {
        let api = &self.ledger.schema().default_api;
        let types = &self.ledger.schema().types;
        self.builder = self.builder.add_immutable(name, data, raw, api, types);
        self
    }

    pub fn assign(
        mut self,
        name: impl Into<StateName>,
        auth: AuthToken,
        data: StrictVal,
        lock: Option<LibSite>,
    ) -> Self {
        let api = &self.ledger.schema().default_api;
        let types = &self.ledger.schema().types;
        self.builder = self
            .builder
            .add_destructible(name, auth, data, lock, api, types);
        self
    }

    pub fn commit<'a>(self) -> Result<Opid, AcceptError>
    where Self: 'a {
        let deed = self.builder.finalize();
        let opid = deed.opid();
        self.ledger.apply_verify(deed, true)?;
        self.ledger.commit_transaction();
        Ok(opid)
    }
}
