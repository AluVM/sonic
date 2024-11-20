# SONIC: Toolchain for formally-verifiable distributed contracts

![Build](https://github.com/AluVM/sonic/workflows/Build/badge.svg)
![Tests](https://github.com/AluVM/sonic/workflows/Tests/badge.svg)
[![codecov](https://codecov.io/gh/AluVM/sonic/branch/master/graph/badge.svg)](https://codecov.io/gh/AluVM/sonic)

[![crates.io](https://img.shields.io/crates/v/sonic)](https://crates.io/crates/sonic)
[![Docs](https://docs.rs/sonic/badge.svg)](https://docs.rs/sonic)
[![License](https://img.shields.io/crates/l/sonic)](./LICENSE)

## What is it

**SONIC** is a toolchain for building partially-replicated state machines with capability-based
memory access, whose history execution trace can be compressed with recursive zk-STARKs (i.e. they
have a fixed-length computational integrity proofs).

What is capability-based memory access (or capability-addressable memory, **CAM**)? The computers we
all used to are random memory access machines (RAM), where a software accesses freely-addressable
global memory. This had opened a door for the all the vulnerabilities and hacks happening in
computer systems across the world for the past decades... CAM model instead, divides all memory into
parts (called *words*) addressable only with some access token (called *capability*). You may think
of this as of a memory where each part is "owned" by certain party, and can be accessed or modified
only given a proof of ownership. This is [**UltraSONIC**], the underlying layer for SONIC.

Now, you can put this into a distributed context, such that the memory is accessible by multiple
parties with different permissions, and the state of the computation (*state machine*) is replicable
across all the parties - so you get CAM upgraded into a *partially-replicated state machine*
(**PRISM**). SONIC takes such PRISM computers and enhances them with zk-STARKS, such that the cost
of replication becomes fixed, independently of how long the system is run for. With that, you have
a programs which can be formally (i.e. mathematically) verified to be safe, and at the same time
run over a computer network in a trustless manner, with the same efficiency no matter how long they
run.

## What I can build

One of the main current applications for SONIC are smart contracts made with client-side
validation, abstracted from a specific underlying blockchain or other consensus mechanism; however,
as you can see, the number of applications for SONIC can be significantly larger. Using SONIC, one
may build distributed software with capability-based access to the memory, including:

- a replicable database with formal safety guarantees;
- distributed operating system with formal safety guarantees;
- remote code execution environments (like in browsers, but with formal safety guarantees);
- blockchain or a zk-rollup;
- client-side validated smart contracts;
- or even distributed digital agents.

## Ecosystem

SONIC is a part of a larger ecosystem used to build safe distributed software, which includes:

- [Strict types]: strong type system made with [generalized algebraic data types][GADT] (*GADT*) and
  [dependent types];
- [AluVM]: a functional register-based virtual machine with a reduced instruction set (RISC); SONARE
  uses a zk-STARK-compatible subset of its instruction set architecture (called zk-AluVM);
- [UltraSONIC]: a transactional execution layer with capability-based memory access on top of
  zk-AluVM;
- [SONARE]: runtime environment for SONIC software;
- [Cation]: a general-purpose high-level programming language made with category theory, which
  features strict types, termination analysis and can be formally verified;
- [Contractum]: a domain-specific version of Cation for writing programs for SONARE.

## License

    Designed in 2019-2024 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
    Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@ubideco.org>
    
    Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
    Copyright (C) 2024-2025 Laboratories for Ubiquitous Deterministic Computing (UBIDECO),
                            Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
    Copyright (C) 2019-2025 Dr Maxim Orlovsky.
    All rights under the above copyrights are reserved.

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
in compliance with the License. You may obtain a copy of the License at
<http://www.apache.org/licenses/LICENSE-2.0>.

Unless required by applicable law or agreed to in writing, software distributed under the License
is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
or implied. See the License for the specific language governing permissions and limitations under
the License.

[Strict types]: https://strict-types.org

[AluVM]: https://aluvm.org

[UltraSONIC]: https://github.com/AluVM/UltraSONIC

[**UltraSONIC**]: https://github.com/AluVM/UltraSONIC

[Cation]: https://cation-lang.org

[Contractum]: https://contractum.org

[GADT]: https://en.wikipedia.org/wiki/Generalized_algebraic_data_type

[dependent types]: https://en.wikipedia.org/wiki/Dependent_type
