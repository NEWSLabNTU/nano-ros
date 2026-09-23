---
id: 1466
title: "The apt idiom `check-workflow-indexed-apt` PRESCRIBES swallows its own script's
  failure: `apt-get install $(prereq-packages.py …)` installs nothing and the step
  SUCCEEDS — measured live in live-peer, which then ran without clang"
status: open
type: bug
area: [ci, tooling]
severity: medium
found: 2026-09-23
related: [1249, 1364, 1359]
---

## What happens

Four workflow steps install their declared system closure with the spelling
`check-workflow-indexed-apt.py` tells them to use:

```sh
apt-get update -q
apt-get install -y --no-install-recommends \
  $(python3 scripts/sdk/prereq-packages.py --manager apt clang libclang-dev)
```

A command substitution's exit status does not propagate to the command it sits
in, and `apt-get install` with zero package arguments exits 0. So when the
script fails, the step installs **nothing** and **succeeds**.

Measured live, `live-peer regression` run **35817736997** (schedule,
2026-09-23T04:17), job `rows whose board is NOT this runner`, step
`Install clang + libclang for bindgen`:

```
Traceback (most recent call last):
  File "/__w/nano-ros/nano-ros/scripts/sdk/prereq-packages.py", line 40, in load
    import tomllib as toml
ModuleNotFoundError: No module named 'tomllib'
…
  File "/__w/nano-ros/nano-ros/scripts/sdk/prereq-packages.py", line 42, in load
    import tomli as toml
ModuleNotFoundError: No module named 'tomli'
Reading package lists...
Building dependency tree...
Reading state information...
0 upgraded, 0 newly installed, 0 to remove and 177 not upgraded.
```

Green step. No `clang`, no `libclang-dev`, and the traceback is three lines above
the `0 newly installed` that reads like a cache hit.

## Why it is worth its own issue

The two things that make this more than a typo:

1. **The idiom is PRESCRIBED.** `scripts/check-workflow-indexed-apt.py` exists to
   stop workflows hardcoding package names, and its own remedy text prints this
   exact line (`check-workflow-indexed-apt.py:212`), with a passing test case for
   it at line 156. So every site that complies with the gate inherits the defect,
   and a new site will too. Fixing one workflow is the fix-at-the-reported-site
   shape; the idiom and the gate have to move together.

2. **It is now LATENT, which is worse than active.** Phase-466 puts a TOML parser
   in the CI images ([[issue-1364]]), so the script stops failing and the four
   steps start installing what they name. Nothing about the swallowing changed —
   the next thing that makes the script exit non-zero (an unknown `[prereq.*]`
   key, which it raises on by design) will be silent again, in a lane that has
   just been made trustworthy.

## The sites

```sh
grep -rn 'prereq-packages.py' .github/workflows/
```

| workflow | line | packages |
| --- | --- | --- |
| `release-nros.yml` | 124 | `zstd` |
| `docs.yml` | 93 | `doxygen graphviz` |
| `live-peer.yml` | 459 | `clang libclang-dev` |
| `nightly.yml` | 825 | `clang libclang-dev` |

`scripts/ci/runner-container.sh:124` gets it RIGHT already — it captures into a
variable inside an `if ! PREREQ_PACKAGES="$(…)"`, which is the 1249-sanctioned
shape. `just/ci.just:464` also assigns first. So the correct spelling already
exists in the tree twice; only the workflows use the swallowing one.

## What this is NOT

- **Not [[issue-1249]].** That is `out="$(cmd)"; rc=$?` under `set -e` — an
  assignment whose status you then try to inspect. This is a substitution used
  as an ARGUMENT, where there is no assignment and no status to inspect at all.
  Same family, different shape, and 1249's gate
  (`check-set-e-bare-assignment`) cannot see this one.
- **Not a bug in `prereq-packages.py`.** It behaves exactly as its header says:
  it refuses unknown keys loudly rather than printing nothing. The refusal is
  correct; nothing consumes it.

## What would close it

The idiom and its gate, together:

1. Change the prescribed spelling to capture first — `pkgs="$(python3 … )"`
   under `set -e` (or `if ! pkgs="$(…)"`), then install `$pkgs`, the way
   `runner-container.sh` already does. Move all four sites.
2. Make `check-workflow-indexed-apt.py` require that shape rather than print the
   swallowing one, including its own test fixtures, so a new site cannot
   reintroduce it.
3. Consider whether an empty resolved list should be an error at the call site
   too: today "the index legitimately maps this key to no package on this OS" and
   "the script died" are the same empty string. `prereq-packages.py` already
   raises on the former, so capturing the status is sufficient — but a `test -n`
   costs nothing and says so.

Acceptance is a workflow step that goes RED when `prereq-packages.py` fails,
demonstrated by a gate self-test rather than by waiting for the next outage.
