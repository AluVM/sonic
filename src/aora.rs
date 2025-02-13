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

// TODO: Move to amplify-io

use core::borrow::Borrow;

/// AORA: Append-only random-accessed data persistence.
pub trait Aora {
    type Item: Sized;
    type Id: Into<[u8; 32]> + From<[u8; 32]>;

    /// Adds item to the append-only log. If the item is already in the log, does noting.
    ///
    /// # Panic
    ///
    /// Panics if item under the given id is different from another item under the same id already
    /// present in the log
    fn append(&mut self, id: Self::Id, item: &Self::Item);
    fn extend(&mut self, iter: impl IntoIterator<Item = (Self::Id, impl Borrow<Self::Item>)>) {
        for (id, item) in iter {
            self.append(id, item.borrow());
        }
    }
    fn has(&self, id: &Self::Id) -> bool;
    fn read(&mut self, id: Self::Id) -> Self::Item;
    fn iter(&mut self) -> impl Iterator<Item = (Self::Id, Self::Item)>;
}

#[cfg(feature = "std")]
pub mod file {
    use std::collections::BTreeMap;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::marker::PhantomData;
    use std::path::{Path, PathBuf};

    use strict_encoding::{StreamReader, StreamWriter, StrictDecode, StrictEncode, StrictReader, StrictWriter};

    use super::*;
    use crate::expect::Expect;

    pub struct FileAora<Id: Ord + From<[u8; 32]>, T> {
        log: File,
        idx: File,
        index: BTreeMap<Id, u64>,
        _phantom: PhantomData<T>,
    }

    impl<Id: Ord + From<[u8; 32]>, T> FileAora<Id, T> {
        fn prepare(path: impl AsRef<Path>, name: &str) -> (PathBuf, PathBuf) {
            let path = path.as_ref();
            let log = path.join(format!("{name}.log"));
            let idx = path.join(format!("{name}.idx"));
            (log, idx)
        }

        pub fn new(path: impl AsRef<Path>, name: &str) -> Self {
            let (log, idx) = Self::prepare(path, name);
            let log = File::create_new(&log)
                .expect_or_else(|| format!("unable to create append-only log file `{}`", log.display()));
            let idx = File::create_new(&idx)
                .expect_or_else(|| format!("unable to create random-access index file `{}`", idx.display()));
            Self { log, idx, index: empty!(), _phantom: PhantomData }
        }

        pub fn open(path: impl AsRef<Path>, name: &str) -> Self {
            let (log, idx) = Self::prepare(path, name);
            let mut log = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&log)
                .expect_or_else(|| format!("unable to create append-only log file `{}`", log.display()));
            let mut idx = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&idx)
                .expect_or_else(|| format!("unable to create random-access index file `{}`", idx.display()));

            let mut index = BTreeMap::new();
            loop {
                let mut id = [0u8; 32];
                let res = idx.read_exact(&mut id);
                if matches!(res, Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof) {
                    break;
                } else {
                    res.expect("unable to read item ID");
                }

                let mut buf = [0u8; 8];
                idx.read_exact(&mut buf)
                    .expect("unable to read index entry");
                let pos = u64::from_le_bytes(buf);

                index.insert(id.into(), pos);
            }

            log.seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the log");
            idx.seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the index");

            Self { log, idx, index, _phantom: PhantomData }
        }
    }

    impl<Id: Ord + From<[u8; 32]> + Into<[u8; 32]>, T: Eq + StrictEncode + StrictDecode> Aora for FileAora<Id, T> {
        type Item = T;
        type Id = Id;

        fn append(&mut self, id: Self::Id, item: &T) {
            if self.has(&id) {
                let old = self.read(id);
                if &old != item {
                    panic!(
                        "item under the given id is different from another item under the same id already present in \
                         the log"
                    );
                }
                return;
            }
            let id = id.into();
            self.log
                .seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the log");
            let pos = self
                .log
                .stream_position()
                .expect("unable to get log position");
            let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(&mut self.log));
            item.strict_encode(writer).unwrap();
            self.idx
                .seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the index");
            self.idx.write_all(&id).expect("unable to write to index");
            self.idx
                .write_all(&pos.to_le_bytes())
                .expect("unable to write to index");
            self.index.insert(id.into(), pos);
        }

        fn has(&self, id: &Self::Id) -> bool { self.index.contains_key(id) }

        fn read(&mut self, id: Self::Id) -> T {
            let pos = self.index.get(&id).expect("unknown item");

            self.log
                .seek(SeekFrom::Start(*pos))
                .expect("unable to seek to the item");
            let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(&self.log));
            T::strict_decode(&mut reader).expect("unable to read item")
        }

        fn iter(&mut self) -> impl Iterator<Item = (Self::Id, T)> {
            self.log
                .seek(SeekFrom::Start(0))
                .expect("unable to seek to the start of the log file");
            self.idx
                .seek(SeekFrom::Start(0))
                .expect("unable to seek to the start of the index file");

            let reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(&self.log));
            Iter { log: reader, idx: &self.idx, _phantom: PhantomData }
        }
    }

    pub struct Iter<'file, Id: From<[u8; 32]>, T: StrictDecode> {
        log: StrictReader<StreamReader<&'file File>>,
        idx: &'file File,
        _phantom: PhantomData<(Id, T)>,
    }

    impl<Id: From<[u8; 32]>, T: StrictDecode> Iterator for Iter<'_, Id, T> {
        type Item = (Id, T);

        fn next(&mut self) -> Option<Self::Item> {
            let mut id = [0u8; 32];
            self.idx.read_exact(&mut id).ok()?;
            self.idx
                .seek(SeekFrom::Current(8))
                .expect("broken index file");
            let item = T::strict_decode(&mut self.log).ok()?;
            Some((id.into(), item))
        }
    }
}
