// SONARE: Runtime environment for formally-verifiable distributed software
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2024 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
//
// Copyright (C) 2019-2025 LNP/BP Standards Association, Switzerland.
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

use std::iter;

use amplify::confinement::TinyOrdMap;
use ultrasonic::{CellAddr, Codex, Operation, Opid};

use super::{Repo, RepoProvider, Stash, StashProvider, State, StateProvider, Trace, TraceProvider};
use crate::api::{ApiId, DataCell, MethodName, StateName, StructData};
use crate::containers::{Contract, ContractMeta, ProofOfPubl};

pub struct ValidStash<H: StashProvider>(Stash<H>);

pub trait ProviderSet {
    type Stash: StateProvider;
    type State: StateProvider;
    type Trace: TraceProvider;
    type Repo: RepoProvider;
}

pub struct Stock<S: ProviderSet>
where
    S::Stash: StashProvider,
    S::State: StateProvider,
    S::Trace: TraceProvider,
    S::Repo: RepoProvider,
{
    pub stash: Stash<S::Stash>,
    pub state: State<S::State>,
    pub trace: Trace<S::Trace>,
    pub repo: Repo<S::Repo>,
}

impl<S: ProviderSet> Stock<S>
where
    S::Stash: StashProvider,
    S::State: StateProvider,
    S::Trace: TraceProvider,
    S::Repo: RepoProvider,
{
    // Ony default API can be used for the contract issue, thus we do not provide any API id here
    pub fn issue<PoP: ProofOfPubl>(
        &mut self,
        meta: ContractMeta<PoP>,
        codex: Codex,
        call: MethodName,
        append_only: TinyOrdMap<StateName, StructData>,
        destructible: TinyOrdMap<StateName, DataCell>,
    ) -> Result<Contract<PoP>, ()> {
        // 1. Create operation
        // 2. Validation operation
        // 3. Add it to stash
        // 4. Add to trace
        // 5. Add to state
        todo!()
    }

    pub fn exec(
        &mut self,
        api: ApiId,
        call: MethodName,
        append_only_input: TinyOrdMap<StateName, CellAddr>,
        destructivle_input: TinyOrdMap<StateName, CellAddr>,
        append_only_output: TinyOrdMap<StateName, StructData>,
        destructible_output: TinyOrdMap<StateName, DataCell>,
    ) -> Result<Operation, ()> {
        todo!()
    }

    pub fn validate(&mut self, other: &Stash<S::Stash>) -> Result<ValidStash<S::Stash>, ()> { todo!() }

    pub fn accept(&mut self, other: &ValidStash<S::Stash>) { todo!() }

    // this should return stash-type object
    pub fn subset(&self, terminals: impl Iterator<Item = Opid>) -> Result<impl Iterator<Item = &Operation>, ()> {
        todo!();
        Ok(iter::empty())
    }
}
