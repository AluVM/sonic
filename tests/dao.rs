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

use std::fs;
use std::path::Path;

use aluvm::{CoreConfig, LibSite};
use amplify::num::u256;
use commit_verify::{Digest, Sha256};
use hypersonic::{Api, GlobalApi, OwnedApi};
use sonic_persist_fs::LedgerDir;
use sonicapi::{
    Aggregator, Issuer, RawBuilder, RawConvertor, Semantics, StateArithm, StateBuilder, StateConvertor, SubAggregator,
};
use strict_types::{SemId, StrictVal};
use ultrasonic::aluvm::FIELD_ORDER_SECP;
use ultrasonic::{AuthToken, CellAddr, Codex, Consensus, Identity};

fn codex() -> Codex {
    let lib = libs::success();
    let lib_id = lib.lib_id();
    Codex {
        name: tiny_s!("SimpleDAO"),
        developer: Identity::default(),
        version: default!(),
        timestamp: 1732529307,
        features: none!(),
        field_order: FIELD_ORDER_SECP,
        input_config: CoreConfig::default(),
        verification_config: CoreConfig::default(),
        verifiers: tiny_bmap! {
            0 => LibSite::new(lib_id, 0),
            1 => LibSite::new(lib_id, 0),
            2 => LibSite::new(lib_id, 0),
        },
    }
}

fn api() -> Api {
    let types = stl::DaoTypes::new();

    let codex = codex();

    Api {
        codex_id: codex.codex_id(),
        conforms: none!(),
        default_call: None,
        global: tiny_bmap! {
            vname!("_parties") => GlobalApi {
                published: true,
                sem_id: types.get("DAO.PartyId"),
                convertor: StateConvertor::TypedEncoder(u256::ZERO),
                builder: StateBuilder::TypedEncoder(u256::ZERO),
                raw_convertor: RawConvertor::StrictDecode(types.get("DAO.Party")),
                raw_builder: RawBuilder::StrictEncode(types.get("DAO.Party")),
            },
            vname!("_votings") => GlobalApi {
                published: true,
                sem_id: types.get("DAO.VoteId"),
                convertor: StateConvertor::TypedEncoder(u256::ONE),
                builder: StateBuilder::TypedEncoder(u256::ONE),
                raw_convertor: RawConvertor::StrictDecode(types.get("DAO.Voting")),
                raw_builder: RawBuilder::StrictEncode(types.get("DAO.Voting")),
            },
            vname!("_votes") => GlobalApi {
                published: true,
                sem_id: types.get("DAO.CastVote"),
                convertor: StateConvertor::TypedEncoder(u256::from(2u8)),
                builder: StateBuilder::TypedEncoder(u256::from(2u8)),
                raw_convertor: RawConvertor::StrictDecode(SemId::unit()),
                raw_builder: RawBuilder::StrictEncode(SemId::unit()),
            },
        },
        owned: tiny_bmap! {
            vname!("signers") => OwnedApi {
                sem_id: types.get("DAO.PartyId"),
                arithmetics: StateArithm::NonFungible,
                convertor: StateConvertor::TypedEncoder(u256::ZERO),
                builder: StateBuilder::TypedEncoder(u256::ZERO),
                witness_sem_id: SemId::unit(),
                witness_builder: StateBuilder::TypedEncoder(u256::ZERO),
            }
        },
        aggregators: tiny_bmap! {
            vname!("parties") => Aggregator::Take(SubAggregator::MapV2U(vname!("_parties"))),
            vname!("votings") => Aggregator::Take(SubAggregator::MapV2U(vname!("_votings"))),
            vname!("votes") => Aggregator::Take(SubAggregator::SetV(vname!("_votes"))),
            vname!("votingCount") => Aggregator::Take(SubAggregator::Count(vname!("_votings"))),
        },
        verifiers: tiny_bmap! {
            vname!("setup") => 0,
            vname!("proposal") => 1,
            vname!("castVote") => 2,
        },
        errors: Default::default(),
    }
}

fn main() {
    let types = stl::DaoTypes::new();
    let codex = codex();
    let api = api();

    // Creating DAO with three participants
    let semantics = Semantics {
        version: 0,
        default: api,
        custom: none!(),
        codex_libs: small_bset![libs::success()],
        api_libs: none!(),
        types: types.type_system(),
    };
    let issuer = Issuer::new(codex, semantics).unwrap();
    let filename = "examples/dao/data/SimpleDAO.issuer";
    fs::remove_file(filename).ok();
    issuer
        .save(filename)
        .expect("unable to save an issuer to a file");

    let seed = &[0xCA; 30][..];
    let mut auth = Sha256::digest(seed);
    let mut next_auth = || -> AuthToken {
        auth = Sha256::digest(&*auth);
        let mut buf = [0u8; 30];
        buf.copy_from_slice(&auth[..30]);
        AuthToken::from(buf)
    };

    let alice_auth = next_auth();
    let bob_auth = next_auth();
    let carol_auth = next_auth();

    let articles = issuer
        .start_issue_testnet("setup", Consensus::None)
        // Alice
        .append("_parties", svnum!(0u64), Some(ston!(name "alice", identity "Alice Wonderland")))
        .assign("signers", alice_auth, svnum!(0u64), None)
        // Bob
        .append("_parties", svnum!(1u64), Some(ston!(name "bob", identity "Bob Capricorn")))
        .assign("signers", bob_auth, svnum!(1u64), None)
        // Carol
        .append("_parties", svnum!(2u64), Some(ston!(name "carol", identity "Carol Caterpillar")))
        .assign("signers", carol_auth, svnum!(2u64), None)

        .finish("WonderlandDAO", 1732529307);
    let opid = articles.genesis_opid();

    let contract_path = Path::new("examples/dao/data/WonderlandDAO.contract");
    if contract_path.exists() {
        fs::remove_dir_all(contract_path).expect("Unable to remove a contract file");
    }
    fs::create_dir_all(contract_path).expect("Unable to create a contract folder");
    let mut ledger = LedgerDir::new(articles, contract_path.to_path_buf()).expect("Can't issue contract");

    // Proposing vote
    let votings = ledger
        .start_deed("proposal")
        .append(
            "_votings",
            svnum!(100u64),
            Some(ston!(title "Is Alice on duty today?", text "Vote 'pro' if Alice should be on duty today")),
        )
        .commit()
        .unwrap();

    let alice_auth2 = next_auth();
    let bob_auth2 = next_auth();
    let carol_auth2 = next_auth();

    // Alice vote against her being on duty today
    ledger
        .start_deed("castVote")
        .using(CellAddr::new(opid, 0))
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(0u8), partyId 0u64), None)
        .assign("signers", alice_auth2, svnum!(0u64), None)
        .commit()
        .unwrap();

    // Bob and Carol vote for Alice being on duty today
    ledger
        .start_deed("castVote")
        .using(CellAddr::new(opid, 1))
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(1u8), partyId 1u64), None)
        .assign("signers", bob_auth2, svnum!(1u64), None)
        .commit()
        .unwrap();
    ledger
        .start_deed("castVote")
        .using(CellAddr::new(opid, 2))
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(1u8), partyId 2u64), None)
        .assign("signers", carol_auth2, svnum!(2u64), None)
        .commit()
        .unwrap();

    let StrictVal::Map(votings) = ledger.state().read("votings") else {
        panic!("invalid data")
    };
    let (_, first_voting) = votings.first().unwrap();
    println!("voting: {first_voting}");
    println!("Votes:");
    let StrictVal::Set(votes) = ledger.state().read("votes") else {
        panic!("invalid data")
    };
    for vote in votes {
        println!("- {vote}");
    }

    // Now anybody accessing this file can figure out who is on duty today, by the decision of DAO.
    let deeds_path = Path::new("examples/dao/data/voting.deeds");
    if deeds_path.exists() {
        fs::remove_file(deeds_path).expect("unable to remove contract file");
    }

    ledger
        .export_to_file([alice_auth2, bob_auth2, carol_auth2], "examples/dao/data/voting.deeds")
        .expect("unable to save deeds to a file");
}

mod libs {
    use aluvm::{aluasm, Lib};

    pub fn success() -> Lib {
        let code = aluasm! {
            stop;
        };
        Lib::assemble(&code).unwrap()
    }

    #[allow(dead_code)]
    pub fn cast_vote() -> Lib {
        // 1. Verify that there is just one referenced global state for the party and one for the voting
        // 2. Verify that the referenced global state has a valid voteId matching the one provided operation
        // 3. Verify that the referenced global state has a valid partyId matching the one provided
        //    operation
        // 4. Verify there is just one input
        // 5. Verify that the provided witness argument is a preimage of the input
        todo!()
    }
}

mod stl {
    use amplify::confinement::{SmallString, TinyString};
    use strict_encoding::{StrictDecode, StrictDumb, StrictEncode};
    use strict_types::stl::std_stl;
    use strict_types::{LibBuilder, SemId, SymbolicSys, SystemBuilder, TypeLib, TypeSystem};

    use super::*;

    pub const LIB_NAME_DAO: &str = "DAO";

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(inner)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct PartyId(u64);

    #[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(r#"{name} "{identity}""#)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct Party {
        pub name: TinyString,
        pub identity: TinyString,
    }

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(inner)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct VoteId(u64);

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(lowercase)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO, tags = repr, try_from_u8, into_u8)]
    #[repr(u8)]
    pub enum Vote {
        #[strict_type(dumb)]
        Contra = 0,
        Pro = 1,
    }

    #[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display("Title: {title}\n\n{text}")]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct Voting {
        pub title: TinyString,
        pub text: SmallString,
    }

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From, Display)]
    #[display("Participant #{party_id} voted {vote} in voting #{vote_id}")]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct CastVote {
        pub vote_id: VoteId,
        pub vote: Vote,
        pub party_id: PartyId,
    }

    #[derive(Debug)]
    pub struct DaoTypes(SymbolicSys);

    impl Default for DaoTypes {
        fn default() -> Self { DaoTypes::new() }
    }

    pub fn stl() -> TypeLib {
        LibBuilder::with(libname!(LIB_NAME_DAO), [std_stl().to_dependency_types()])
            .transpile::<Party>()
            .transpile::<Voting>()
            .transpile::<CastVote>()
            .compile()
            .expect("invalid DAO type library")
    }

    impl DaoTypes {
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
                .unwrap_or_else(|| panic!("type '{name}' is absent in standard RGBContract type library"))
        }
    }
}
