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

use std::convert::Infallible;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

use clap::ValueHint;
use hypersonic::{AuthToken, CallParams, Identity, IssueParams, Issuer, SigBlob, SigValidator};
use sonic_persist_fs::LedgerDir;

use crate::dump::dump_ledger;

#[derive(Parser)]
pub enum Cmd {
    /// Issue a new SONIC contract
    Issue {
        /// Issuer used to issue the contract
        issuer: PathBuf,

        /// Parameters and data for the contract
        params: PathBuf,

        /// Output contract directory
        output: Option<PathBuf>,
    },

    /// Print out a contract state
    State {
        /// Contract directory
        dir: PathBuf,
    },

    /// Make a contract call
    Call {
        /// Contract directory
        dir: PathBuf,
        /// Parameters and data for the call
        call: PathBuf,
    },

    /// Export contract deeds to a file
    Export {
        /// Contract directory
        dir: PathBuf,

        /// List of authority tokens which should serve as contract terminals.
        #[clap(short, long)]
        terminals: Vec<AuthToken>,

        /// Location to save the deed file to
        output: PathBuf,
    },

    /// Accept deeds into a contract
    Accept {
        /// Contract directory
        dir: PathBuf,

        /// File with deeds to accept
        input: PathBuf,
    },

    /// Dump ledger data into multiple debug files
    Dump {
        /// Remove the destination directory if it already exists
        #[clap(short, long, global = true)]
        force: bool,

        /// Source data to process
        #[clap(value_hint = ValueHint::FilePath)]
        src: PathBuf,

        /// Destination directory to put dump files
        ///
        /// If skipped, adds the `dump` subdirectory to the `src` path.
        #[clap(value_hint = ValueHint::FilePath)]
        dst: Option<PathBuf>,
    },
}

impl Cmd {
    pub fn exec(self) -> anyhow::Result<()> {
        match self {
            Cmd::Issue { issuer, params, output } => issue(issuer, params, output)?,
            Cmd::State { dir } => state(dir)?,
            Cmd::Call { dir, call: path } => call(dir, path)?,
            Cmd::Export { dir, terminals, output } => export(dir, terminals, output)?,
            Cmd::Accept { dir, input } => accept(dir, input)?,
            Cmd::Dump { force, src, dst } => dump(force, src, dst)?,
        }
        Ok(())
    }
}

fn issue(issuer_file: PathBuf, form: PathBuf, output: Option<PathBuf>) -> anyhow::Result<()> {
    let issuer = Issuer::load(issuer_file)?;
    let file = File::open(&form)?;
    let params = serde_yaml::from_reader::<_, IssueParams>(file)?;

    let path = output.unwrap_or(form);
    let output = path
        .with_file_name(params.name.as_str())
        .with_extension("contract");

    let articles = issuer.issue(params);
    LedgerDir::new(articles, output)?;

    Ok(())
}

fn state(path: PathBuf) -> anyhow::Result<()> {
    let ledger = LedgerDir::load(path)?;
    let val = serde_yaml::to_string(&ledger.state().main)?;
    println!("{val}");
    Ok(())
}

fn call(dir: PathBuf, form: PathBuf) -> anyhow::Result<()> {
    let mut ledger = LedgerDir::load(dir)?;
    let file = File::open(form)?;
    let call = serde_yaml::from_reader::<_, CallParams>(file)?;
    let opid = ledger.call(call)?;
    println!("Operation ID: {opid}");
    Ok(())
}

fn export(dir: PathBuf, terminals: impl IntoIterator<Item = AuthToken>, output: PathBuf) -> anyhow::Result<()> {
    let mut ledger = LedgerDir::load(dir)?;
    ledger.export_to_file(terminals, output)?;
    Ok(())
}

fn accept(dir: PathBuf, input: PathBuf) -> anyhow::Result<()> {
    // TODO: (v0.13) Use some real signature validator
    pub struct DumbValidator;
    impl SigValidator for DumbValidator {
        fn validate_sig(&self, _: impl Into<[u8; 32]>, _: &Identity, _: &SigBlob) -> Result<u64, impl Error> {
            Result::<_, Infallible>::Ok(0)
        }
    }
    let mut ledger = LedgerDir::load(dir)?;
    ledger.accept_from_file(input, DumbValidator)?;
    Ok(())
}

fn dump(force: bool, src: PathBuf, dst: Option<PathBuf>) -> anyhow::Result<()> {
    match src.extension() {
        Some(ext) if ext == "contract" => {
            let dst = dst
                .as_ref()
                .map(|p| p.to_owned())
                .unwrap_or_else(|| src.join("dump"));
            dump_ledger(&src, dst, force).inspect_err(|_| println!())?;
            Ok(())
        }
        Some(_) => Err(anyhow!("Can't detect the type for '{}': the extension is not recognized", src.display())),
        None => Err(anyhow!("The path '{}' can't be recognized as known data", src.display())),
    }
}
