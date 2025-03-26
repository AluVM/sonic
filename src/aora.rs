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

    use amplify::confinement::ConfinedVec;
    use strict_encoding::{
        ReadRaw, StreamReader, StreamWriter, StrictDecode, StrictDumb, StrictEncode, StrictReader, StrictType,
        StrictWriter, TypedWrite,
    };

    use super::*;
    use crate::expect::Expect;
    use crate::LIB_NAME_SONIC;

    pub struct FileAora<Id: Ord + From<[u8; 32]>, T> {
        log: File,
        idx: File,
        index: BTreeMap<Id, u64>,
        _phantom: PhantomData<T>,
    }

    #[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
    #[derive(StrictType, StrictEncode, StrictDumb, StrictDecode)]
    #[strict_type(lib = LIB_NAME_SONIC)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    struct FileAoraBlob<T: Eq + StrictEncode + StrictDecode + StrictDumb> {
        index: ConfinedVec<[u8; 32]>,
        items: ConfinedVec<T>,
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

    impl<Id: Ord + From<[u8; 32]> + Into<[u8; 32]> + Clone, T: Eq + StrictEncode + StrictDecode + StrictDumb>
        FileAora<Id, T>
    {
        pub fn export<W: TypedWrite>(&mut self, mut writer: W) -> io::Result<W> {
            let index = ConfinedVec::<[u8; 32]>::from_checked(
                self.index
                    .keys()
                    .map(|id| id.clone().into())
                    .collect::<Vec<_>>(),
            );

            let data = ConfinedVec::from_checked(self.iter().map(|(_, item)| item).collect());

            let blob = FileAoraBlob { index, items: data };

            writer = blob.strict_encode(writer)?;

            Ok(writer)
        }

        pub fn import(&mut self, reader: &mut StrictReader<impl ReadRaw>) -> io::Result<()> {
            let blob =
                FileAoraBlob::<T>::strict_decode(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            for (i, id) in blob.index.iter().enumerate() {
                self.append((*id).into(), &blob.items[i]);
            }

            Ok(())
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

#[cfg(test)]
mod tests {
    use amplify::confinement::ConfinedString;
    use sonicapi::LIB_NAME_SONIC;
    use strict_encoding::{StreamReader, StreamWriter, StrictReader, StrictWriter};
    use tempfile::tempdir;

    use super::file::FileAora;
    use super::*;

    // Test type that implements all required traits
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_SONIC)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    struct TestItem {
        value: u32,
        data: ConfinedString,
    }

    // Helper function to create a temporary FileAora instance
    fn setup_test_aora() -> (tempfile::TempDir, FileAora<[u8; 32], TestItem>) {
        let dir = tempdir().unwrap();
        let aora = FileAora::new(&dir, "test");
        (dir, aora)
    }

    #[test]
    fn test_new_creates_files() {
        let dir = tempdir().unwrap();
        let name = "test_create";

        // Explicitly create log and idx paths
        let path = dir.as_ref();
        let log = path.join(format!("{name}.log"));
        let idx = path.join(format!("{name}.idx"));
        let (log_path, idx_path) = (log, idx);

        // Files shouldn't exist before creation
        assert!(!log_path.exists());
        assert!(!idx_path.exists());

        // Files shouldn't exist before creation
        assert!(!log_path.exists());
        assert!(!idx_path.exists());

        // Create new instance
        let _aora = FileAora::<[u8; 32], TestItem>::new(&dir, name);

        // Files should exist after creation
        assert!(log_path.exists());
        assert!(idx_path.exists());
    }

    #[test]
    fn test_open_existing() {
        let (dir, mut aora) = setup_test_aora();

        // Add some data
        let id1 = [1u8; 32];
        let item1 = TestItem {
            value: 42,
            data: ConfinedString::from_checked("test1".to_string()),
        };
        aora.append(id1, &item1);

        // Create new instance by opening existing files
        let mut opened_aora = FileAora::<[u8; 32], TestItem>::open(&dir, "test");

        // Verify the data is still there
        assert!(opened_aora.has(&id1));
        assert_eq!(opened_aora.read(id1), item1);
    }

    #[test]
    fn test_append_and_read() {
        let (_, mut aora) = setup_test_aora();

        let id1 = [1u8; 32];
        let item1 = TestItem {
            value: 42,
            data: ConfinedString::from_checked("test1".to_string()),
        };

        // Append and verify
        aora.append(id1, &item1);
        assert!(aora.has(&id1));
        assert_eq!(aora.read(id1), item1);

        // Append another item
        let id2 = [2u8; 32];
        let item2 = TestItem {
            value: 100,
            data: ConfinedString::from_checked("test2".to_string()),
        };
        aora.append(id2, &item2);

        // Verify both items exist
        assert!(aora.has(&id1));
        assert!(aora.has(&id2));
        assert_eq!(aora.read(id1), item1);
        assert_eq!(aora.read(id2), item2);
    }

    #[test]
    fn test_append_same_item_twice() {
        let (_, mut aora) = setup_test_aora();

        let id = [1u8; 32];
        let item = TestItem {
            value: 42,
            data: ConfinedString::from_checked("test".to_string()),
        };

        // First append
        aora.append(id, &item);

        // Second append with same data should be fine
        aora.append(id, &item);

        // Verify still only one item exists
        assert_eq!(aora.iter().count(), 1);
    }

    #[test]
    #[should_panic(expected = "item under the given id is different")]
    fn test_append_conflicting_item() {
        let (_, mut aora) = setup_test_aora();

        let id = [1u8; 32];
        let item1 = TestItem {
            value: 42,
            data: ConfinedString::from_checked("test1".to_string()),
        };
        let item2 = TestItem {
            value: 42,
            data: ConfinedString::from_checked("test2".to_string()),
        };

        aora.append(id, &item1);
        aora.append(id, &item2); // This should panic
    }

    #[test]
    fn test_extend() {
        let (_, mut aora) = setup_test_aora();

        let items = [([1u8; 32], TestItem {
                value: 1,
                data: ConfinedString::from_checked("one".to_string()),
            }),
            ([2u8; 32], TestItem {
                value: 2,
                data: ConfinedString::from_checked("two".to_string()),
            }),
            ([3u8; 32], TestItem {
                value: 3,
                data: ConfinedString::from_checked("three".to_string()),
            })];

        aora.extend(items.iter().map(|(id, item)| (*id, item)));

        assert_eq!(aora.iter().count(), 3);
        assert!(aora.has(&[1u8; 32]));
        assert!(aora.has(&[2u8; 32]));
        assert!(aora.has(&[3u8; 32]));
    }

    #[test]
    fn test_iter() {
        let (_, mut aora) = setup_test_aora();

        let items = vec![
            ([1u8; 32], TestItem {
                value: 1,
                data: ConfinedString::from_checked("one".to_string()),
            }),
            ([2u8; 32], TestItem {
                value: 2,
                data: ConfinedString::from_checked("two".to_string()),
            }),
            ([3u8; 32], TestItem {
                value: 3,
                data: ConfinedString::from_checked("three".to_string()),
            }),
        ];

        for (id, item) in &items {
            aora.append(*id, item);
        }

        let mut iter = aora.iter();
        for (expected_id, expected_item) in items {
            let (actual_id, actual_item) = iter.next().unwrap();
            assert_eq!(actual_id, expected_id);
            assert_eq!(actual_item, expected_item);
        }

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_export_import() {
        let (_dir, mut aora) = setup_test_aora();

        // Add some data
        let items = vec![
            ([1u8; 32], TestItem {
                value: 1,
                data: ConfinedString::from_checked("one".to_string()),
            }),
            ([2u8; 32], TestItem {
                value: 2,
                data: ConfinedString::from_checked("two".to_string()),
            }),
            ([3u8; 32], TestItem {
                value: 3,
                data: ConfinedString::from_checked("three".to_string()),
            }),
        ];

        for (id, item) in &items {
            aora.append(*id, item);
        }

        // Export to a vector
        let file = tempfile::NamedTempFile::new().unwrap();
        let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file.as_file()));

        let _ = aora.export(writer).expect("unable to export data");

        // Create a new AORA and import the data
        let (_, mut new_aora) = setup_test_aora();
        let mut reader =
            StrictReader::with(StreamReader::new::<{ usize::MAX }>(file.reopen().expect("unable to reopen file")));

        new_aora.import(&mut reader).expect("unable to import data");

        // Verify all data was imported correctly
        for (id, item) in items {
            assert!(new_aora.has(&id));
            assert_eq!(new_aora.read(id), item);
        }
    }

    #[test]
    fn test_empty_iter() {
        let (_, mut aora) = setup_test_aora();
        assert_eq!(aora.iter().count(), 0);
    }

    #[test]
    fn test_has_nonexistent() {
        let (_, aora) = setup_test_aora();
        assert!(!aora.has(&[1u8; 32]));
    }

    #[test]
    #[should_panic(expected = "unknown item")]
    fn test_read_nonexistent() {
        let (_, mut aora) = setup_test_aora();
        aora.read([1u8; 32]);
    }
}
