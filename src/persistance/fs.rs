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

use core::borrow::Borrow;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use aora::file::{FileAoraMap, FileAuraMap};
use aora::{AoraMap, AuraMap};
use sonicapi::{Articles, MergeError, Schema};
use strict_encoding::{DeserializeError, SerializeError, StreamReader, StreamWriter, StrictReader, StrictWriter};
use ultrasonic::{AuthToken, CallError, CellAddr, ContractName, Operation, Opid};

use crate::{AcceptError, EffectiveState, RawState, Stock, StockError, Supply, Transition};

pub type FileStock = Stock<FileSupply>;

const STASH_MAGIC: u64 = u64::from_be_bytes(*b"RGBSTASH");
const TRACE_MAGIC: u64 = u64::from_be_bytes(*b"RGBTRACE");
const SPENT_MAGIC: u64 = u64::from_be_bytes(*b"RGBSPENT");

pub struct FileSupply {
    path: PathBuf,
    stash: FileAoraMap<Opid, Operation, STASH_MAGIC, 1>,
    trace: FileAoraMap<Opid, Transition, TRACE_MAGIC, 1>,
    spent: FileAuraMap<CellAddr, Opid, SPENT_MAGIC, 1, 34>,
    articles: Articles,
    state: EffectiveState,
}

impl FileSupply {
    const FILENAME_ARTICLES: &'static str = "contract.articles";
    const FILENAME_STATE_RAW: &'static str = "state.dat";
    const CONTRACT_DIR_EXTENSION: &'static str = "contract";

    pub fn issue(articles: Articles, path: impl AsRef<Path>) -> Result<Self, IssueError> {
        let state = EffectiveState::from_genesis(&articles)
            .map_err(|e| IssueError::Genesis(articles.issue.meta.name.clone(), e))?;

        let name = format!("{}.{}.{}", articles.issue.meta.name, articles.contract_id(), Self::CONTRACT_DIR_EXTENSION);
        let path = path.as_ref().join(name);
        fs::create_dir_all(&path)?;

        let stash = FileAoraMap::create_new(&path, "stash")?;
        let trace = FileAoraMap::create_new(&path, "trace")?;
        let spent = FileAuraMap::create_new(&path, "spent")?;

        articles
            .save(path.join(Self::FILENAME_ARTICLES))
            .map_err(IssueError::ArticlesPersistence)?;
        state
            .raw
            .save(path.join(Self::FILENAME_STATE_RAW))
            .map_err(IssueError::StatePersistence)?;

        Ok(Self { path, stash, trace, spent, articles, state })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref().to_path_buf();

        let stash = FileAoraMap::open(&path, "stash")?;
        let trace = FileAoraMap::open(&path, "trace")?;
        let spent = FileAuraMap::open(&path, "spent")?;

        let articles = Articles::load(path.join(Self::FILENAME_ARTICLES)).map_err(LoadError::ArticlesPersistence)?;
        let raw = RawState::load(path.join(Self::FILENAME_STATE_RAW)).map_err(LoadError::StatePersistence)?;
        let state = EffectiveState::with(raw, &articles.schema);

        Ok(Self { path, stash, trace, spent, articles, state })
    }
}

impl Supply for FileSupply {
    #[inline]
    fn articles(&self) -> &Articles { &self.articles }
    #[inline]
    fn state(&self) -> &EffectiveState { &self.state }
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
}

impl FileStock {
    pub fn issue(articles: Articles, path: impl AsRef<Path>) -> Result<Self, IssueError> {
        FileSupply::issue(articles, path).map(Self)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> { FileSupply::load(path).map(Self) }

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
        self.import(&mut reader)
    }

    pub fn path(&self) -> &Path { &self.0.path }
}

#[derive(Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum IssueError {
    #[from]
    #[display(inner)]
    Io(io::Error),

    /// unable to issue a new contract '{0}' due to invalid genesis data. Specifically, {1}
    Genesis(ContractName, CallError),

    /// unable to save contract articles - {0}
    ArticlesPersistence(SerializeError),

    /// unable to save contract state data - {0}
    StatePersistence(SerializeError),
}

#[derive(Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum LoadError {
    #[from]
    #[display(inner)]
    Io(io::Error),

    /// unable to load contract articles - {0}
    ArticlesPersistence(DeserializeError),

    /// unable to load contract state data - {0}
    StatePersistence(DeserializeError),
}
