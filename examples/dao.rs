#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

use std::path::Path;

use aluvm::{CoreConfig, LibSite};
use commit_verify::{Digest, Ripemd160};
use rand::random;
use sonic::embedded::{EmbeddedArithm, EmbeddedImmutable, EmbeddedProc, EmbeddedReaders};
use sonic::{Api, ApiInner, AppendApi, Deeds, DestructibleApi, IssueBuilder, Issuer, Private};
use strict_types::{SemId, StrictVal};
use ultrasonic::{fe128, CellAddr, Codex, Identity, Opid};

use crate::stl::{PartyId, Vote, VoteId};

fn codex() -> Codex {
    let lib = libs::success();
    let lib_id = lib.lib_id();
    Codex {
        version: default!(),
        name: tiny_s!("Simple DAO"),
        developer: Identity::default(),
        timestamp: 1732529307,
        field_order: 0xFFFFFFFF00000001,
        input_config: CoreConfig::default(),
        verification_config: CoreConfig::default(),
        verifiers: tiny_bmap! {
            0 => LibSite::new(lib_id, 0),
            1 => LibSite::new(lib_id, 0),
            2 => LibSite::new(lib_id, 0),
        },
        reserved: default!(),
    }
}

fn api() -> Api {
    let types = stl::DaoTypes::new();

    let codex = codex();

    Api::Embedded(ApiInner::<EmbeddedProc> {
        version: default!(),
        codex_id: codex.codex_id(),
        timestamp: 1732529307,
        name: None,
        developer: Identity::default(),
        append_only: tiny_bmap! {
            vname!("_parties") => AppendApi {
                sem_id: types.get("DAO.PartyId"),
                raw_sem_id: types.get("DAO.Party"),
                published: true,
                adaptor: EmbeddedImmutable(0),
            },
            vname!("_votings") => AppendApi {
                sem_id: types.get("DAO.VoteId"),
                raw_sem_id: types.get("DAO.Voting"),
                published: true,
                adaptor: EmbeddedImmutable(1),
            },
            vname!("_votes") => AppendApi {
                sem_id: types.get("DAO.CastVote"),
                raw_sem_id: SemId::unit(),
                published: true,
                adaptor: EmbeddedImmutable(2),
            },
        },
        destructible: tiny_bmap! {
            vname!("signers") => DestructibleApi {
                sem_id: types.get("DAO.PartyId"),
                arithmetics: EmbeddedArithm::NonFungible,
                adaptor: EmbeddedImmutable(0),
            }
        },
        readers: tiny_bmap! {
            vname!("parties") => EmbeddedReaders::MapV2U(vname!("_parties")),
            vname!("votings") => EmbeddedReaders::MapV2U(vname!("_votings")),
            vname!("votes") => EmbeddedReaders::SetV(vname!("_votes")),
            vname!("votingCount") => EmbeddedReaders::Count(vname!("votings")),
        },
        verifiers: tiny_bmap! {
            vname!("setup") => 0,
            vname!("proposal") => 1,
            vname!("castVote") => 2,
        },
        errors: Default::default(),
    })
}

#[derive(Clone, Debug)]
pub struct DaoBuilder {
    name: &'static str,
    timestamp: i64,
    builder: IssueBuilder,
}

impl DaoBuilder {
    pub fn add_party(mut self, id: PartyId, nick: &str, full_name: &str) -> Self {
        self.builder = self
            .builder
            .append("_parties", svnum!(id.0), Some(ston!(name nick, identity full_name)))
            .assign("signers", fe128(0), svnum!(id.0), None);
        self
    }

    pub fn finish(self) -> Dao { Dao(self.builder.finish::<Private>(self.name, self.timestamp)) }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Dao(Deeds<Private>);

impl Dao {
    pub fn codex() -> Issuer {
        let types = stl::DaoTypes::new();
        let codex = codex();
        let api = api();
        Issuer::new(codex, api, [libs::success()], types.type_system())
    }

    pub fn create(name: &'static str, timestamp: i64) -> DaoBuilder {
        let codex = Self::codex();
        let builder = codex.start_issue("setup");
        DaoBuilder { builder, name, timestamp }
    }

    pub fn start_voting(&mut self, vote_id: VoteId, title: &str, text: &str) -> VotingRef {
        let opid = self
            .0
            .start_deed("proposal")
            .append("_votings", svnum!(vote_id.0), Some(ston!(title title, text text)))
            .commit();
        VotingRef { addr: CellAddr::new(opid, 0), vote_id }
    }

    pub fn vote(&mut self, voting: VotingRef, vote: Vote, party: &mut PartySigner) -> Opid {
        let init_state = self.0.effective_state();
        let party_id = party.id();
        let unlock = party.unlock();
        self.0
            .start_deed("castVote")
            .using(unlock.seal, svnum!(unlock.witness), &init_state)
            .reading(voting.addr)
            .append("_votes", ston!(voteId voting.vote_id.0, vote svenum!(vote), partyId party_id.0), None)
            .assign("signers", party.next_lock(), svnum!(party_id.0), None)
            .commit()
    }
}

fn main() {
    let codex = Dao::codex();
    codex
        .save("examples/dao.codex")
        .expect("unable to save issuer to a file");

    // Creating DAO with three participants
    let mut alice = PartySigner::new();
    let mut bob = PartySigner::new();
    let mut carol = PartySigner::new();
    let mut dao = Dao::create("WonderlandDAO", 1732529307)
        .add_party(alice.id, "alice", "Alice Wonderland")
        .add_party(bob.id, "alice", "Alice Wonderland")
        .add_party(carol.id, "alice", "Alice Wonderland")
        .finish();
    dao.save("examples/dao.contract");

    // Proposing vote
    let votings = deeds
        .start_deed("proposal")
        .append(
            "_votings",
            svnum!(100u64),
            Some(ston!(title "Is Alice on duty today?", text "Vote 'pro' if Alice should be on duty today")),
        )
        .commit();

    let init_state = deeds.effective_state();

    // Alice vote against her being on duty today
    deeds
        .start_deed("castVote")
        .using(fe128(0), svnum!(0u64), &init_state)
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(0u8), partyId 0u64), None)
        .assign("signers", fe128(10), svnum!(0u64), None)
        .commit();

    // Bob and Carol vote for Alice being on duty today
    deeds
        .start_deed("castVote")
        .using(fe128(1), svnum!(1u64), &init_state)
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(1u8), partyId 1u64), None)
        .assign("signers", fe128(11), svnum!(1u64), None)
        .commit();
    deeds
        .start_deed("castVote")
        .using(fe128(2), svnum!(2u64), &init_state)
        .reading(CellAddr::new(votings, 0))
        .append("_votes", ston!(voteId 100u64, vote svenum!(1u8), partyId 2u64), None)
        .assign("signers", fe128(12), svnum!(2u64), None)
        .commit();

    let post_voting_state = deeds.effective_state();
    //eprintln!("{post_voting_state:#x?}");
    let StrictVal::Map(votings) = post_voting_state.read("votings") else {
        panic!("invalid data")
    };
    let (_, first_voting) = votings.first().unwrap();
    println!("voting: {first_voting}");
    println!("Votes:");
    let StrictVal::Set(votes) = post_voting_state.read("votes") else {
        panic!("invalid data")
    };
    for vote in votes {
        println!("- {vote}");
    }

    // Now anybody accessing this file can figure out who is on duty today, by the decision of DAO.
    deeds
        .save("examples/dao.deeds")
        .expect("unable to save issuer to a file");
}

/// Party signer is a personal voting account which should be created and managed in private by
/// which DAO participant.
///
/// Here, for example purposes, we use a very simple setup where a lock is a hash value, and an
/// unlocking condition (giving the voting right under the participant id) is a preimage value.
pub struct PartySigner {
    id: PartyId,
    preimage: fe128,
}

impl PartySigner {
    fn digest(value: fe128) -> fe128 {
        let digest = Ripemd160::digest(value.0.to_le_bytes());
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&digest[..16]);
        fe128(u128::from_le_bytes(buf))
    }

    pub fn id(&self) -> PartyId { self.id }
    pub fn unlock(&self) -> Unlock { Unlock { seal: Self::digest(self.preimage), witness: self.preimage.0 } }
    pub fn curr_lock(&self) -> fe128 { Self::digest(self.preimage) }
    pub fn next_lock(&mut self) -> fe128 {
        self.preimage = Self::digest(fe128(random::<u128>()));
        self.curr_lock()
    }
}

pub struct Unlock {
    pub seal: fe128,
    pub witness: u128,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VotingRef {
    pub addr: CellAddr,
    pub vote_id: VoteId,
}

mod libs {
    use aluvm::{aluasm, Lib};

    pub fn success() -> Lib {
        let code = aluasm! {
            stop;
        };
        Lib::assemble(&code).unwrap()
    }

    pub fn cast_vote() -> Lib {
        // 1. Verify that there is just one referenced global state for the party and one for the voting
        // 2. Verify that referenced global state has a valid voteId matching the one provided in the
        //    operation
        // 3. Verify that referenced global state has a valid partyId matching the one provided in the
        //    operation
        // 4. Verify there is just one input
        // 5. Verify that the provided witness argument is a prehash of the input
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
    pub struct PartyId(pub u64);

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
    pub struct VoteId(pub u64);

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
        LibBuilder::new(libname!(LIB_NAME_DAO), tiny_bset! {
            std_stl().to_dependency(),
        })
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
