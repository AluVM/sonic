#[macro_use]
extern crate amplify;
#[macro_use]
extern crate strict_types;

use sonic::embedded::{EmbeddedArithm, EmbeddedImmutable, EmbeddedProc, EmbeddedReaders};
use sonic::{Api, ApiInner, AppendApi, DestructibleApi, Issuer, Private};
use strict_types::SemId;
use ultrasonic::{fe128, Codex, Identity};

mod dao {
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

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct CastVote {
        pub vote_id: VoteId,
        pub vote: Vote,
        pub party_id: PartyId,
    }

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(lowercase)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO, tags = repr, try_from_u8, into_u8)]
    #[repr(u8)]
    enum Pro {
        #[strict_type(dumb)]
        Pro = 1,
    }

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display(lowercase)]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO, tags = repr, try_from_u8, into_u8)]
    #[repr(u8)]
    enum Conter {
        #[strict_type(dumb)]
        Conter = 0,
    }

    pub trait Query: StrictDumb + StrictEncode + StrictDecode {}
    impl Query for Pro {}
    impl Query for Conter {}

    #[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, From)]
    #[display("")]
    #[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
    #[strict_type(lib = LIB_NAME_DAO)]
    pub struct VoteQuery<Q: Query> {
        pub vote_id: VoteId,
        pub vote: Q,
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
        .transpile::<VoteQuery<Pro>>()
        .transpile::<VoteQuery<Conter>>()
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

fn main() {
    let types = dao::DaoTypes::new();

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
            vname!("parties") => EmbeddedReaders::MapF2S {
                name: vname!("_parties"),
                key: types.get("DAO.PartyId"),
                val: types.get("DAO.Party"),
            },
            vname!("votings") => EmbeddedReaders::MapF2S {
                name: vname!("_votings"),
                key: types.get("DAO.VoteId"),
                val: types.get("DAO.Voting"),
            },
            vname!("votes") => EmbeddedReaders::Set(vname!("_votes"), types.get("DAO.CastVote")),
            vname!("votingCount") => EmbeddedReaders::Count(vname!("votings")),
            vname!("totalVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("DAO.VoteId")),
            vname!("proVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("DAO.VoteQueryPro")),
            vname!("conterVotes") => EmbeddedReaders::CountPrefixed(vname!("votings"), types.get("DAO.VoteQueryConter")),
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
    issuer
        .save("examples/dao.cdx")
        .expect("unable to save issuer to a file");

    let deeds = issuer
        .start_issue("setup")
        .add_immutable("_parties", svnum!(0u64), Some(ston!(name "me", identity "My Own Name")))
        .add_destructible("signers", fe128(0), svnum!(0u128), None)
        .finish::<Private>("ExampleDAO");
    // TODO: Save the deeds
    deeds
        .save("examples/dao.deed")
        .expect("unable to save issuer to a file");
}
