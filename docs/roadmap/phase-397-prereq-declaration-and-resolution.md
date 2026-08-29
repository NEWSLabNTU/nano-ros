# Phase 397 — `[prereq.*]`: one prerequisite namespace, four providers

**Status (2026-08-29). W1 landed — the schema and resolver exist and
`[system.*]` is an alias. W2–W5 not started.** Implements the 2026-08-29
amendment to [RFC-0062](../design/0062-unified-dependency-ssot.md), which
settled the declaration and resolution strategy before any code was written.

## Why

A dependency this index ALREADY declared reached the dynamic loader. An agent
hit `libslirp.so.0` missing on the store's QEMU; `[tool.qemu] system =
["libslirp"]` had been declared precisely so setup and doctor could say so
"BEFORE the smoke check fails with a bare loader error", and nothing consulted
it on the path where the tool is used. That consumer gap is fixed (the store
resolver now probes). What it exposed is the schema gap behind it:

**a user cannot declare a prerequisite at all.** `package.xml` `<depend>` feeds
build ORDER only, and a name that is not a workspace package is silently
ignored by construction — so a missing prereq has no way to be noticed early.

## The settled decisions (RFC-0062, amended 2026-08-29)

1. **`[prereq.<key>]`** is the declaration table, spanning four providers that
   already exist as separate classes: `system` (OS package), `sdk`
   (`[tool.*].dist`), `source` (`[tool.*].source` + `install`), `submodule`
   (`[source.*]`).
2. **An unknown key is an ERROR.** Ladder: workspace package → build order;
   generated message package → `nros sync` owns it; `[prereq.*]` key → its
   provider; anything else → fail, naming the key.
3. **rosdep is not consulted.** It answers for one provider of four, cannot
   carry a `check`, and a resolver present on only some hosts makes one tree
   resolve two ways.
4. **`[system.*]` merges in and retires** — it is the `provider = "system"`
   case and carries no field `[prereq.*]` lacks.
5. **`check` is provider-independent** — it answers "is the capability usable?",
   never "did provider X install it?". Provider verification (store
   path+version, submodule rev, dist sha256) is a different question and stays
   with the provider.

## Waves

- [x] **W1** Schema + resolver. `[prereq.<key>]` parses with `provider`
      (default `system`) and an ordered `providers` list; `[system.*]` parses
      as an alias lowering to `provider = "system"`. `nros setup --system` /
      `--check` and `nros doctor` read the merged table. **No behaviour change
      for any existing entry** — this wave is additive and its acceptance is
      that the 25 existing entries resolve identically before and after.

      **Done.** `nros setup --system --check` reports `22 present, 0 missing,
      3 unprobed` before AND after — byte-identical. Verified beyond that by
      adding entries temporarily: a `[prereq.*]` entry is read (22 → 23 present,
      and a deliberately-absent one reports `[MISSING]`), and a duplicate key
      resolves to the `[prereq.*]` side (`genromfs` overridden, count 22 → 21
      with its new probe failing). Five schema tests assert the alias keeps
      every field, the override rule, the default chain, all four providers
      parsing, and that a mistyped field is an ERROR rather than a silently
      empty entry.

      `prereqs()` is THE accessor and no consumer reads either table directly —
      one that read `[system.*]` alone would see a shrinking half of the SSoT
      while the migration runs.

- [x] **W2** The two new `check` kinds the current four cannot express:
      `runs` (the resolved binary EXECUTES — `cmd` is `command_exists`, a PATH
      lookup, and a store dist is not on PATH) and `path` (a file inside a
      checkout, for providers whose only test today is "the directory exists",
      true of an empty uninitialised submodule). Both must return `Unknown`
      where they cannot answer — issue 0487's rule: a probe that cannot answer
      must not vote.

      Then give `[tool.*]` and `[source.*]` entries probes. **Measured today:
      `[system.*]` 22 of 25 have one; `[tool.*]` 0 of 14 and `[source.*]` 0 of
      15 have none** — their presence test is "the path exists", which is the
      state the motivating failure exploited.

      **Done.** `runs` and `path` land with the tri-state honoured, and six
      entries gain probes: `qemu` (sdk, `runs`) and five submodule checkouts
      (`path` on a sentinel header). `nros setup --system --check` goes
      22 → 28 present.

      Both kinds mutation-checked in the real index, restored after:
      hollowing out `freertos-kernel`'s `FreeRTOS.h` reports `[MISSING]` — the
      uninitialised-submodule case "the directory exists" could never catch —
      and for `runs`, a non-zero exit reports MISSING (broken dist, loader
      failure: the libslirp shape) while a binary absent from PATH reports
      UNPROBED, because "cannot execute here" is not "absent".

      `path` resolves against the provider's own `dest` via
      `SdkIndex::prereq_checkout_dir`, so no prereq entry restates a location
      `[source.*]` already declares. Without a base it ABSTAINS rather than
      guessing a root.

- [ ] **W3** `package.xml` as a declaration site, and the fail-on-unknown
      behaviour change. **This is the wave with blast radius**: it touches every
      `package.xml` in the tree, and it converts a silent skip into an error.
      Sequence deliberately: land the ladder's first three rungs and REPORT what
      rung 4 would reject, before making rung 4 fatal. `NROS_ALLOW_UNRESOLVED_DEPS=1`
      is the escape hatch.

- [ ] **W4** Retire `[system.*]`. Alias → warn → gate → delete at the next
      minor. **The warning is wired at index load AND a test asserts it is
      reached** — phase-383's W1.f shipped a correct, well-tested deprecation
      lint that no production path called, so the warning reached nobody and a
      removal on that basis would have landed on users never told. A gate
      rejects a key declared in two provider tables, so the `genromfs` shape
      (one prereq, two tables, tied together only by prose in `why`) cannot
      silently return.

- [ ] **W5** Delete the `rosdep_resolve` fallback in `cmd/setup.rs`
      (phase-327 W6), dead once decision 3 lands — not left as an unreachable
      branch.

## Still open, deliberately

- Whether `providers` is ordered preference or a policy-driven chain (never
  build from source in CI; prefer the dist offline). Interacts with RFC-0065
  D14's offline guarantee.
- Whether `check` becomes REQUIRED. Three entries have none today
  (`ros-rmw-zenoh-cpp`, `python3-venv`, `picolibc-riscv64-unknown-elf`) and
  report `UNPROBED`; forcing an answer may not be possible for all of them.

## Evidence the merge is right, not preferred

- `board.packages` is ALREADY a flat namespace resolving across four classes:
  `qemu`/`arm-none-eabi-gcc`/`espflash` → `[tool.*]`,
  `freertos-kernel`/`lwip`/`nuttx-*`/`threadx*` → `[source.*]`, `arm-fvp` →
  `[gated.*]`, `genromfs` → `[tool.*]` AND `[system.*]`.
- `genromfs` needs two providers and says so in PROSE: `[system.genromfs].why`
  reads "the `[tool.genromfs]` source recipe is the store alternative".
