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

extern crate alloc;

#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use aluvm::{CoreConfig, LibSite};
use amplify::num::u256;
use commit_verify::{Digest, Sha256};
use hypersonic::embedded::{EmbeddedArithm, EmbeddedImmutable, EmbeddedProc};
use hypersonic::persistance::LedgerDir;
use hypersonic::{Api, ApiInner, DestructibleApi, Schema};
use sonicapi::IssueParams;
use sonix::dump_ledger;
use ultrasonic::aluvm::FIELD_ORDER_SECP;
use ultrasonic::{AuthToken, CellAddr, Codex, Consensus, Identity};

fn codex() -> Codex {
    let lib = libs::success();
    let lib_id = lib.lib_id();
    Codex {
        name: tiny_s!("FungibleToken"),
        developer: Identity::default(),
        version: default!(),
        timestamp: 1732529307,
        field_order: FIELD_ORDER_SECP,
        input_config: CoreConfig::default(),
        verification_config: CoreConfig::default(),
        verifiers: tiny_bmap! {
            0 => LibSite::new(lib_id, 0),
            1 => LibSite::new(lib_id, 0),
        },
        reserved: default!(),
    }
}

fn api() -> Api {
    let types = stl::FungibleTypes::new();

    let codex = codex();

    Api::Embedded(ApiInner::<EmbeddedProc> {
        version: default!(),
        codex_id: codex.codex_id(),
        timestamp: 1732529307,
        name: None,
        developer: Identity::default(),
        conforms: None,
        default_call: None,
        append_only: none!(),
        destructible: tiny_bmap! {
            vname!("amount") => DestructibleApi {
                sem_id: types.get("Fungible.Amount"),
                arithmetics: EmbeddedArithm::Fungible,
                adaptor: EmbeddedImmutable(u256::ZERO),
            }
        },
        readers: none!(),
        verifiers: tiny_bmap! {
            vname!("issue") => 0,
            vname!("transfer") => 1,
        },
        errors: Default::default(),
        reserved: Default::default(),
    })
}

fn setup() -> LedgerDir {
    let types = stl::FungibleTypes::new();
    let codex = codex();
    let api = api();

    let issuer = Schema::new(codex, api, [libs::success()], types.type_system());

    let seed = &[0xCA; 30][..];
    let mut auth = Sha256::digest(seed);
    let mut next_auth = || -> AuthToken {
        auth = Sha256::digest(&*auth);
        let mut buf = [0u8; 30];
        buf.copy_from_slice(&auth[..30]);
        AuthToken::from(buf)
    };

    let mut issue = IssueParams::new_testnet("FungibleTest", Consensus::None);
    for _ in 0u16..10 {
        issue.push_owned_unlocked("amount", next_auth(), svnum!(100u64));
    }
    let articles = issuer.issue(issue);
    let opid = articles.issue.genesis_opid();

    let contract_path = Path::new("tests/data/Reorg.contract");
    if contract_path.exists() {
        fs::remove_dir_all(contract_path).expect("Unable to remove a contract file");
    }
    fs::create_dir_all(contract_path).expect("Unable to create a contract folder");
    let mut ledger = LedgerDir::new(articles, contract_path.to_path_buf()).expect("Can't issue a contract");

    let owned = &ledger.state().main.owned;
    assert_eq!(owned.len(), 1);
    let owned = owned.get("amount").unwrap();
    let mut prev = vec![];
    for (addr, val) in owned {
        assert_eq!(val, &svnum!(100u64));
        assert_eq!(addr.opid, opid);
        prev.push(*addr);
    }

    for _ in 0u16..10 {
        let mut new_prev = vec![];
        for prevout in prev {
            let opid = ledger
                .start_deed("transfer")
                .using(prevout, svnum!(0u64))
                .assign("amount", next_auth(), svnum!(100u64), None)
                .commit()
                .unwrap();
            new_prev.push(CellAddr::new(opid, 0));
        }
        prev = new_prev;
    }

    let owned = &ledger.state().main.owned;
    assert_eq!(owned.len(), 1);
    assert_eq!(prev.len(), 10);
    let owned = owned.get("amount").unwrap();
    assert_eq!(owned.len(), 10);
    for (_, val) in owned.iter() {
        assert_eq!(val, &svnum!(100u64));
    }
    assert_eq!(owned.keys().collect::<BTreeSet<_>>(), prev.iter().collect::<BTreeSet<_>>());

    ledger
}

mod libs {
    use aluvm::{aluasm, Lib};

    pub fn success() -> Lib {
        let code = aluasm! {
            stop;
        };
        Lib::assemble(&code).unwrap()
    }
}

mod stl {
    use strict_types::stl::std_stl;
    use strict_types::{LibBuilder, SemId, SymbolicSys, SystemBuilder, TypeLib, TypeSystem};

    use super::*;

    pub const LIB_NAME_FUNGIBLE: &str = "Fungible";

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(inner)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_FUNGIBLE)]
    pub struct Amount(u64);

    pub fn stl() -> TypeLib {
        LibBuilder::with(libname!(LIB_NAME_FUNGIBLE), [std_stl().to_dependency_types()])
            .transpile::<Amount>()
            .compile()
            .expect("invalid Fungible type library")
    }

    #[derive(Debug)]
    pub struct FungibleTypes(SymbolicSys);

    impl Default for FungibleTypes {
        fn default() -> Self { FungibleTypes::new() }
    }

    impl FungibleTypes {
        pub fn new() -> Self {
            Self(
                SystemBuilder::new()
                    .import(std_stl())
                    .unwrap()
                    .import(stl())
                    .unwrap()
                    .finalize()
                    .unwrap(),
            )
        }

        pub fn type_system(&self) -> TypeSystem {
            let types = stl().types;
            let types = types.iter().map(|(tn, ty)| ty.sem_id_named(tn));
            self.0.as_types().extract(types).unwrap()
        }

        pub fn get(&self, name: &'static str) -> SemId {
            *self
                .0
                .resolve(name)
                .unwrap_or_else(|| panic!("type '{name}' is absent in the type library"))
        }
    }
}

#[test]
fn no_reorgs() {
    setup();
    dump_ledger("tests/data/Reorg.contract", "tests/data/Reorg.dump", true).unwrap();
}
