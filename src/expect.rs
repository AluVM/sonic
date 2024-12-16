// SONIC: Toolchain for formally-verifiable distributed contracts
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

use core::fmt::Display;
// TODO: Move to amplify

pub trait Unwrap: Expect {
    fn unwrap_or_panic(self) -> Self::Unwrap;
}

pub trait Expect {
    type Unwrap;
    fn expect_or<D: Display>(self, msg: D) -> Self::Unwrap;
    fn expect_or_else<D: Display>(self, f: impl FnOnce() -> D) -> Self::Unwrap;
}

impl<T, E: Display> Expect for Result<T, E> {
    type Unwrap = T;
    fn expect_or<D: Display>(self, msg: D) -> Self::Unwrap {
        self.unwrap_or_else(|err| panic!("Error: {msg}\nDetails: {err}"))
    }
    fn expect_or_else<D: Display>(self, f: impl FnOnce() -> D) -> Self::Unwrap {
        self.unwrap_or_else(|err| panic!("Error: {}\nDetails: {err}", f()))
    }
}

impl<T, E: Display> Unwrap for Result<T, E> {
    fn unwrap_or_panic(self) -> Self::Unwrap { self.unwrap_or_else(|err| panic!("Error: {err}")) }
}

impl<T> Expect for Option<T> {
    type Unwrap = T;
    fn expect_or<D: Display>(self, msg: D) -> Self::Unwrap { self.unwrap_or_else(|| panic!("Error: {msg}")) }
    fn expect_or_else<D: Display>(self, f: impl FnOnce() -> D) -> Self::Unwrap {
        self.unwrap_or_else(|| panic!("Error: {}", f()))
    }
}
