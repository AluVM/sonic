// SONIC: Toolchain for formally-verifiable distributed contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2024 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
//
// Copyright (C) 2019-2025 LNP/BP Standards Association, Switzerland.
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

use std::path::PathBuf;

use sonic::AuthToken;

#[derive(Parser)]
pub enum Cmd {
    /// Issue a new HyperSONIC contract
    Issue {
        /// Schema used to issue the contract
        schema: PathBuf,

        /// Parameters and data for the contract
        params: PathBuf,

        /// Output file which will contain articles of the contract
        output: Option<PathBuf>,
    },

    /// Expand contract articles into a contract stock directory
    Expand { articles: PathBuf, stock: Option<PathBuf> },

    /// Print out a contract state
    State { stock: PathBuf },

    /// Make a contract call
    Call { stock: PathBuf, call: PathBuf },

    /// Export contract deeds to a file
    Export {
        stock: PathBuf,

        /// List of tokens of authority which should serve as a contract terminals.
        terminals: Vec<AuthToken>,

        output: PathBuf,
    },

    /// Accept deeds into a contract stock
    Accept { stock: PathBuf, input: PathBuf },
}
