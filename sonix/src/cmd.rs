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

use std::fs::File;
use std::path::{Path, PathBuf};

use sonic::{Articles, AuthToken, CallParams, IssueParams, Private, Schema, Stock};
use strict_encoding::{StreamWriter, StrictWriter};

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

    /// Process contract articles into a contract stock directory
    Process {
        /// Contract articles to process
        articles: PathBuf,
        /// Directory to put the contract stock directory inside
        stock: Option<PathBuf>,
    },

    /// Print out a contract state
    State {
        /// Contract stock directory
        stock: PathBuf,
    },

    /// Make a contract call
    Call {
        /// Contract stock directory
        stock: PathBuf,
        /// Parameters and data for the call
        call: PathBuf,
    },

    /// Export contract deeds to a file
    Export {
        /// Contract stock directory
        stock: PathBuf,

        /// List of tokens of authority which should serve as a contract terminals.
        terminals: Vec<AuthToken>,

        /// Location to save the deeds file to
        output: PathBuf,
    },

    /// Accept deeds into a contract stock
    Accept {
        /// Contract stock directory
        stock: PathBuf,

        /// File with deeds to accept
        input: PathBuf,
    },
}

impl Cmd {
    pub fn exec(&self) -> anyhow::Result<()> {
        match self {
            Cmd::Issue { schema, params, output } => issue(schema, params, output.as_deref())?,
            Cmd::Process { articles, stock } => process(articles, stock.as_deref())?,
            Cmd::State { stock } => state(stock),
            Cmd::Call { stock, call: path } => call(stock, path)?,
            Cmd::Export { stock, terminals, output } => export(stock, terminals, output)?,
            Cmd::Accept { .. } => todo!(),
        }
        Ok(())
    }
}

fn issue(schema: &Path, form: &Path, output: Option<&Path>) -> anyhow::Result<()> {
    let schema = Schema::load(schema)?;
    let file = File::open(form)?;
    let params = serde_yaml::from_reader::<_, IssueParams>(file)?;

    let path = output.unwrap_or(form);
    let output = path.with_file_name(&format!("{}.articles", params.name));

    let articles = schema.issue::<Private>(params);
    articles.save(output)?;

    Ok(())
}

fn process(articles: &Path, stock: Option<&Path>) -> anyhow::Result<()> {
    let path = stock.unwrap_or(articles);

    let articles = Articles::<Private>::load(articles)?;
    Stock::new(articles, path);

    Ok(())
}

fn state(path: &Path) {
    let stock = Stock::<Private, _>::load(path);
    let val = serde_yaml::to_string(&stock.state().main).expect("unable to generate YAML");
    println!("{val}");
}

fn call(stock: &Path, form: &Path) -> anyhow::Result<()> {
    let mut stock = Stock::<Private, _>::load(stock);
    let file = File::open(form)?;
    let call = serde_yaml::from_reader::<_, CallParams>(file)?;
    let opid = stock.call(call);
    println!("Operation ID: {opid}");
    Ok(())
}

fn export<'a>(stock: &Path, terminals: impl IntoIterator<Item = &'a AuthToken>, output: &Path) -> anyhow::Result<()> {
    let mut stock = Stock::<Private, _>::load(stock);
    let file = File::create_new(output)?;
    let writer = StrictWriter::with(StreamWriter::new::<{ usize::MAX }>(file));
    stock.export(terminals, writer)?;
    Ok(())
}
