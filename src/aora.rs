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

pub trait AoraItem: Sized {
    fn aora_id(&self) -> [u8; 32];
}

/// AORA: Append-only random-accessed data persistence.
pub trait Aora<T: AoraItem> {
    fn append(&mut self, item: &T);
    fn extend(&mut self, iter: impl IntoIterator<Item = impl Borrow<T>>) {
        for item in iter {
            self.append(item.borrow());
        }
    }
    fn has(&self, id: impl Into<[u8; 32]>) -> bool;
    fn read(&mut self, id: impl Into<[u8; 32]>) -> T;
    fn iter(&mut self) -> impl Iterator<Item = ([u8; 32], T)>;
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
    use ultrasonic::Operation;

    use super::*;
    use crate::Transition;

    impl AoraItem for Operation {
        fn aora_id(&self) -> [u8; 32] { self.opid().to_byte_array() }
    }
    impl AoraItem for Transition {
        fn aora_id(&self) -> [u8; 32] { self.opid.to_byte_array() }
    }

    pub struct FileAora<T: AoraItem> {
        log: File,
        idx: File,
        index: BTreeMap<[u8; 32], u64>,
        _phantom: PhantomData<T>,
    }

    impl<T: AoraItem> FileAora<T> {
        fn prepare(path: impl AsRef<Path>, name: &str) -> (PathBuf, PathBuf) {
            let path = path.as_ref();
            let log = path.join(format!("{name}.log"));
            let idx = path.join(format!("{name}.idx"));
            (log, idx)
        }

        pub fn new(path: impl AsRef<Path>, name: &str) -> Self {
            let (log, idx) = Self::prepare(path, name);
            let log = File::create_new(log).expect("unable to create append-only log file");
            let idx = File::create_new(idx).expect("unable to create random-access index file");
            Self { log, idx, index: empty!(), _phantom: PhantomData }
        }

        pub fn open(path: impl AsRef<Path>, name: &str) -> Self {
            let (log, idx) = Self::prepare(path, name);
            let mut log = OpenOptions::new()
                .read(true)
                .write(true)
                .open(log)
                .expect("unable to open append-only log file");
            let mut idx = OpenOptions::new()
                .read(true)
                .write(true)
                .open(idx)
                .expect("unable to open random-access index file");

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

                index.insert(id, pos);
            }

            log.seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the log");
            idx.seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the index");

            Self { log, idx, index, _phantom: PhantomData }
        }
    }

    impl<T: AoraItem + StrictEncode + StrictDecode> Aora<T> for FileAora<T> {
        fn append(&mut self, item: &T) {
            let id = item.aora_id();
            let pos = self
                .log
                .stream_position()
                .expect("unable to get log position");
            let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(&mut self.log));
            item.strict_encode(writer).unwrap();
            self.idx
                .seek(SeekFrom::End(0))
                .expect("unable to seek to the end of the index");
            debug_assert_eq!(id.as_ref().len(), 32);
            self.idx
                .write_all(&id.as_ref())
                .expect("unable to write to index");
            self.idx
                .write_all(&pos.to_le_bytes())
                .expect("unable to write to index");
            self.index.insert(id, pos);
        }

        fn has(&self, id: impl Into<[u8; 32]>) -> bool { self.index.contains_key(&id.into()) }

        fn read(&mut self, id: impl Into<[u8; 32]>) -> T {
            let pos = self.index.get(&id.into()).expect("unknown item");

            self.log
                .seek(SeekFrom::Start(*pos))
                .expect("unable to seek to the item");
            let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(&self.log));
            T::strict_decode(&mut reader).expect("unable to read item")
        }

        fn iter(&mut self) -> impl Iterator<Item = ([u8; 32], T)> {
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

    pub struct Iter<'file, T: AoraItem + StrictDecode> {
        log: StrictReader<StreamReader<&'file File>>,
        idx: &'file File,
        _phantom: PhantomData<T>,
    }

    impl<'file, T: AoraItem + StrictDecode> Iterator for Iter<'file, T> {
        type Item = ([u8; 32], T);

        fn next(&mut self) -> Option<Self::Item> {
            let mut id = [0u8; 32];
            self.idx.read_exact(&mut id).ok()?;
            self.idx
                .seek(SeekFrom::Current(8))
                .expect("broken index file");
            let item = T::strict_decode(&mut self.log).ok()?;
            Some((id, item))
        }
    }
}
