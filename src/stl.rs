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

use aluvm::stl::aluvm_stl;
use commit_verify::stl::commit_verify_stl;
use sonic_callreq::LIB_NAME_SONIC;
use sonicapi::{Articles, ArticlesId, Issuer, IssuerId};
use strict_types::stl::{std_stl, strict_types_stl};
use strict_types::typelib::LibBuilder;
use strict_types::{CompileError, TypeLib};
use ultrasonic::stl::finite_field_stl;
pub use ultrasonic::stl::usonic_stl;

use crate::Transition;

/// Strict types id for the library providing data types for RGB consensus.
pub const LIB_ID_SONIC: &str = "stl:FOnI~0yx-bcvd_Wq-~xjW37p-uqzQ3Nq-GGH8AtN-dO64ac8#river-atomic-dallas";

#[allow(clippy::result_large_err)]
fn _sonic_stl() -> Result<TypeLib, CompileError> {
    LibBuilder::with(libname!(LIB_NAME_SONIC), [
        std_stl().to_dependency_types(),
        strict_types_stl().to_dependency_types(),
        commit_verify_stl().to_dependency_types(),
        aluvm_stl().to_dependency_types(),
        finite_field_stl().to_dependency_types(),
        usonic_stl().to_dependency_types(),
    ])
    .transpile::<ArticlesId>()
    .transpile::<IssuerId>()
    .transpile::<Articles>()
    .transpile::<Issuer>()
    .transpile::<Transition>()
    .compile()
}

/// Generates a strict type library providing data types for RGB consensus.
pub fn sonic_stl() -> TypeLib { _sonic_stl().expect("invalid strict type SONIC library") }

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]
    use super::*;

    #[test]
    fn lib_id() {
        let lib = sonic_stl();
        assert_eq!(lib.id().to_string(), LIB_ID_SONIC);
    }
}
