# Phase 365 — An SDK path is CONSTRUCTED, not searched

**Status (2026-08-16). W1–W5 LANDED + W3a.2 (stale-cache self-heal). Owed: the `lane=all` re-measurement, now expected to reach zero.** Design agreed; waves W1–W5 below, each
landing on its own with its own acceptance.

**Owns:** [issue 0625](../issues/0625-tool-resolution-ignores-the-pin.md).
**Retires on completion:** [issue 0500](../issues/README.md)'s ordering rule and
its gate, and the `< 0.6.0` Corrosion warning added 2026-08-16.
**Implements:** RFC-0014 (provisioning) — this is a change to how a provisioned
tool is LOCATED, not to how it is installed.

## The principle

nano-ros decides where a provisioned tool goes. `nros setup` writes

```
~/.nros/sdk/<tool>/<version-from-nros-sdk-index.toml>
```

because the index named that version. The layout is nano-ros's own OUTPUT, not a
fact about the environment. So consumption must CONSTRUCT the path from the same
two inputs that produced it, and never search for it.

A search can return something we did not install (the legacy unversioned
prefix), something a DIFFERENT project installed (a newer version), or nothing —
three wrong answers to a question with a known right one.

Every mechanism in the current design exists only because it asks instead of
constructs:

* `file(GLOB)` + `COMPARE NATURAL ORDER DESCENDING` — a search, so it needs an
  ordering rule to be deterministic;
* two implementations of that ordering (cmake and shell) — so it needs
  `check-cmake-corrosion-prefix` to keep them agreeing;
* and a THIRD route (`add_subdirectory`) that consults neither, which is what
  actually decided 155 of 183 resolutions.

A constructed path needs no ordering, no gate keeping two orderings in step, and
cannot be routed around: there is one spelling, and it either exists or it does
not.

## What this fixes, measured

One `lane=all` configure on 2026-08-16, in a tree pinning `corrosion 0.6.1-nros1`:

| resolution | count |
| --- | --- |
| Corrosion 0.5.1 | **155** |
| Corrosion 0.6.1 | 28 |

The prefix list's own ordering is CORRECT and was verified directly
(`0.6.1-nros1`, `0.5.1-nros1`, flat) — so this is not an ordering bug, and no
amount of fixing the sort would have helped.

It also answers the second half of 0625: a user with two checkouts pinning
different versions. The store is already version-keyed so both coexist; each
checkout reads its own index. Today they collide because resolution scans a
SHARED store and answers globally — "newest installed" is a global answer to a
per-project question, so provisioning from the newer project silently changes
what the older one resolves.

## Surface

* **14 tools**, every one pinning a `version` in `nros-sdk-index.toml`.
* **Store already version-keyed**: `arm-none-eabi-gcc/13.2-nros1`,
  `corrosion/0.6.1-nros1`, …
* **No shared path helper exists.** 29 raw `.nros/sdk` mentions across four
  mechanisms: cmake modules, cmake toolchain files, `just` recipes, the Rust CLI,
  and shell scripts.
* **The producer is `packages/cli/nros-cli-core/src/orchestration/sdk_store.rs`** —
  the CLI reads the index and writes the tree, so it owns the path function.

## Design

**One constructor, in the producer.**

```
tool_dir(tool) = <store>/<tool>/<index[tool].version>
```

**Consumers ask; they do not re-derive.** Non-Rust callers use
`nros sdk-path <tool>`. This is the rule the repo already applies one layer up:
CLAUDE.md requires consumers to locate a SystemModel through
`nros_orchestration_ir::model_location`, "never a hand-derived path". Same rule,
same reason.

**Resolution semantics:**

| case | behaviour |
| --- | --- |
| hit | use it, and REPORT it (`nano-ros: <tool> <version> via <origin>`) |
| miss | FAIL naming the pinned version, what the store holds, and the provisioning command. Never substitute another version |
| already-loaded (a parent scope supplied it) | accept only if it EQUALS the pin; otherwise fail naming both |

The report line is not decoration: it is the only reason the 155/28 split was
visible at all.

## Waves

### W1 — the constructor, in the producer — LANDED

`sdk_store::tool_dir(tool)`, reading the index pin. `nros setup` writes exactly
there (it already does; this makes the two the same function rather than two
spellings that agree).

**Acceptance — met.** `sdk_store::tool_dir()` reads the pin and reuses the
installer's own `tool_prefix()`. `phase365_tool_dir_tests` loads the REAL index
(not a fixture, which could agree with a stale copy of the pins) and asserts
install-path == `tool_dir()` for every pinned tool, plus that an unpinned tool
resolves to `None` rather than to a fallback.

### W2 — expose it to the other three mechanisms — LANDED

`nros sdk-path <tool>` prints the constructed path; `--require` exits non-zero
with the provisioning command when it does not exist.

**Acceptance — met.**

```
$ nros sdk-path corrosion
/home/aeon/.nros/sdk/corrosion/0.6.1-nros1

$ nros sdk-path corrosion --require          # with the pinned dir renamed away
Error: corrosion is pinned to 0.6.1-nros1 but …/0.6.1-nros1 does not exist.
Provision it:  nros setup --tool corrosion
exit 1
```

The load-bearing detail: `0.5.1-nros1` was present in the store throughout, and
`--require` still failed. A search would have returned it.

### W3a — Corrosion, the caught case — LANDED

Point `Corrosion_DIR` at the W2 path. Delete `_nros_corrosion_prefixes`, the
`scripts/build/cmake-prefix.sh` twin, and find the `add_subdirectory` route that
bypassed both ("Using Corrosion as a subdirectory") and convert it.

**Acceptance — met on the case that failed.** `examples/native/c/talker`, one of
the leaves that resolved 0.5.1 during the 155/28 measurement:

```
configure rc=0
nano-ros: Corrosion 0.6.1 via SDK store [hashed per-workspace cargo dirs]
0.5.1 mentions: 0
```

A full `lane=all` re-measurement is still owed and is listed under W3b-rest.

**The mechanism, finally identified** — and it was neither of the two things
suspected. The cmake side did `list(APPEND CMAKE_PREFIX_PATH …)`, putting its
newest-first candidates AFTER whatever the environment already carried; and
`scripts/build/cmake-prefix.sh` PREPENDED store prefixes from a glob of
`$store/corrosion/*/`, which matched the legacy unversioned install's `lib/` and
`share/` subdirectories — not versions at all, and under `sort -Vr` a pure-alpha
name sorts BEFORE the numeric ones. So the flat 0.5.x install led the exported
prefix path and won every resolution, no matter how correctly cmake sorted its
own list afterwards.

That is why fixing the ordering could never have worked, and why probing the
ordering in isolation looked innocent. The fix is `PATHS <constructed>
NO_DEFAULT_PATH` on one side and a single `nros sdk-path` line on the other:
with nothing enumerated, there is nothing to mis-order.

**Retired with it:** `check-cmake-corrosion-prefix`. It asserted that both
derivations spell the ordering idiom — and after the idiom was deleted it still
reported OK, because the two idioms now appear only in the COMMENTS explaining
their removal. A gate that passes on prose about itself is worse than no gate.

### W3b — the remaining consumers — corrosion half LANDED

`scripts/build/cmake-prefix.sh` now emits `nros sdk-path corrosion` and nothing
else; its glob + `sort -Vr` are gone. Consumers of that file
(`compile-check-fixtures.sh`, `fixture-matrix.sh`, `cmake-incremental.sh`)
therefore export the pinned prefix.

Still open: ~10 files spelling the store directly (`just/workspace.just` ×8,
`scripts/{zenohd,xrce-agent,dev,installers}/…`, `zephyr-fixture-leaves.sh`,
`doctor.rs`). W4's gate lands with them, not before — a gate against a rule the
tree still breaks in ten places is a red nobody can act on.

### W3b — the other tools — LANDED

Converted the two consumers that enumerated: `scripts/dev/zenohd.sh`
(`ls …/sdk/zenohd/*/bin/zenohd | sort -V | tail -1`) and
`cmake/toolchain/riscv64-threadx.cmake` (`file(GLOB …/riscv-none-elf-gcc/*/…)`
plus `list(GET … -1)`). Both are the corrosion defect in another tool, and both
were found by the rule rather than by a failure — zenohd especially matters,
since CLAUDE.md pins it to 1.7.2 for `rmw_zenoh_cpp` compatibility.

### W3b-rest — the other tools

cmake toolchain files, `just` recipes, shell scripts, in that order.

**Acceptance.** Zero raw `.nros/sdk` outside the constructor and the W2 command.

### W4 — make the class uncommittable — LANDED

**The rule changed once the survey was done, and the survey was right.** The
planned rule was "one spelling of `.nros/sdk` per language". The 18 sites in the
tree are three populations and only one is wrong:

* INSTALLERS legitimately write the store — they are the producer;
* `.nros/sdks/arm-fvp` is a different tree (`sdks`, not `sdk`);
* the defect is a CONSUMER enumerating versions and picking one.

So `check-sdk-store-not-enumerated` bans the SHAPE — a wildcard where the
version belongs (`/sdk/<tool>/*`) — not the mention. A rule that fired on
correct installers would have been edited away within a week.

**Acceptance — met**, and the gate paid for itself on its first correct run by
finding two sites the manual survey missed:

* `zephyr/cmake/nros_rmw_cyclonedds.cmake` globbed `<store>/cyclonedds/*/bin`
  into `HINTS` — selecting the IDL compiler that emits type-support, i.e. the
  museum-compiler failure the surrounding comment already warns about;
* `book/src/getting-started/installation.md` told USERS to run
  `ls -d ~/.nros/sdk/zenohd/*/bin/zenohd | tail -1` by hand.

**And the gate was vacuous in its first draft** — it anchored on `.nros`
immediately preceding `/sdk/`, while the real spelling is
`"${NROS_HOME:-$HOME/.nros}"/sdk/zenohd/*` with `}"` in between. It reported OK
on a deliberately reintroduced search. That is why the self-test is part of the
gate and not a formality: this is the second gate today that would have shipped
green and blind.

### W5 — retire the unversioned prefix — LANDED

`~/.nros/sdk/corrosion/{lib,share}` belongs to no version and therefore to no
project. Provisioning stops writing it; `nros doctor` reports it removable.

**The flat prefix had a PRODUCER, and it was the cause.** `nros setup --tool
corrosion` wrote `<store>/corrosion/<version>` while
`just workspace install-corrosion` wrote the unversioned `<store>/corrosion/` —
two producers, two layouts. That second layout is what caused 0625:
`cmake-prefix.sh` globbed `<store>/corrosion/*/`, which matched the flat
install's `lib/` and `share/` SUBDIRECTORIES (not versions), and under
`sort -Vr` a pure-alpha name sorts before the numeric ones, so it led the prefix
path and won 155 of 183 resolutions.

`install-corrosion` now asks `nros sdk-path corrosion` for its destination, so
installer and consumers share one constructor.

**Acceptance — met.** `nros doctor` reports an existing flat prefix and is
silent once it is gone:

```
nros doctor: [REMOVABLE] legacy unversioned install under ~/.nros/sdk/corrosion
    ~/.nros/sdk/corrosion/lib
    ~/.nros/sdk/corrosion/share
    remove it:  rm -rf ~/.nros/sdk/corrosion/lib ~/.nros/sdk/corrosion/share
```

**Caught before landing:** the first draft printed
`rm -rf <store>/corrosion` — which deletes the VERSIONED installs along with the
legacy one. A doctor that tells you to delete your pinned toolchain is worse
than one that says nothing, so the remedy now names only the flat
subdirectories.

## The decision this needs, made deliberately

W2 makes CMake depend on `nros` being on PATH. That is already true via
`activate.sh`, and consistent with the existing stale-CLI guard, which refuses to
run rather than silently using a wrong tool.

The alternative — CMake parsing the index itself — recreates exactly the second
implementation this phase removes, and the day this phase documents is what two
implementations of one rule cost. Take the PATH dependency.

## Sequencing note

W3a before W3b: Corrosion is the case with a measurement attached, so it is the
one that can prove the design before the mechanical conversions follow.

## W3a.2 — a cached `Corrosion_DIR` outranks the constructed path

The first `lane=all` after W1–W5 did NOT reach acceptance:

| | before | after W1–W5 |
| --- | --- | --- |
| resolutions of 0.5.1 | 155 | **64** |
| resolutions of 0.6.1 | 28 | 22 |

Better, not fixed — so the phase's claim stayed unproven, and three further
rounds of narrowing by inspection each eliminated a plausible route and left the
count non-zero.

**What finally found it was instrumentation, not argument.** The report line said
WHAT resolved and never WHO asked; adding `CMAKE_CURRENT_SOURCE_DIR` to it
showed the same source directory — the repo root, reached by every example's
`add_subdirectory(<repo-root>)` — resolving 0.6.1 nine times and 0.5.0 four
times in one family. Same source, different answers, so the difference had to be
per-BUILD-dir state:

```
139 example build dirs cached Corrosion_DIR=0.5.1-nros1
 20 cached 0.6.1-nros1
```

`find_package` short-circuits on a cached `<Pkg>_DIR` and never consults `PATHS`
or `NO_DEFAULT_PATH`. So constructing the prefix was only half the principle: a
build dir configured before the pin moved answers with the old version forever,
and nothing says so.

**Fix.** The resolver drops a cached `Corrosion_DIR` that is not inside the
constructed prefix, says so, and lets the search re-cache the right one. We know
where the tool is; a cache is not a second opinion worth honouring. Clearing 139
build dirs by hand would have been a remedy for this host and not a fix.

Verified in place, without deleting the build dir:

```
cache before : 0.5.1-nros1
-- nano-ros: dropping stale Corrosion_DIR (…/0.5.1-nros1/lib/cmake/Corrosion)
-- nano-ros: Corrosion 0.6.1 via SDK store
cache after  : 0.6.1-nros1
```

**Method note.** I twice reported "no live 0.5.1 caches" from a `grep -r
--include=CMakeCache.txt` that silently returned nothing — it also reported zero
caches on 0.6.1, which a single known counter-example would have contradicted.
A negative result whose positive control is also absent is not evidence.
