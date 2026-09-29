---
id: 1553
title: "The CI image provisions no Corrosion, so every probe gate git-clones it at configure time and two of them race one shared fetch cache"
status: open
type: bug
area: [ci, build]
severity: medium
found: 2026-09-28
related: [0500, 0726, 1457, 1482]
---

## What happens

On the `check` job of `gate.yml`, a gate that configures a probe project
reaches the repo root's `nros_resolve_corrosion` and finds nothing in the SDK
store, so it falls through to a git `FetchContent` into one shared cache. With
two such gates in the `-P4` parallel fast lane they clone into the same
directory at the same time and one loses.

PR #1354, run **36436355781** (2026-09-28T14:29:24Z), job **108975287240**,
`1 of 366 gate(s) FAILED`:

```
===== FAIL (probe-workspace-caps, rc=1, 2937ms) =====
[FAIL] [no caps] probe configure failed
-- nano-ros: no Corrosion at the pinned prefix (/github/home/.nros/sdk/corrosion/0.6.1-nros1) — falling through to FetchContent
-- nano-ros: Corrosion not provisioned — fetching v0.6.1 (1499b14e...) from git into the host fetch cache at /github/home/.nros/fetch
[ 11%] Performing download step (git clone) for 'corrosion-populate'
BUG: refs/files-backend.c:2992: initial ref transaction called with existing refs
fatal: destination path 'corrosion-src' already exists and is not an empty directory.
-- Had to git clone more than once:
          3 times.
CMake Error ... Failed to clone repository: 'https://github.com/corrosion-rs/corrosion.git'
```

`destination path 'corrosion-src' already exists and is not an empty directory`
plus git's own `BUG: refs/files-backend.c:2992: initial ref transaction called
with existing refs` is a concurrent clone into a populated directory. It is not
a network failure — the clone reaches GitHub.

## The same condition produced a DIFFERENT verdict the day before

The previous run of the same PR, **36321234788**, job **108625376621**
(2026-09-27), failed **both** probe gates, with the identical pair of
`nano-ros:` lines and a different loser's symptom:

```
===== FAIL (probe-workspace-caps, rc=1, 2046ms) =====
===== FAIL (probe-shared-types, rc=1, 2060ms) =====
  Parse error.  Expected "(", got newline with text "
  CMake step for corrosion failed: 1
  /__w/nano-ros/nano-ros/cmake/NanoRosCorrosion.cmake:865 (FetchContent_MakeAvailable)
```

A `Parse error` on the fetched `CMakeLists.txt` is a half-written cache — the
same race, read at a different moment. Two days, two symptoms, one condition:
that is the "a lane red every cycle has no signal capacity" shape in miniature,
and it is why the 2026-09-27 red was first attributed to a provisioning gap
that would clear on its own.

## The negative control: both gates pass where the store is provisioned

On a host whose SDK store holds the pinned Corrosion, both gates pass from the
same checkout as the failing run (`fe36c6593`), built with `just setup-cli`
first — run as `just check <gate>`, one gate at a time:

| gate | result |
| --- | --- |
| `probe-workspace-caps` | rc=0, `[PASS] all 5 checks passed` |
| `probe-shared-types` | rc=0, `[PASS] all 17 checks passed` |

(The first of those two recipes arrives with PR #1354 and is not on `main` yet,
which is why this section names the gates rather than pasting the two command
lines: `check-doc-recipe-refs` resolves a documented invocation against the
CURRENT justfile, and it is right to refuse one a reader cannot run. It ejected
this issue's own PR from the merge queue for exactly that.)

So neither test is wrong about its subject, and nothing about the probe projects
needs changing. The only difference between pass and fail is whether
`find_package` resolves Corrosion or the configure has to clone it.

## What this is NOT

**Not PR #1354's defect.** `probe-shared-types` is on `main` and renders the
same probe shape — `find_package(nano_ros REQUIRED)` + `nros_workspace_interfaces()`
+ a package whose `CMakeLists.txt` calls `nros_components_register_node` — so
it has the same Corrosion requirement. #1354 adds a SECOND gate of that shape,
which is what turns a latent single-fetcher condition into a collision. The
2026-09-27 run, where both failed, is the evidence that the requirement is not
new.

**Not issue 0500.** That one is a stale store prefix SHADOWING a newer pin; here
the store is empty, and the configure says so in as many words.

**Not issue 0726.** There the PATH fallback could not fire because `find_program`
no-ops on an already-defined variable. Here the fallback fires correctly and
lands in a git clone, because there is nothing to find.

## What would close it

`nros setup --tool corrosion` provisioned in the CI image, so `find_package`
resolves at the pinned prefix and no gate reaches `FetchContent` at all.

**Acceptance is `probe-workspace-caps` passing in CI on PR #1354** — the gate
that is actually blocked.

The first version of this section said "a `check fast` log containing no
`falling through to FetchContent` line", and that criterion **cannot be
observed**: both probe tests write their configure output to a per-case log
(`$TEST_TMPDIR/configure*.log`) and only `tail` it when the configure FAILS. So
a green run prints nothing about Corrosion either way, and an absent line says
only that nothing failed — it is the same shape as a gate that cannot fail on
the case it names. Measured on 2026-09-28: the check job of merge_group run
36462943336, which is green, contains **zero** lines matching either
`FetchContent` or `Corrosion`, on an image where nothing had provisioned the
store. An absence there proves nothing.

If a direct read of the image is wanted instead of a gate verdict, it has to come
from something that prints unconditionally — `scripts/ci/runner-doctor.sh`
reporting the pinned prefix, or a configure that names its resolution the way
`nano-ros: Corrosion <ver> via <origin>` does (issue 0500: read that line, never
infer the version from having run the installer).

Per issues 1457/1482 that is a change to the Dockerfile
`scripts/ci/runner-container.sh` generates, not a host `apt install` — the
running container is `--cap-drop ALL` non-root and provisions nothing itself.

**Explicitly not the remedy:** serialising the two gates, or taking a lock
around the fetch. Both leave every probe gate depending on a network clone at
configure time — the condition the SDK store exists to remove (0500) — and they
would hide an empty store rather than fill it.

## The remedy could not be applied as written (2026-09-29)

The "What would close it" section above says *provisioned in the CI image*.
Measured: an image layer cannot hold it. `$HOME` in a `container:` job is
`/github/home`, which the Actions runner BIND-MOUNTS over the image from
`_temp/_github_home` before the first step runs, so anything baked at
`~/.nros` in `ci/docker/ci-base` is masked and the store is empty exactly as
observed.

The store root the resolver reads is `$NROS_HOME/sdk`, falling back to
`$HOME/.nros/sdk` (`_nros_corrosion_store` in `cmake/NanoRosCorrosion.cmake`),
and nothing else — the CLI's own `NROS_STORE` knob does not reach that
function. So the only prefix an image layer could fill is one reached by
setting `NROS_HOME`, which moves every lane's workspaces, fetch cache and
`bin/` off the mounted volume with it. That is a larger change than the defect,
on the same lanes issue 1353 is already measuring for disk.

**Fixed as a run-time step instead**, beside the clang-format one that exists
for the same reason (the image ships no clang-format either, and the resolver
errors rather than degrading):

```yaml
- name: Provision Corrosion (pinned by [tool.corrosion])
  run: |
    source ./activate.sh
    just workspace install-corrosion
```

Affordable on every event: `[tool.corrosion]` is config-only at install time —
a small CMake superproject staged into the store, which is why the index files
it under the `default` tier — and the recipe is a forwarder to
`nros setup --tool corrosion`, idempotent on the pinned prefix.

The acceptance is unchanged and still the right one: **`probe-workspace-caps`
passing in CI on PR #1354.** That PR's run 36556534101 (job 109358679755,
2026-09-29) failed it again on the second attempt of the same head sha, with
the same `destination path 'corrosion-src' already exists` — so the collision
is not a once-per-blue-moon race, it is what this lane now does every time two
probe gates of that shape run in one `-P4` fast lane.
