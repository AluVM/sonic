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

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::path::Path;

use anyhow::Context;
use hypersonic::{Articles, CellAddr, Opid};
use serde::{Deserialize, Serialize};
use sonic_persist_fs::LedgerDir;

pub fn dump_articles(articles: &Articles, dst: &Path) -> anyhow::Result<Opid> {
    let genesis_opid = articles.issue.genesis_opid();
    let out = File::create_new(dst.join(format!("0000-genesis-{genesis_opid}.yaml")))
        .context("can't create dump files; try to use the `--force` flag")?;
    serde_yaml::to_writer(&out, &articles.issue.genesis)?;

    let out = File::create_new(dst.join("meta.yaml"))?;
    serde_yaml::to_writer(&out, &articles.issue.meta)?;

    let out = File::create_new(dst.join(format!("codex-{:#}.yaml", articles.issue.codex.codex_id())))?;
    serde_yaml::to_writer(&out, &articles.issue.codex)?;

    let out = File::create_new(dst.join("api-default.yaml"))?;
    serde_yaml::to_writer(&out, &articles.default_api)?;

    for api in &articles.custom_apis {
        let out = File::create_new(dst.join(format!("api-{}.yaml", api.name().expect("invalid api"))))?;
        serde_yaml::to_writer(&out, &api)?;
    }

    // TODO: Process type system
    // TODO: Process AluVM libraries

    Ok(genesis_opid)
}

#[derive(Clone, Debug, Default)]
#[derive(Serialize, Deserialize)]
pub struct OpLinks {
    pub readers: BTreeMap<u16, Opid>,
    pub spenders: BTreeMap<u16, Opid>,
}

pub fn dump_ledger(src: impl AsRef<Path>, dst: impl AsRef<Path>, force: bool) -> anyhow::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if force {
        let _ = fs::remove_dir_all(dst);
    }
    fs::create_dir_all(dst)?;

    print!("Reading contract ledger from '{}' ... ", src.display());
    let path = src.to_path_buf();
    let ledger = LedgerDir::load(path)?;
    println!("success reading {}", ledger.contract_id());

    print!("Processing contract articles ... ");
    let articles = ledger.articles();
    dump_articles(articles, dst)?;
    println!("success");

    print!("Processing operations ... none found");
    for (no, (opid, op)) in ledger.operations().enumerate() {
        let out = File::create_new(dst.join(format!("{:04}-op-{opid}.yaml", no + 1)))?;
        serde_yaml::to_writer(&out, &op)?;
        let out = File::create_new(dst.join(format!("{:04}-links-{opid}.yaml", no + 1)))?;
        let mut links = OpLinks::default();
        for no in 0..op.immutable_out.len_u16() {
            links.readers.extend(
                ledger
                    .read_by(CellAddr::new(opid, no))
                    .map(|child| (no, child)),
            );
        }
        for no in 0..op.destructible_out.len_u16() {
            let Some(child) = ledger.spent_by(CellAddr::new(opid, no)) else {
                continue;
            };
            links.spenders.insert(no, child);
        }
        serde_yaml::to_writer(&out, &links)?;
        print!("\rProcessing operations ... {} processed", no + 1);
    }
    println!();

    print!("Processing trace ... none state transitions found");
    for (no, (opid, st)) in ledger.trace().enumerate() {
        let out = File::create_new(dst.join(format!("{:04}-trace-{opid}.yaml", no + 1)))?;
        serde_yaml::to_writer(&out, &st)?;
        print!("\rProcessing trace ... {} state transition processed", no + 1);
    }
    println!();

    print!("Processing state ... ");
    let state = ledger.state();

    let out = File::create_new(dst.join("state-default.yaml"))?;
    serde_yaml::to_writer(&out, &state.main)?;
    let out = File::create_new(dst.join("state-raw.yaml"))?;
    serde_yaml::to_writer(&out, &state.raw)?;
    for (name, state) in &state.aux {
        let out = File::create_new(dst.join(format!("state-{name}.yaml")))?;
        serde_yaml::to_writer(&out, state)?;
    }
    println!("success");

    Ok(())
}
