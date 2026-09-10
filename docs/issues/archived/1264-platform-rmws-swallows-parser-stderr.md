---
id: 1264
title: "`nros_platform_rmws` redirects the manifest parser's stderr to
  /dev/null, so an interpreter that cannot read `fixtures.toml` is reported
  as a platform that does not exist"
status: resolved
type: bug
area: build, testing
severity: medium
found: 2026-09-10
resolved_in: "fix(#1264): a dead manifest parser is reported as one, not as a platform/id that does not exist"
related: [phase-395]
---

## What this is

`scripts/build/platform-rmws.sh`:

```sh
out="$(python3 "$root/scripts/build/fixtures-manifest.py" coords 2>/dev/null \
    | awk -F'\x1f' -v p="$platform" '$2 == p { print $4 }' \
    | grep -v '^$' \
    | sort -u)"

if [ -z "$out" ]; then
    echo "nros_platform_rmws: no fixture rows for platform '$platform'" >&2
    echo "  (known platforms come from examples/fixtures.toml)" >&2
    return 1
fi
```

Empty output has two causes and the function reports one of them. The parser
printing nothing because the platform genuinely has no rows is the case the
message describes. The parser DYING is the case it does not: `2>/dev/null`
discards the traceback, the pipeline's status is `sort`'s, and `$out` is empty
either way.

## How it was found

Provisioning a contained self-hosted runner on Ubuntu 22.04:

```
nros_platform_rmws: no fixture rows for platform 'threadx-riscv64'
  (known platforms come from examples/fixtures.toml)
error: recipe `setup` failed with exit code 1
```

`threadx-riscv64` appears in `examples/fixtures.toml` 42 times, with four
`platform = "threadx-riscv64"` rows. The real cause was three layers away:
Ubuntu 22.04 ships python3.10, which has no `tomllib`; `fixtures-manifest.py`
falls back to `tomli`, which was not installed either; so it raised
`ModuleNotFoundError` before reading a byte of the manifest.

Running the same command by hand — without the redirect — says so in one line.
The diagnosis cost far more than that, because the message was specific,
confident, and about the wrong subject. It aims the reader at the manifest and
at the platform NAME, which are both fine.

## Why it matters beyond one host

The message is load-bearing by design. The file says so:

```
# Deliberately NOT silent on failure: a platform with no rows is a caller error
# (a typo'd name), and returning "nothing to provision" for it would reproduce
# the bug this fixes one level up.
```

That reasoning is right, and the redirect defeats it for every failure mode
that is not a typo. A probe that can only return "nothing" reads exactly like a
clean result — the shape this repo has paid for repeatedly (the vacuous-test
gate, the staleness probe, `check-one-producer-per-tool`'s own positive
control).

## Fix

Distinguish the two. Capture stderr rather than discarding it, and check the
parser's exit status separately from the emptiness of its output — noting
issue 1249's rule, that a status meant for inspection dies at the assignment
under `set -e` unless it is written `rc=0; out="$(...)" || rc=$?`.

On a non-zero status, report what the parser said. On a zero status with empty
output, the current message is correct and should stay exactly as it is.

## Sweep

`2>/dev/null` on a call whose emptiness is then interpreted is the class, not
this one site. Grep for the pattern across `scripts/` before closing, and fix
the siblings in the same commit.

## Not this issue

That the runner image lacked `tomli` is a container-provisioning gap, fixed in
`scripts/ci/runner-bootstrap.sh` (PR #842). This issue is that the failure was
unreadable, which would have been just as true on any python3.10 host without
`tomli` installed.

## Resolution

Added `scripts/lib/manifest-query.sh` — one shared `nros_manifest_query
<python-script> [args...]` that runs `fixtures-manifest.py`, captures stderr
into a temp file rather than discarding it, and checks the parser's exit
status via the `rc=0; out="$(...)" || rc=$?` idiom (issue 1249) rather than a
bare assignment. On failure it prints the parser's own stderr, indented, and
returns the parser's status (never 0); on success it prints stdout, empty or
not, unchanged.

`nros_platform_rmws` (`scripts/build/platform-rmws.sh`) now calls it and
reports "the manifest parser failed (above), so it is unknown whether
'\<platform\>' has fixture rows — not that it doesn't" on a parser failure,
leaving the original "no fixture rows for platform '\<platform\>'" message
untouched for the genuine-emptiness case.

**Sweep.** Grepping `scripts/` for `2>/dev/null` on a `fixtures-manifest.py`
invocation found two closely related siblings in `scripts/build/
fixture-id-guard.sh`, doing the identical thing for a different table:

- `nros_fixture_id_no_match` read a dead parser's empty `describe-id` output
  as "no row anywhere carries id '\<id\>'" — the exact misdiagnosis, one table
  over.
- `nros_fixture_require_known_platform` read a dead parser's empty
  `list-platforms` output as `return 0` ("manifest unreadable — not this
  guard's job"), silently disabling platform-typo validation on the same
  failure that broke the parser — a quieter failure mode, but the same root
  cause.

Both now use `nros_manifest_query` and fail loud, naming the parser failure,
with the genuine-typo and genuine-empty messages left exactly as they were.

Other `2>/dev/null` sites against `fixtures-manifest.py` exist
(`nuttx-libc-pin-guard.sh`, `drop-family-artifacts.sh`,
`check-fixtures-stale.sh`, `measure-fixture-build.sh`) but read emptiness as
"nothing to do" rather than emitting a confident wrong claim about a named
entity — a quieter, lower-severity shape than this issue's title. Left alone
here to keep the change scoped to the misdiagnosis class; worth a follow-up if
the quieter shape turns out to matter in practice.

**Tests.** `scripts/check-fixture-id-guard.sh` gained two cases exercising
both `fixture-id-guard.sh` functions against a fake `python3` that dies the
way a python3.10 host with no `tomllib`/`tomli` does. A new
`scripts/check-platform-rmws.sh` (`just check platform-rmws`) does the same
for `nros_platform_rmws`, plus the two pre-existing cases (real platform,
typo'd platform). All four cases were verified by mutation: reverting the
fix reproduces the original misdiagnosis and the new assertions catch it.
