---
id: 1222
title: "`check-fixture-binary-names` scans `tests/*.rs` only, so the binary name a
  RESOLVER passes is unchecked — and the ThreadX RV64 leaves have three spellings"
status: resolved
type: bug
area: testing, tooling
found: 2026-09-08
related: [0556, 0720, 0743]
---

# The gate looks at the callers and not at the resolvers

`scripts/check-fixture-binary-names.py:118` globs exactly one directory:

```python
for src in sorted((ROOT / "packages/testing/nros-tests/tests").glob("*.rs")):
```

So a binary name written in a TEST file is checked, and the same name written in
`packages/testing/nros-tests/src/fixtures/binaries/**` — where the resolvers
that actually build the path live — is not.

## What that leaves unchecked, measured

`packages/testing/nros-tests/src/fixtures/binaries/threadx_riscv64.rs:130` and
the five rows beside it:

```rust
build_rust_example("talker", "qemu-riscv64-threadx-talker")
```

Three spellings exist for one artifact:

| spelling | where | what it is |
| --- | --- | --- |
| `qemu-riscv64-threadx-talker` | resolver's `binary_name` arg | the CARGO PACKAGE name |
| `qemu_riscv64_threadx_talker` | the leaf's `[lib] name` | the staticlib |
| `riscv64_threadx_rust_talker` | `tests/threadx_riscv64_qemu.rs:250` | what the test greps |

The leaf declares `[lib]` and no `[[bin]]`, so the final ELF is linked by CMake
under the third name. The gate sees only the third, because it is the one in a
`tests/` file.

## Why this matters more than a naming nit

Its own file records the last time this class bit: issue 0556's comment above
`build_rust_example` says the resolver read a leaf tree the fixture build had
stopped writing MONTHS earlier, and *"both `rtos_e2e` ThreadxRiscv64 cases read
as failures for it, and looked like flaky QEMU."* A wrong `binary_name` produces
a path no target writes, and a fixture that resolves nowhere is a STALE verdict
— absorbing, and indistinguishable from a red cell (issue 0445).

## Not verified

Whether `qemu-riscv64-threadx-talker` currently resolves to nothing was NOT
measured — it needs a ThreadX RV64 fixture build, which the tree could not
afford when this was found. What IS measured is the gate's scope and the three
spellings. If the resolver is in fact correct, the finding is only the coverage
gap; if it is not, four `rtos_e2e` cases have been skipping or failing for a
reason nobody attributed.

## The fix

Extend the glob to `src/fixtures/binaries/**/*.rs`. That is where #393's rule
("move the test-side locator in the SAME commit as the build-side path") is
actually enforced or not, and it is the half the gate cannot currently see.

Found during phase-437 W4 while renaming this board; the rename carried the
spellings across unchanged rather than silently repairing them, so this issue
describes the state before and after.

## RESOLVED (2026-09-08) — and the suspected half was real

The gate scans `src/fixtures/binaries/**/*.rs` as well as `tests/*.rs` now. That
alone was not enough, and the reason is the interesting part:

**The literals live one frame up, in a LOCAL WRAPPER.**
`threadx_riscv64.rs` defines `fn build_rust_example(name, binary_name)` whose
body is a single call to `build_threadx_rv64_rust_example_rmw(name,
binary_name, Rmw::Zenoh)`. The resolver therefore sees VARIABLES, and the gate
— which keys on resolver names — counted six real call sites as "non-literal,
not checked" and printed OK. Widening the glob changed nothing.

A wrapper is recognised STRUCTURALLY, never listed: a `fn` in the same file
whose body calls a known resolver. A registry of wrapper names would be a
second list to drift, which is the defect one level up.

With that, the gate FAILED on six sites:

    build_rust_example asks for "qemu-riscv64-threadx-talker", which
    examples/qemu-riscv64-threadx/rust/talker/CMakeLists.txt does not declare.
    It declares: riscv64_threadx_rust_talker

So the half this issue could not verify without a build is answered: **all six
ThreadX RV64 rust resolvers asked for the cargo PACKAGE name, and CMake declares
a different target.** Every one resolved to a path no target writes, which the
caller turns into `skip!("fixture missing")` — the exact shape issue 0720 was
written for, in the file whose own comment records 0556's instance of it.

Fixed at the six call sites (`riscv64_threadx_rust_<role>`), and
negative-controlled: putting one package name back makes the gate name it and
exit 1.

`check-fixture-binary-names`: 22 call sites checked, up from 9.
