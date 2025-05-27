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
use std::path::PathBuf;

use aluvm::{CoreConfig, LibSite};
use amplify::num::u256;
use commit_verify::{Digest, Sha256};
use hypersonic::{Api, OwnedApi};
use indexmap::{indexset, IndexSet};
use petgraph::dot::{Config, Dot};
use petgraph::graph::EdgeReference;
use petgraph::prelude::NodeIndex;
use petgraph::Graph;
use rand::rng;
use rand::seq::SliceRandom;
use sonic_persist_fs::LedgerDir;
use sonicapi::{IssueParams, Issuer, Semantics, StateArithm, StateBuilder, StateConvertor};
use sonix::dump_ledger;
use strict_types::SemId;
use ultrasonic::aluvm::FIELD_ORDER_SECP;
use ultrasonic::{AuthToken, CellAddr, Codex, Consensus, Identity, Operation};

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
    }
}

fn api() -> Api {
    let types = stl::FungibleTypes::new();

    let codex = codex();

    Api {
        codex_id: codex.codex_id(),
        conforms: none!(),
        default_call: None,
        global: none!(),
        owned: tiny_bmap! {
            vname!("amount") => OwnedApi {
                sem_id: types.get("Fungible.Amount"),
                arithmetics: StateArithm::Fungible,
                convertor: StateConvertor::TypedEncoder(u256::ZERO),
                builder: StateBuilder::TypedEncoder(u256::ZERO),
                witness_sem_id: SemId::unit(),
                witness_builder: StateBuilder::TypedEncoder(u256::ZERO),
            }
        },
        aggregators: none!(),
        verifiers: tiny_bmap! {
            vname!("issue") => 0,
            vname!("transfer") => 1,
        },
        errors: Default::default(),
    }
}

fn setup(name: &str) -> LedgerDir {
    let types = stl::FungibleTypes::new();
    let codex = codex();
    let api = api();

    let semantics = Semantics {
        version: 0,
        default: api,
        custom: none!(),
        codex_libs: small_bset![libs::success()],
        api_libs: none!(),
        types: types.type_system(),
    };
    let issuer = Issuer::new(codex, semantics).unwrap();
    issuer.save("tests/data/Test.issuer").ok();

    let seed = &[0xCA; 30][..];
    let mut auth = Sha256::digest(seed);
    let mut next_auth = || -> AuthToken {
        auth = Sha256::digest(&*auth);
        let mut buf = [0u8; 30];
        buf.copy_from_slice(&auth[..30]);
        AuthToken::from(buf)
    };

    let mut issue = IssueParams::new_testnet(issuer.codex_id(), "FungibleTest", Consensus::None);
    for _ in 0u16..10 {
        issue.push_owned_unlocked("amount", next_auth(), svnum!(100u64));
        issue.push_owned_unlocked("amount", next_auth(), svnum!(100u64));
    }
    let articles = issuer.issue(issue);
    let opid = articles.genesis_opid();

    let contract_path = PathBuf::from(format!("tests/data/{name}.contract"));
    if contract_path.exists() {
        fs::remove_dir_all(&contract_path).expect("Unable to remove a contract file");
    }
    fs::create_dir_all(&contract_path).expect("Unable to create a contract folder");
    let mut ledger = LedgerDir::new(articles, contract_path).expect("Can't issue a contract");

    let owned = &ledger.state().main.owned;
    assert_eq!(owned.len(), 1);
    let owned = owned.get("amount").unwrap();
    assert_eq!(owned.len(), 20);
    let mut prev = vec![];
    for (addr, val) in owned {
        assert_eq!(val, &svnum!(100u64));
        assert_eq!(addr.opid, opid);
        prev.push(*addr);
    }
    assert_eq!(prev.len(), 20);

    for round in 0u16..10 {
        // shuffle outputs to create twisted DAG
        prev.shuffle(&mut rng());
        let mut iter = prev.into_iter();
        let mut new_prev = vec![];
        while let Some((first, second)) = iter.next().zip(iter.next()) {
            let opid = ledger
                .start_deed("transfer")
                .using(first)
                .using(second)
                .assign("amount", next_auth(), svnum!(100u64 - round as u64), None)
                .assign("amount", next_auth(), svnum!(100u64 - round as u64), None)
                .commit()
                .unwrap();
            new_prev.push(CellAddr::new(opid, 0));
            new_prev.push(CellAddr::new(opid, 1));
        }
        prev = new_prev;
    }

    let owned = &ledger.state().main.owned;
    assert_eq!(owned.len(), 1);
    assert_eq!(prev.len(), 20);
    let owned = owned.get("amount").unwrap();
    assert_eq!(owned.len(), 20);
    for (_, val) in owned.iter() {
        assert_eq!(val, &svnum!(91u64));
    }
    assert_eq!(owned.keys().collect::<BTreeSet<_>>(), prev.iter().collect::<BTreeSet<_>>());

    ledger
}

fn graph(name: &str, ledger: &LedgerDir) {
    let mut graph = Graph::<(String, bool), ()>::new();
    let genesis_opid = ledger.articles().genesis_opid();
    let mut nodes = bmap! {
        genesis_opid => graph.add_node(("0".to_string(), true))
    };
    for (opid, op) in ledger.operations() {
        let id = opid.to_string()[..2].to_string();
        let valid = ledger.is_valid(opid);
        let node = graph.add_node((id, valid));
        nodes.insert(opid, node);
        for inp in &op.destructible_in {
            graph.add_edge(node, nodes[&inp.addr.opid], ());
        }
    }

    // Generate a DOT format representation of the graph.
    let node_attr = |_: &Graph<(String, bool), ()>, (_, (name, valid)): (NodeIndex, &(String, bool))| {
        let color = match *valid {
            true => "green",
            false => "red",
        };
        format!("label=\"{name}\", color={color}, style=filled")
    };
    let edge_attr = |_: &Graph<(String, bool), ()>, _: EdgeReference<'_, ()>| String::new();
    let dot = Dot::with_attr_getters(&graph, &[Config::EdgeNoLabel], &edge_attr, &node_attr);
    let graph = format!("{dot:?}");
    fs::write(format!("tests/data/{name}.dot"), graph).unwrap();
}

#[test]
fn no_reorgs() {
    setup("NoReorgs");
    dump_ledger("tests/data/NoReorgs.contract", "tests/data/NoReorgs.dump", true).unwrap();
}

fn check_rollback(ledger: LedgerDir, mut removed: IndexSet<Operation>) -> IndexSet<Operation> {
    let opids = removed.iter().map(|op| op.opid()).collect::<BTreeSet<_>>();

    let mut index = 0usize;
    while let Some(op) = removed.get_index(index) {
        let opid = op.opid();
        let mut new = IndexSet::new();
        for no in 0..op.immutable_out.len_u16() {
            for child in ledger.read_by(CellAddr::new(opid, no)) {
                let child = ledger.operation(child);
                new.insert(child);
            }
        }
        for no in 0..op.destructible_out.len_u16() {
            let Some(child) = ledger.spent_by(CellAddr::new(opid, no)) else {
                continue;
            };
            let child = ledger.operation(child);
            new.insert(child);
        }
        removed.append(&mut new);
        index += 1;
    }

    println!("List of operations which must be rolled back (descendants):");
    let removed_opids = removed.iter().map(|op| op.opid()).collect::<BTreeSet<_>>();
    let descendants = ledger.descendants(opids).collect::<BTreeSet<_>>();
    assert_eq!(removed_opids, descendants);
    for opid in descendants {
        println!("- {opid}");
    }

    for (opid, _) in ledger.operations() {
        if removed_opids.contains(&opid) {
            assert!(!ledger.is_valid(opid));
        } else {
            assert!(ledger.is_valid(opid));
        }
    }

    // Now we check that no outputs of the rolled-back ops participate in the valid state
    let state = ledger.state().main.owned.get("amount").unwrap();
    eprintln!("Not rolled back outputs:");
    for addr in state.keys() {
        assert!(!removed_opids.contains(&addr.opid));
    }

    removed
}

#[test]
fn single_rollback() {
    let mut ledger = setup("SingleRollback");
    let (mid_opid, mid_op) = ledger.operations().nth(50).unwrap();
    println!("Rolling back {mid_opid} and its descendants");
    ledger.rollback([mid_opid]).unwrap();
    dump_ledger("tests/data/SingleRollback.contract", "tests/data/SingleRollback.dump", true).unwrap();
    graph("SingleRollback", &ledger);
    check_rollback(ledger, indexset![mid_op]);
}

#[test]
fn double_rollback() {
    let mut ledger = setup("DoubleRollback");
    let (mid_opid1, mid_op1) = ledger.operations().nth(50).unwrap();
    let (mid_opid2, mid_op2) = ledger.operations().nth(30).unwrap();
    println!("Rolling back {mid_opid1}, {mid_opid2} and their descendants");
    ledger.rollback([mid_opid1, mid_opid2]).unwrap();
    dump_ledger("tests/data/DoubleRollback.contract", "tests/data/DoubleRollback.dump", true).unwrap();
    graph("DoubleRollback", &ledger);
    check_rollback(ledger, indexset![mid_op1, mid_op2]);
}

#[test]
fn two_rollbacks() {
    let mut ledger = setup("TwoRollbacks");
    let (mid_opid1, mid_op1) = ledger.operations().nth(50).unwrap();
    let (mid_opid2, mid_op2) = ledger.operations().nth(30).unwrap();
    println!("Rolling back {mid_opid1} and its descendants");
    ledger.rollback([mid_opid1]).unwrap();
    println!("Rolling back {mid_opid2} and its descendants");
    ledger.rollback([mid_opid2]).unwrap();
    dump_ledger("tests/data/TwoRollbacks.contract", "tests/data/TwoRollbacks.dump", true).unwrap();
    graph("TwoRollbacks", &ledger);
    check_rollback(ledger, indexset![mid_op1, mid_op2]);
}

#[test]
fn rollback_forward() {
    let mut ledger = setup("RollbackForward");
    let init_state = ledger.state().main.clone();
    let (mid_opid, _) = ledger.operations().nth(50).unwrap();
    println!("Rolling back {mid_opid} and its descendants");
    ledger.rollback([mid_opid]).unwrap();
    println!("Applying {mid_opid} and its descendants back");
    ledger.forward([mid_opid]).unwrap();
    dump_ledger("tests/data/RollbackForward.contract", "tests/data/RollbackForward.dump", true).unwrap();
    graph("RollbackForward", &ledger);
    assert_eq!(ledger.state().main, init_state);
}

#[test]
fn partial_forward() {
    let mut ledger = setup("PartialForward");
    let (mid_opid1, _) = ledger.operations().nth(50).unwrap();
    let (mid_opid2, _) = ledger.operations().nth(30).unwrap();
    println!("Rolling back {mid_opid2} and its descendants");
    ledger.rollback([mid_opid2]).unwrap();
    let mid_state = ledger.state().main.clone();
    println!("Rolling back {mid_opid1} and its descendants");
    ledger.rollback([mid_opid1]).unwrap();
    println!("Applying {mid_opid2} and its descendants back");
    ledger.forward([mid_opid1]).unwrap();
    dump_ledger("tests/data/PartialForward.contract", "tests/data/PartialForward.dump", true).unwrap();
    graph("PartialForward", &ledger);
    assert_eq!(ledger.state().main, mid_state);
}
