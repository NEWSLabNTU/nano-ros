---
id: 455
title: "CLI unit tests hand-rolled 22 scratch-path spellings; the differences between them were the bug"
status: resolved
type: bug
severity: low
area: cli, testing
related: [issue-0363, issue-0196]
resolved_in: phase-338
---

## Resolution

One helper, `nros-cli-core/src/test_support.rs`, is now the only way a unit
test in the crate names a scratch directory. Every call site goes through it:

```
$ git grep -o 'test_support::scratch_\(dir\|path\)' -- packages/cli/nros-cli-core/src | wc -l
26                    # across 19 files

$ git grep -n 'env::temp_dir' -- packages/cli/nros-cli-core/src
                      # (nothing)
```

The earlier passes fixed the sites that had already failed. This one deletes
the ability to spell it wrong.

## What the helper changes

A path is now `<base>/nros-cli-core-tests-<pid>/<tag>-<seq>`, and **uniqueness
does not depend on the clock**. The pid separates processes; a process-wide
atomic `SEQ` separates calls within one. That matters because the spellings it
replaces divided into four kinds, three of them collidable:

| shape | sites | collides when |
| --- | --- | --- |
| `{n}` nanosecond stamp only | 6 | two processes, any time |
| `{stamp}` nanosecond + nothing else | 3 | two runs in the same tick |
| no discriminator at all | 1 | always |
| `{tag}-{pid}-{stamp}` | 12 | (correct, but a 12th spelling) |

`CARGO_TARGET_TMPDIR` is still honoured as the base, but the pid segment is
appended **either way**. The three sites fixed in the earlier passes put the pid
only in the *fallback* — and cargo hands the same `CARGO_TARGET_TMPDIR` to every
run of a given test binary, so two concurrent runs of one integration test would
still have shared a path. That hole was latent only because `check-cli-tests`
runs `--lib`, where the variable is unset.

## The residual table in this issue was itself incomplete

The prior revision listed 9 remaining sites. Running its own prescribed sweep
found **11**: `orchestration/metadata_refresh.rs:451` (`nros-md-refresh-{name}`,
no pid) and `orchestration/sdk_store.rs:560` (`nros_store_{tag}_{n}`, nanos
only) were absent from the table.

That is the third time this issue reproduced the shape it documents — first
"both sites that carried this idiom" when there were three, then a 9-row
residual table when there were 11. Each pass fixed what it had looked at. The
structural answer is the one the issue itself prescribed and this change
finally applies: delete the second spelling rather than correct it.

**The prescribed sweep is also lossy**, which is part of why the table was
short:

```
git grep -n 'temp_dir()' -- packages/cli | grep -v 'process::id()'
```

`grep` is line-based, so a site that wraps `format!(` onto a second line shows
up as a hit even when the next line carries `std::process::id()`. Eight of the
sweep's hits were that false positive, which buries the two genuine ones. Read
the surrounding lines before trusting a row.

## Verification

- `cargo test -p nros-cli-core --lib` — **512 passed, 0 failed**.
- The issue's repro, three concurrent suites: 512/512 each, **no `Text file
  busy`**.
- `just check cli-tests` — **975 tests, 0 failures**.
- `cargo clippy -p nros-cli-core --lib --all-targets -- -D warnings` — clean.

Three unit tests in `test_support` pin the properties directly rather than
leaving them to a timing test: two calls with the same tag yield different
directories, the path carries a pid segment whether or not
`CARGO_TARGET_TMPDIR` is set, and `scratch_path` returns something that does not
exist yet.

## Original symptom (retained)

`just ci` red at `check-cli-tests`, one test of 490:

```
panicked at nros-cli-core/src/cmd/codegen_cyclonedds_descriptors.rs:447:10:
Caused by:
   0: spawn idlc at /tmp/nros-cli-core-tests/codegen_cyclonedds_descriptors_.../idlc
   1: Text file busy (os error 26)
```

Re-run solo: 3/3 pass. These tests write an executable stub and exec it, so a
concurrent run truncating the same path makes Linux refuse the exec. The quieter
mode hit every site: the helpers open with `remove_dir_all`, so one run deleted
another's scratch mid-test. Because the panic names whichever verb was running,
it read as a codegen regression rather than a harness defect — which is what
made it expensive to place.
