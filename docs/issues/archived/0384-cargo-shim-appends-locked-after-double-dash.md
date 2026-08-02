---
id: 384
title: "The scripts/bin/cargo shim appends --locked at argv TAIL, so any `cargo <sub> -- <args>` leaks it to the child (test harness / clippy-driver): `error: Unrecognized option: 'locked'`"
status: resolved
type: bug
area: build
related: [issue-0359, issue-0378, issue-0379]
resolved_in: this commit
---

## Symptom

Under a sourced `activate.sh` (which puts `scripts/bin/cargo` on PATH), any
cargo subcommand that forwards args past `--` fails:

```
$ cargo test -p nros-tests --test launch_synth -- --nocapture
error: Unrecognized option: 'locked'

$ cargo clippy --workspace --all-targets -- -D warnings
error: Unrecognized option: 'locked'
```

The libtest harness / clippy-driver receives `--locked` as one of its own
arguments and rejects it.

## Root cause

`scripts/bin/cargo` (the project-wide `--locked` injector from issues 0359 /
0378) ends with:

```bash
exec "$real_cargo" "$@" $FLAGS      # FLAGS defaults to `--locked`
```

`$FLAGS` is appended at the **tail** of argv. Cargo's `--` separator means
"everything after this goes to the child binary" (the test harness, the run
target, clippy-driver). When the caller's `"$@"` contains a `--`, the appended
`--locked` lands *after* it and is handed to the child, which does not
understand it. The shim already guards the two adjacent cases — it skips
injection for subcommands that don't take the flag and when the caller already
passed `--locked/--frozen/--offline` — but it never accounts for the `--`
separator.

## Impact

- Every `cargo <sub> -- <args>` through the shim breaks: `-- --nocapture`,
  `-- --test-threads=N`, `-- -D warnings`, `cargo run … -- <app args>`.
- Issue 0379's new `check-cli-clippy` lane had to pass `--locked` *before* `--`
  as a workaround (which trips the "already asked for" guard into passthrough).
- Four runtime-clippy tests in
  `packages/cli/rosidl-codegen/tests/compilation_test.rs`
  (`test_clippy_no_warnings` and siblings, which run `cargo clippy … -- -W
  clippy::all`) fail whenever the suite runs under a sourced `activate.sh`.

## Fix

Insert `$FLAGS` **before the first `--`** in argv (they are cargo's own flags,
so they belong on cargo's side of the separator); with no `--`, append at the
end as before. Preserves every existing guard (subcommand allowlist,
already-present short-circuit, `+toolchain` handling, empty-`FLAGS`
passthrough).

Verified after the fix: `cargo test … -- --nocapture` and `cargo clippy … --
-D warnings` both run cleanly through the shim, and
`compilation_test.rs`'s clippy tests pass under `activate.sh`.
