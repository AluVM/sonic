#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

use amplify::confinement::{SmallString, TinyString};
use sonic::embedded::{EmbeddedAdaptors, EmbeddedArithm, EmbeddedProc, EmbeddedReaders, Source};
use sonic::{Api, ApiInner, AppendApi, CollectionType, DestructibleApi};
use strict_types::stl::std_stl;
use strict_types::{SemId, SymbolicSys, SystemBuilder, TypeSystem};
use ultrasonic::Codex;

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
        api_version: 0,
        name: None,
        developer: tiny_s!("ssi:anonymous"),
        append_only: tiny_bmap! {
            vname!("parties") => AppendApi {
                published: true,
                collection: CollectionType::Map {
                    key: types.get("PartyId"),
                    val: types.get("Party"),
                },
                adaptor: EmbeddedAdaptors::Map { key: Source::FieldElements, val: Source::AssociatedData },
                builder: EmbeddedAdaptors::Map { key: Source::FieldElements, val: Source::AssociatedData },
            },
            vname!("votings") => AppendApi {
                published: true,
                collection: CollectionType::Map {
                    key: types.get("VoteId"),
                    val: types.get("Voting"),
                },
                adaptor: EmbeddedAdaptors::Map { key: Source::FieldElements, val: Source::AssociatedData },
                builder: EmbeddedAdaptors::Map { key: Source::FieldElements, val: Source::AssociatedData },
            },
            vname!("votes") => AppendApi {
                published: true,
                collection: CollectionType::Set(types.get("CastVote")),
                adaptor: EmbeddedAdaptors::BytesFrom(Source::FieldElements),
                builder: EmbeddedAdaptors::BytesFrom(Source::FieldElements),
            },
        },
        destructible: tiny_bmap! {
            vname!("signers") => DestructibleApi {
                sem_id: types.get("PartyId"),
                arithmetics: EmbeddedArithm::NonFungible,
                adaptor: EmbeddedAdaptors::BytesFrom(Source::FieldElements),
                builder: EmbeddedAdaptors::BytesFrom(Source::FieldElements),
            }
        },
        readers: tiny_bmap! {
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
}
