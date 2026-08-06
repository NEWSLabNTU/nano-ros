---
id: 455
title: "CLI unit tests share a FIXED `/tmp` scratch path, so two concurrent runs race — one exec's a stub while the other truncates it (`Text file busy`)"
status: open
type: bug
severity: low
area: cli, testing
related: [issue-0363]
---

## Symptom

`just ci` red at `check-cli-tests`, one test of 490:

```
---- cmd::codegen_cyclonedds_descriptors::tests::codegen_cyclonedds_emits_std_msgs stdout ----
panicked at nros-cli-core/src/cmd/codegen_cyclonedds_descriptors.rs:447:10:
verb runs: emit cyclonedds descriptors

Caused by:
   0: spawn idlc at /tmp/nros-cli-core-tests/codegen_cyclonedds_descriptors_emits_c_for_std_msgs_int32/idlc
   1: Text file busy (os error 26)

test result: FAILED. 489 passed; 1 failed
```

Re-run solo: **3/3 pass**. It only fails under load.

## Cause

`scratch_dir` picked its base like this:

```rust
let base = std::env::var_os("CARGO_TARGET_TMPDIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::temp_dir().join("nros-cli-core-tests"));
```

`CARGO_TARGET_TMPDIR` is set for **integration** tests only. `check-cli-tests`
runs these as `--lib`, so the variable is absent and every run took the
fallback — a **fixed** path with nothing run-specific in it. It is therefore
shared by every concurrent run on the host: a second checkout, a parallel
agent session, CI beside a local run.

Two failure modes follow from the sharing:

1. **`ETXTBSY`** (the observed one). These tests write an executable stub
   (`idlc`) and then exec it. Run A exec's the stub while run B opens the same
   path for writing — Linux refuses to exec a file open for write.
2. **Scratch deletion.** `scratch_dir` starts with `remove_dir_all(&dir)`, so
   run B deletes run A's scratch mid-test.

The failure names the codegen verb, so it reads as a codegen regression rather
than a test-harness defect — which is what made it expensive to place.

## Fix (partial, landed)

Scope the fallback to the process:

```rust
.unwrap_or_else(|| {
    std::env::temp_dir().join(format!("nros-cli-core-tests-{}", std::process::id()))
});
```

Applied to **three** sites carrying this exact idiom — all sharing the same
`nros-cli-core-tests` base name, so they could collide with each other as well
as across runs:

- `nros-cli-core/src/cmd/codegen_cyclonedds_descriptors.rs`
- `nros-cli-core/src/cmd/codegen_system.rs`
- `nros-cli-core/src/orchestration/nros_config.rs` — **added 2026-08-06.** The
  first pass said "both sites that carried this exact idiom" and there were
  three; the third is byte-identical, down to the base name and the opening
  `remove_dir_all`. It was found by running the sweep this issue already
  prescribes below, which is the whole point of writing the sweep down: a fix
  that lands on the sites you happened to look at is the issue-0196 shape, and
  this issue reproduced it in its own first pass.

## Residual — the wider class

Most other CLI test scratch paths already carry `std::process::id()`
(`abi_guard.rs`, `bringup.rs`, `check.rs`, `check_workspace.rs`,
`scaffold_deploy.rs:225`, `emit_package_xml.rs`, `new_system.rs`,
`ament.rs`). These do **not**, and are the remaining audit:

| site | path |
| --- | --- |
| `cmd/doctor.rs:511,557,589,667` | `nros_doctor_deploy_{n}`, `nros_gate_{n}`, `nros_gate_fvp_{n}`, `nros_gate_unk_{n}` |
| `cmd/setup.rs:1764,1782` | `nros_idx_{n}`, `nros_noidx_{n}` |
| `cmd/setup.rs:1825` | `nros-source-build-names-test` (no counter at all) |
| `cmd/scaffold_deploy.rs:364,380` | `nros-scaffold-noboot-{stamp}`, `nros-scaffold-multi-{stamp}` (time-based; two runs in the same tick collide) |

None of these are known to exec a file they wrote, so `ETXTBSY` is unlikely
there — but the scratch-deletion mode applies to all of them.

Per the "one shared helper, not a second spelling" rule, the right shape is a
single test-support helper (one `scratch_root()` in a shared `testing` module)
that every `scratch_dir` calls, rather than nine hand-written spellings of the
same `temp_dir().join(...)` — which is how two of them ended up sharing a base
name. That refactor is not done here.

Sweep to re-check the class:

```
git grep -n 'temp_dir()' -- packages/cli | grep -v 'process::id()'
```

## Repro

Run the lib tests twice concurrently from two checkouts (or two shells):

```
cargo test --manifest-path packages/cli/Cargo.toml -p nros-cli-core --lib &
cargo test --manifest-path packages/cli/Cargo.toml -p nros-cli-core --lib
```

Before the fix this raced on `/tmp/nros-cli-core-tests/`; after, each process
owns `/tmp/nros-cli-core-tests-<pid>/`.
