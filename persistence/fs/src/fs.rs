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

use core::borrow::Borrow;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use aora::file::{FileAoraMap, FileAuraMap};
use aora::{AoraMap, AuraMap, TransactionalMap};
use hypersonic::{
    AcceptError, Articles, AuthToken, CellAddr, EffectiveState, IssueError, Ledger, LoadError, MergeError, Operation,
    Opid, RawState, Schema, Stock, StockError, Transition,
};
use strict_encoding::{SerializeError, StreamReader, StreamWriter, StrictReader, StrictWriter};

#[derive(Wrapper, WrapperMut, Debug, From)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
pub struct LedgerDir(Ledger<StockFs>);

const STASH_MAGIC: u64 = u64::from_be_bytes(*b"RGBSTASH");
const TRACE_MAGIC: u64 = u64::from_be_bytes(*b"RGBTRACE");
const SPENT_MAGIC: u64 = u64::from_be_bytes(*b"RGBSPENT");
const VALID_MAGIC: u64 = u64::from_be_bytes(*b"RGBVALID");

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum OpValidity {
    Invalid,
    Valid,
}

impl From<[u8; 1]> for OpValidity {
    fn from(bytes: [u8; 1]) -> Self {
        match bytes[0] {
            1 => Self::Valid,
            _ => Self::Invalid,
        }
    }
}

impl From<OpValidity> for [u8; 1] {
    fn from(v: OpValidity) -> Self {
        match v {
            OpValidity::Valid => [1],
            OpValidity::Invalid => [0],
        }
    }
}

impl From<OpValidity> for bool {
    fn from(v: OpValidity) -> Self {
        match v {
            OpValidity::Valid => true,
            OpValidity::Invalid => false,
        }
    }
}

#[derive(Debug)]
pub struct StockFs {
    path: PathBuf,
    stash: FileAoraMap<Opid, Operation, STASH_MAGIC, 1>,
    trace: FileAoraMap<Opid, Transition, TRACE_MAGIC, 1>,
    valid: FileAuraMap<Opid, OpValidity, VALID_MAGIC, 1, 32, 1>,
    spent: FileAuraMap<CellAddr, Opid, SPENT_MAGIC, 1, 34>,
    articles: Articles,
    state: EffectiveState,
}

impl StockFs {
    const FILENAME_ARTICLES: &'static str = "contract.articles";
    const FILENAME_STATE_RAW: &'static str = "state.dat";
}

impl Stock for StockFs {
    type Conf = PathBuf;
    type Error = io::Error;

    fn new(articles: Articles, path: PathBuf) -> Result<Self, IssueError<io::Error>> {
        // TODO: Move state and validity init into a shared code (i.e. `Ledger` implementation)
        let state = EffectiveState::from_genesis(&articles)
            .map_err(|e| IssueError::Genesis(articles.issue.meta.name.clone(), e))?;

        let stash = FileAoraMap::create_new(&path, "stash").map_err(IssueError::OtherPersistence)?;
        let trace = FileAoraMap::create_new(&path, "trace").map_err(IssueError::OtherPersistence)?;
        let spent = FileAuraMap::create_new(&path, "spent").map_err(IssueError::OtherPersistence)?;
        let mut valid = FileAuraMap::create_new(&path, "valid").map_err(IssueError::OtherPersistence)?;

        articles
            .save(path.join(Self::FILENAME_ARTICLES))
            .map_err(IssueError::ArticlesPersistence)?;
        state
            .raw
            .save(path.join(Self::FILENAME_STATE_RAW))
            .map_err(IssueError::StatePersistence)?;

        valid.insert_only(articles.issue.genesis_opid(), OpValidity::Valid);
        valid.commit_transaction();

        Ok(Self { path, stash, trace, spent, articles, state, valid })
    }

    fn load(path: PathBuf) -> Result<Self, LoadError<io::Error>> {
        let path = path.to_path_buf();

        let stash = FileAoraMap::open(&path, "stash").map_err(LoadError::OtherPersistence)?;
        let trace = FileAoraMap::open(&path, "trace").map_err(LoadError::OtherPersistence)?;
        let spent = FileAuraMap::open(&path, "spent").map_err(LoadError::OtherPersistence)?;
        let valid = FileAuraMap::open(&path, "valid").map_err(LoadError::OtherPersistence)?;

        let articles = Articles::load(path.join(Self::FILENAME_ARTICLES)).map_err(LoadError::ArticlesPersistence)?;
        let raw = RawState::load(path.join(Self::FILENAME_STATE_RAW)).map_err(LoadError::StatePersistence)?;
        let state = EffectiveState::with(raw, &articles.schema);

        Ok(Self { path, stash, trace, spent, articles, state, valid })
    }

    fn config(&self) -> Self::Conf { self.path.clone() }

    #[inline]
    fn articles(&self) -> &Articles { &self.articles }
    #[inline]
    fn state(&self) -> &EffectiveState { &self.state }

    #[inline]
    fn is_valid(&self, opid: Opid) -> bool { self.valid.get(opid).map(bool::from).unwrap_or_default() }
    #[inline]
    fn mark_valid(&mut self, opid: Opid) { self.valid.insert_or_update(opid, OpValidity::Valid) }
    #[inline]
    fn mark_invalid(&mut self, opid: Opid) { self.valid.insert_or_update(opid, OpValidity::Invalid) }

    #[inline]
    fn has_operation(&self, opid: Opid) -> bool { self.stash.contains_key(opid) }
    #[inline]
    fn operation(&self, opid: Opid) -> Operation { self.stash.get_expect(opid) }
    #[inline]
    fn operations(&self) -> impl Iterator<Item = (Opid, Operation)> { self.stash.iter() }
    #[inline]
    fn transition(&self, opid: Opid) -> Transition { self.trace.get_expect(opid) }
    #[inline]
    fn trace(&self) -> impl Iterator<Item = (Opid, Transition)> { self.trace.iter() }
    #[inline]
    fn spent_by(&self, addr: CellAddr) -> Option<Opid> { self.spent.get(addr) }

    fn update_articles(
        &mut self,
        f: impl FnOnce(&mut Articles) -> Result<(), MergeError>,
    ) -> Result<(), StockError<MergeError>> {
        f(&mut self.articles).map_err(StockError::Inner)?;
        self.articles
            .save(self.path.join(Self::FILENAME_ARTICLES))?;
        Ok(())
    }

    fn update_state<R>(&mut self, f: impl FnOnce(&mut EffectiveState, &Schema) -> R) -> Result<R, SerializeError> {
        let res = f(&mut self.state, &self.articles.schema);
        self.state
            .raw
            .save(self.path.join(Self::FILENAME_STATE_RAW))?;
        self.state
            .recompute(&self.articles.schema.default_api, self.articles.schema.custom_apis.keys());
        Ok(res)
    }

    #[inline]
    fn add_operation(&mut self, opid: Opid, operation: &Operation) { self.stash.insert(opid, operation) }
    #[inline]
    fn add_transition(&mut self, opid: Opid, transition: &Transition) { self.trace.insert(opid, transition) }
    #[inline]
    fn add_spending(&mut self, spent: CellAddr, spender: Opid) { self.spent.insert_or_update(spent, spender) }
    #[inline]
    fn commit_transaction(&mut self) {
        self.spent.commit_transaction();
        self.valid.commit_transaction();
    }
}

impl LedgerDir {
    pub fn new(articles: Articles, conf: PathBuf) -> Result<Self, IssueError<io::Error>> {
        Ledger::new(articles, conf).map(Self)
    }

    pub fn load(conf: PathBuf) -> Result<Self, LoadError<io::Error>> { Ledger::load(conf).map(Self) }

    pub fn backup_to_file(&mut self, output: impl AsRef<Path>) -> io::Result<()> {
        let file = File::create_new(output)?;
        let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
        self.export_all(writer)
    }

    pub fn export_to_file(
        &mut self,
        terminals: impl IntoIterator<Item = impl Borrow<AuthToken>>,
        output: impl AsRef<Path>,
    ) -> io::Result<()> {
        let file = File::create_new(output)?;
        let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
        self.export(terminals, writer)
    }

    pub fn accept_from_file(&mut self, input: impl AsRef<Path>) -> Result<(), AcceptError> {
        let file = File::open(input)?;
        let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
        self.accept(&mut reader)
    }

    pub fn path(&self) -> &Path { &self.0.stock().path }
}
