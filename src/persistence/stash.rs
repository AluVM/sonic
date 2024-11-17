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

use ultrasonic::{Codex, Operation, Opid};

/// Stash is an ordered set of operations under a contract, such that for any given unordered subset
/// of operations which represent part of some terminal contract state it can produce a valid
/// evaluation order.
pub struct Stash<H: StashProvider> {
    provider: H,
}

impl<H: StashProvider> Stash<H> {
    pub fn new(provider: H) -> Self { Stash { provider } }

    pub fn codex(&self) -> &Codex { self.provider.codex() }

    pub fn merge(&mut self, other: &Self) {
        let mut next = other.provider.first();
        while let Some(opid) = next {
            let op = other
                .provider
                .operation(opid)
                .expect("invalid stash implementation");
            self.provider.append(op.clone());
            next = other.provider.next(opid);
        }
    }

    /// Iterate over a subset of all operations in a valid evaluation order, which evaluates to a
    /// contract state with terminals defined by the provided `terminal` argument.
    pub fn subset(&self, terminals: impl Iterator<Item = Opid>) -> impl Iterator<Item = &Operation> {
        struct Iter<'provider, H: StashProvider, I: Iterator<Item = Opid>> {
            opids: I,
            provider: &'provider H,
        }
        impl<'provider, H: StashProvider, I: Iterator<Item = Opid>> Iterator for Iter<'provider, H, I> {
            type Item = &'provider Operation;
            fn next(&mut self) -> Option<Self::Item> {
                self.opids
                    .next()
                    .and_then(|opid| self.provider.operation(opid))
            }
        }

        let opids = self.provider.ancestors(terminals);
        Iter { opids, provider: &self.provider }
    }
}

pub trait StashProvider {
    fn codex(&self) -> &Codex;

    fn first(&self) -> Option<Opid>;
    fn next(&self, after: Opid) -> Option<Opid>;
    fn operation(&self, opid: Opid) -> Option<&Operation>;

    /// Returns whether operation was already known.
    fn append(&mut self, op: Operation) -> bool;

    /// Computes are returns an iterator over all operations (in a valid evaluation ordering) which
    /// are ancestors for a given terminal operations.
    fn ancestors(&self, terminals: impl Iterator<Item = Opid>) -> impl Iterator<Item = Opid>;
}
