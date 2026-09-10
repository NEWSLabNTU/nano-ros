---
id: 1264
title: "`nros_platform_rmws` redirects the manifest parser's stderr to
  /dev/null, so an interpreter that cannot read `fixtures.toml` is reported
  as a platform that does not exist"
status: open
type: bug
area: build, testing
severity: medium
found: 2026-09-10
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
