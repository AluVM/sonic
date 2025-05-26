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

use core::str::FromStr;

use aluvm::LibSite;
use strict_types::value::StrictNum;
use strict_types::StrictVal;

use crate::LIB_NAME_SONIC;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_SONIC, tags = custom)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum StateArithm {
    #[strict_type(tag = 0x00, dumb)]
    Fungible,

    #[strict_type(tag = 0x01)]
    NonFungible,
    // In the future more arithmetics can be added.
    /// Execute a custom function.
    #[strict_type(tag = 0xFF)]
    AluVM(
        /// The entry point to the script (virtual machine uses libraries from
        /// [`crate::Semantics`]).
        LibSite,
    ),
}

impl StateArithm {
    pub fn calculator(&self) -> StateCalc {
        match self {
            Self::Fungible => StateCalc::Fungible(StrictVal::Number(StrictNum::Uint(0))),
            Self::NonFungible => StateCalc::NonFungible(vec![]),
            Self::AluVM(_) => StateCalc::AluVM,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display, Error)]
#[display(doc_comments)]
pub enum StateCalcError {
    /// integer overflow during state computation.
    Overflow,

    /// state cannot be computed.
    UncountableState,

    /// AluVM is not yet supported for the state arithmetics.
    Unsupported,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum StateCalc {
    NonFungible(Vec<StrictVal>),
    Fungible(StrictVal),
    // AluVM is reserved for the future. We need it here to avoid breaking changes.
    AluVM,
}

impl StateCalc {
    pub fn accumulate(&mut self, state: &StrictVal) -> Result<(), StateCalcError> {
        match self {
            Self::NonFungible(states) => {
                states.push(state.clone());
                Ok(())
            }
            Self::Fungible(value) => {
                let (val, add) = match (state, value) {
                    // TODO: Use `if let` guards to avoid `unwrap` once rust supports them
                    (StrictVal::String(s), StrictVal::Number(StrictNum::Uint(val))) if u64::from_str(s).is_ok() => {
                        let add = u64::from_str(s).unwrap();
                        (val, add)
                    }
                    (StrictVal::Number(StrictNum::Uint(add)), StrictVal::Number(StrictNum::Uint(val))) => (val, *add),
                    _ => return Err(StateCalcError::UncountableState),
                };
                *val = val.checked_add(add).ok_or(StateCalcError::Overflow)?;
                Ok(())
            }
            Self::AluVM => Err(StateCalcError::Unsupported),
        }
    }

    pub fn lessen(&mut self, state: &StrictVal) -> Result<(), StateCalcError> {
        match self {
            Self::NonFungible(states) => {
                if let Some(pos) = states.iter().position(|s| s == state) {
                    states.remove(pos);
                    Ok(())
                } else {
                    Err(StateCalcError::UncountableState)
                }
            }
            Self::Fungible(value) => {
                let (val, dec) = match (state, value) {
                    // TODO: Use `if let` guards to avoid `unwrap` once rust supports them
                    (StrictVal::String(s), StrictVal::Number(StrictNum::Uint(val))) if u64::from_str(s).is_ok() => {
                        let dec = u64::from_str(s).unwrap();
                        (val, dec)
                    }
                    (StrictVal::Number(StrictNum::Uint(dec)), StrictVal::Number(StrictNum::Uint(val))) => (val, *dec),
                    _ => return Err(StateCalcError::UncountableState),
                };
                if dec > *val {
                    return Err(StateCalcError::Overflow);
                }
                *val -= dec;
                Ok(())
            }
            Self::AluVM => Err(StateCalcError::Unsupported),
        }
    }

    pub fn diff(&self) -> Result<Vec<StrictVal>, StateCalcError> {
        Ok(match self {
            Self::NonFungible(items) => items.clone(),
            Self::Fungible(value) => match value {
                StrictVal::Number(StrictNum::Uint(val)) => {
                    if val.eq(&u64::MIN) {
                        vec![]
                    } else {
                        vec![value.clone()]
                    }
                }
                _ => return Err(StateCalcError::UncountableState),
            },
            Self::AluVM => return Err(StateCalcError::Unsupported),
        })
    }

    pub fn is_satisfied(&self, target: &StrictVal) -> bool {
        match self {
            Self::NonFungible(items) => items.contains(target),
            Self::Fungible(value) => {
                if value == target {
                    true
                } else if let StrictVal::Number(StrictNum::Uint(val)) = value {
                    if let StrictVal::Number(StrictNum::Uint(tgt)) = target {
                        val >= tgt
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Self::AluVM => false,
        }
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]
    use super::*;

    #[test]
    fn arithm_fungible() {
        let mut calc = StateArithm::Fungible.calculator();
        let mut acc = 0u64;
        for n in 0..5u64 {
            calc.accumulate(&svnum!(n)).unwrap();
            acc += n;
        }
        assert_eq!(calc.diff().unwrap(), [svnum!(acc)]);
        assert!(calc.is_satisfied(&svnum!(acc)));
        assert!(calc.is_satisfied(&svnum!(acc - 1)));
        assert!(!calc.is_satisfied(&svnum!(acc + 1)));

        for n in 0..2u64 {
            calc.lessen(&svnum!(n)).unwrap();
            acc -= n;
        }

        assert_eq!(calc.diff().unwrap(), [svnum!(acc)]);
        assert!(calc.is_satisfied(&svnum!(acc)));
        assert!(calc.is_satisfied(&svnum!(acc - 1)));
        assert!(!calc.is_satisfied(&svnum!(acc + 1)));
    }

    #[test]
    fn arithm_nonfungible() {
        let mut calc = StateArithm::NonFungible.calculator();
        for n in 0..5u64 {
            calc.accumulate(&svnum!(n)).unwrap();
        }
        assert_eq!(calc.diff().unwrap(), [svnum!(0u64), svnum!(1u64), svnum!(2u64), svnum!(3u64), svnum!(4u64)]);
        assert!(calc.is_satisfied(&svnum!(0u64)));
        assert!(calc.is_satisfied(&svnum!(1u64)));
        assert!(calc.is_satisfied(&svnum!(2u64)));
        assert!(calc.is_satisfied(&svnum!(3u64)));
        assert!(calc.is_satisfied(&svnum!(4u64)));
        assert!(!calc.is_satisfied(&svnum!(5u64)));

        for n in 0..2u64 {
            calc.lessen(&svnum!(n)).unwrap();
        }

        assert_eq!(calc.diff().unwrap(), [svnum!(2u64), svnum!(3u64), svnum!(4u64)]);
        assert!(!calc.is_satisfied(&svnum!(0u64)));
        assert!(!calc.is_satisfied(&svnum!(1u64)));
        assert!(calc.is_satisfied(&svnum!(2u64)));
        assert!(calc.is_satisfied(&svnum!(3u64)));
        assert!(calc.is_satisfied(&svnum!(4u64)));
        assert!(!calc.is_satisfied(&svnum!(5u64)));
    }
}
