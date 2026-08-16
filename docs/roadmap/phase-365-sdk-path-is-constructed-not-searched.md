# Phase 365 — An SDK path is CONSTRUCTED, not searched

**Status (2026-08-16). W1 starting.** Design agreed; waves W1–W5 below, each
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

### W1 — the constructor, in the producer

`sdk_store::tool_dir(tool)`, reading the index pin. `nros setup` writes exactly
there (it already does; this makes the two the same function rather than two
spellings that agree).

**Acceptance.** A test asserts install-path == `tool_dir()` for all 14 tools, so
producer and consumer cannot drift.

### W2 — expose it to the other three mechanisms

`nros sdk-path <tool>` prints the constructed path; `--require` exits non-zero
with the provisioning command when it does not exist.

**Acceptance.** `nros sdk-path corrosion` prints
`~/.nros/sdk/corrosion/0.6.1-nros1`; with the directory renamed away,
`--require` fails naming the pin and `nros setup --tool corrosion`.

### W3a — Corrosion, the caught case

Point `Corrosion_DIR` at the W2 path. Delete `_nros_corrosion_prefixes`, the
`scripts/build/cmake-prefix.sh` twin, and find the `add_subdirectory` route that
bypassed both ("Using Corrosion as a subdirectory") and convert it.

**Acceptance.** A `lane=all` configure reports ONE version, and zero resolutions
of 0.5.1 — the direct inverse of the 155/28 measurement above.

### W3b — the remaining consumers

cmake toolchain files, `just` recipes, shell scripts, in that order.

**Acceptance.** Zero raw `.nros/sdk` outside the constructor and the W2 command.

### W4 — make the class uncommittable

A gate: `.nros/sdk` may be spelled in exactly ONE place per language. Same shape
as `check-atomic-sync-writes` — the discipline exists, so the gate is what stops
the second spelling.

**Acceptance.** Self-tested red on a reintroduced literal, in both languages.

### W5 — retire the unversioned prefix

`~/.nros/sdk/corrosion/{lib,share}` belongs to no version and therefore to no
project. Provisioning stops writing it; `nros doctor` reports it removable.

**Acceptance.** A fresh provisioning run writes no flat prefix; `doctor` names
an existing one.

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
