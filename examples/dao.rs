#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

use amplify::confinement::{SmallString, TinyString};
use sonic::embedded::{EmbeddedArithm, EmbeddedImmutable, EmbeddedProc, EmbeddedReaders};
use sonic::{Api, ApiInner, AppendApi, DestructibleApi, Issuer, Private};
use strict_types::stl::std_stl;
use strict_types::{SemId, StrictVal, SymbolicSys, SystemBuilder, TypeSystem};
use ultrasonic::{fe128, Codex, Identity};

pub struct PartyId(u64);
pub struct Party {
    pub name: TinyString,
    pub identity: TinyString,
}
pub struct VoteId(u64);
pub enum Vote {
    Contra = 0,
    Pro = 1,
}
pub struct Voting {
    pub title: TinyString,
    pub text: SmallString,
}
pub struct CastVote {
    pub vote_id: VoteId,
    pub vote: Vote,
    pub party_id: PartyId,
}

enum VotePro {
    Pro = 1,
}
pub struct VoteProQuery {
    pub vote_id: VoteId,
    pub vote: VotePro,
}
enum VoteConter {
    Conter = 0,
}
pub struct VoteConterQuery {
    pub vote_id: VoteId,
    pub vote: VoteConter,
}
#[derive(Debug)]
pub struct DaoTypes(SymbolicSys);

impl Default for DaoTypes {
    fn default() -> Self { DaoTypes::new() }
}

impl DaoTypes {
    pub fn new() -> Self {
        Self(
            SystemBuilder::new()
                .import(std_stl())
                .unwrap()
                .finalize()
                .unwrap(),
        )
    }

    pub fn type_system(&self) -> TypeSystem { self.0.as_types().clone() }

    pub fn get(&self, name: &'static str) -> SemId {
        *self
            .0
            .resolve(name)
            .unwrap_or_else(|| panic!("type '{name}' is absent in standard RGBContract type library"))
    }
}

fn main() {
    let types = DaoTypes::new();

    let codex = Codex {
        name: Default::default(),
        developer: Default::default(),
        version: default!(),
        field_order: 0,
        input_config: Default::default(),
        verification_config: Default::default(),
        verifiers: Default::default(),
        reserved: default!(),
    };

    let api = Api::Embedded(ApiInner::<EmbeddedProc> {
        version: default!(),
        codex_id: codex.codex_id(),
        timestamp: chrono::Utc::now().timestamp(),
        name: None,
        developer: Identity::default(),
        append_only: tiny_bmap! {
            vname!("_parties") => AppendApi {
                sem_id: types.get("PartyId"),
                raw_sem_id: types.get("Party"),
                published: true,
                adaptor: EmbeddedImmutable(0),
            },
            vname!("_votings") => AppendApi {
                sem_id: types.get("VoteId"),
                raw_sem_id: types.get("Voting"),
                published: true,
                adaptor: EmbeddedImmutable(1),
            },
            vname!("_votes") => AppendApi {
                sem_id: types.get("CastVote"),
                raw_sem_id: SemId::unit(),
                published: true,
                adaptor: EmbeddedImmutable(2),
            },
        },
        destructible: tiny_bmap! {
            vname!("signers") => DestructibleApi {
                sem_id: types.get("PartyId"),
                arithmetics: EmbeddedArithm::NonFungible,
                adaptor: EmbeddedImmutable(0),
            }
        },
        readers: tiny_bmap! {
            vname!("parties") => EmbeddedReaders::MapF2S {
                name: vname!("_parties"),
                key: types.get("PartyId"),
                val: types.get("Party"),
            },
            vname!("votings") => EmbeddedReaders::MapF2S {
                name: vname!("_votings"),
                key: types.get("VoteId"),
                val: types.get("Voting"),
            },
            vname!("votes") => EmbeddedReaders::Set(vname!("_votes"), types.get("CastVote")),
            vname!("votingCount") => EmbeddedReaders::Count(vname!("votings")),
            vname!("totalVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("VoteId")),
            vname!("proVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("VoteProQuery")),
            vname!("conterVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("VoteCounterQuery")),
        },
        verifiers: tiny_bmap! {
            vname!("setup") => 0,
            vname!("proposal") => 1,
            vname!("castVote") => 2,
        },
        errors: Default::default(),
    });

    let issuer = Issuer::new(codex, api, [], types.type_system());
    // TODO: Save the issuer
    //issuer.save("ExampleDAO.cnt");

    let deeds = issuer
        .start_issue("setup")
        .add_immutable("_parties", svnum!(0), Some(ston!("me", "My Own Name")))
        .add_destructible("signers", fe128(0), svnum!(0), None)
        .finish::<Private>("ExampleDAO");
    // TODO: Save the deeds
    //deeds.save("ExampleDAO.cnt");
}
