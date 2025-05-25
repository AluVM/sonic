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

use std::borrow::Borrow;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};

use amplify::MultiError;
use aora::file::{FileAoraIndex, FileAoraMap, FileAuraMap};
use aora::{AoraIndex, AoraMap, AuraMap, TransactionalMap};
use binfile::BinFile;
use commit_verify::StrictHash;
use hypersonic::{
    AcceptError, Articles, AuthToken, CellAddr, EffectiveState, Genesis, Identity, Issue, IssueError, Ledger,
    Operation, Opid, RawState, SemanticError, Semantics, SigBlob, Stock, Transition,
};
use strict_encoding::{
    DecodeError, StreamReader, StreamWriter, StrictDecode, StrictEncode, StrictReader, StrictWriter,
};

#[derive(Wrapper, WrapperMut, Debug, From)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
pub struct LedgerDir(Ledger<StockFs>);

const STASH_MAGIC: u64 = u64::from_be_bytes(*b"CONSTASH");
const TRACE_MAGIC: u64 = u64::from_be_bytes(*b"CONTRACE");
const SPENT_MAGIC: u64 = u64::from_be_bytes(*b"OPSPENT ");
const READ_MAGIC: u64 = u64::from_be_bytes(*b"OPREADBY");
const VALID_MAGIC: u64 = u64::from_be_bytes(*b"OPVALID ");

const SEMANTICS_MAGIC: u64 = u64::from_be_bytes(*b"SEMANTIC");
const STATE_MAGIC: u64 = u64::from_be_bytes(*b"CONSTATE");
const GENESIS_MAGIC: u64 = u64::from_be_bytes(*b"CGENESIS");

const VERSION_0: u16 = 0;

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
    read: FileAoraIndex<CellAddr, Opid, READ_MAGIC, 1, 34>,
    articles: Articles,
    state: EffectiveState,
}

impl StockFs {
    const FILENAME_CODEX: &'static str = "codex.yaml";
    const FILENAME_META: &'static str = "meta.toml";
    const FILENAME_GENESIS: &'static str = "genesis.dat";
    const FILENAME_SEMANTICS: &'static str = "semantics.dat";
    const FILENAME_STATE_RAW: &'static str = "state.dat";
}

impl Stock for StockFs {
    type Conf = PathBuf;
    type Error = FsError;

    fn new(articles: Articles, state: EffectiveState, path: PathBuf) -> Result<Self, FsError> {
        let stash = FileAoraMap::create_new(&path, "stash")?;
        let trace = FileAoraMap::create_new(&path, "trace")?;
        let spent = FileAuraMap::create_new(&path, "spent")?;
        let read = FileAoraIndex::create_new(&path, "read")?;
        let valid = FileAuraMap::create_new(&path, "valid")?;

        let meta = toml::to_string(&articles.issue().meta)?;
        let mut file = File::create_new(path.join(Self::FILENAME_META))?;
        file.write_all(meta.as_ref())?;

        let file = File::create_new(path.join(Self::FILENAME_CODEX))?;
        serde_yaml::to_writer(file, articles.codex())?;

        let file = BinFile::<GENESIS_MAGIC, VERSION_0>::create_new(path.join(Self::FILENAME_GENESIS))?;
        let writer = StreamWriter::new::<{ usize::MAX }>(file);
        articles.genesis().strict_write(writer)?;

        let file = BinFile::<SEMANTICS_MAGIC, VERSION_0>::create_new(path.join(Self::FILENAME_SEMANTICS))?;
        let mut writer = StreamWriter::new::<{ usize::MAX }>(file);
        articles.semantics().strict_write(&mut writer)?;
        articles.sig().strict_write(writer)?;

        let file = BinFile::<STATE_MAGIC, VERSION_0>::create_new(path.join(Self::FILENAME_STATE_RAW))?;
        let writer = StreamWriter::new::<{ usize::MAX }>(file);
        state.raw.strict_write(writer)?;

        Ok(Self { path, stash, trace, spent, read, articles, state, valid })
    }

    fn load(path: PathBuf) -> Result<Self, FsError> {
        let path = path.to_path_buf();

        let stash = FileAoraMap::open(&path, "stash")?;
        let trace = FileAoraMap::open(&path, "trace")?;
        let spent = FileAuraMap::open(&path, "spent")?;
        let read = FileAoraIndex::open(&path, "read")?;
        let valid = FileAuraMap::open(&path, "valid")?;

        let meta = fs::read_to_string(path.join(Self::FILENAME_META))?;
        let meta = toml::from_str(&meta)?;

        let file = File::open(path.join(Self::FILENAME_CODEX))?;
        let codex = serde_yaml::from_reader(file)?;

        // TODO: Check there is no content left at the end of reading
        let file = BinFile::<GENESIS_MAGIC, VERSION_0>::open(path.join(Self::FILENAME_GENESIS))?;
        let reader = StreamReader::new::<{ usize::MAX }>(file);
        let genesis = Genesis::strict_read(reader)?;

        let file = BinFile::<SEMANTICS_MAGIC, VERSION_0>::open(path.join(Self::FILENAME_SEMANTICS))?;
        let mut reader = StreamReader::new::<{ usize::MAX }>(file);
        let semantics = Semantics::strict_read(&mut reader)?;
        let sig = Option::<SigBlob>::strict_read(reader)?;

        let file = BinFile::<STATE_MAGIC, VERSION_0>::open(path.join(Self::FILENAME_STATE_RAW))?;
        let reader = StreamReader::new::<{ usize::MAX }>(file);
        let raw = RawState::strict_read(reader)?;

        let issue = Issue { version: default!(), meta, codex, genesis };
        let articles = match sig {
            None => Articles::new(semantics, issue)?,
            Some(_sig) => todo!("signature validation"),
        };

        let state = EffectiveState::with_raw_state(raw, &articles);

        Ok(Self { path, stash, trace, spent, read, articles, state, valid })
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
    fn read_by(&self, addr: CellAddr) -> impl Iterator<Item = Opid> { self.read.get(addr) }
    #[inline]
    fn spent_by(&self, addr: CellAddr) -> Option<Opid> { self.spent.get(addr) }

    fn update_articles(
        &mut self,
        f: impl FnOnce(&mut Articles) -> Result<bool, SemanticError>,
    ) -> Result<bool, MultiError<SemanticError, FsError>> {
        let res = f(&mut self.articles).map_err(MultiError::A)?;

        let file = BinFile::<SEMANTICS_MAGIC, VERSION_0>::create(self.path.join(Self::FILENAME_SEMANTICS))
            .map_err(MultiError::from_b)?;
        let mut writer = StreamWriter::new::<{ usize::MAX }>(file);
        self.articles
            .semantics()
            .strict_write(&mut writer)
            .map_err(MultiError::from_b)?;
        self.articles
            .sig()
            .strict_write(writer)
            .map_err(MultiError::from_b)?;

        Ok(res)
    }

    fn update_state<R>(&mut self, f: impl FnOnce(&mut EffectiveState, &Articles) -> R) -> Result<R, FsError> {
        let res = f(&mut self.state, &self.articles);

        let file = BinFile::<STATE_MAGIC, VERSION_0>::create(self.path.join(Self::FILENAME_STATE_RAW))?;
        let writer = StreamWriter::new::<{ usize::MAX }>(file);
        self.state.raw.strict_write(writer)?;

        self.state.recompute(self.articles.semantics());

        Ok(res)
    }

    #[inline]
    fn add_operation(&mut self, opid: Opid, operation: &Operation) { self.stash.insert(opid, operation) }
    #[inline]
    fn add_transition(&mut self, opid: Opid, transition: &Transition) { self.trace.insert(opid, transition) }
    #[inline]
    fn add_reading(&mut self, addr: CellAddr, spender: Opid) { self.read.push(addr, spender); }
    #[inline]
    fn add_spending(&mut self, spent: CellAddr, spender: Opid) { self.spent.insert_or_update(spent, spender) }
    #[inline]
    fn commit_transaction(&mut self) {
        self.spent.commit_transaction();
        self.valid.commit_transaction();
    }
}

impl LedgerDir {
    pub fn new(articles: Articles, conf: PathBuf) -> Result<Self, MultiError<IssueError, FsError>> {
        Ledger::new(articles, conf).map(Self)
    }

    pub fn load(conf: PathBuf) -> Result<Self, FsError> { Ledger::load(conf).map(Self) }

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

    pub fn accept_from_file<E>(
        &mut self,
        input: impl AsRef<Path>,
        sig_validator: impl FnOnce(StrictHash, &Identity, &SigBlob) -> Result<(), E>,
    ) -> Result<(), MultiError<AcceptError, FsError>> {
        let file = File::open(input).map_err(MultiError::from_b)?;
        let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
        self.accept(&mut reader, sig_validator)
    }

    pub fn path(&self) -> &Path { &self.0.stock().path }
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum FsError {
    #[from]
    Io(io::Error),

    #[from]
    Decode(DecodeError),

    #[from]
    Articles(SemanticError),

    #[from]
    Yaml(serde_yaml::Error),

    #[from]
    TomlDecode(toml::de::Error),

    #[from]
    TomlEncode(toml::ser::Error),
}
