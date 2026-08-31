# Phase 400 — the unified config system: build it, migrate onto it, retire the rest

**Status (2026-08-31): W2-W5 and W7 done; W6's first and largest tenant
(`NROS_EXECUTOR_*`) done, its remaining tenants and W1 / W8 outstanding.** Design is
[RFC-0086](../design/0086-unified-configuration-transport-tenant-and-coupling.md),
which amends RFC-0049 (Stable) and adopts RFC-0071 D8. Nothing here proposes a
new mechanism — the ladder and `nros config explain` already exist and work.
This phase gives them reach and deletes what they replace.

**The measurement this phase exists to move.** Counted against the tree on
2026-08-30:

| where configuration lives | count |
| --- | --- |
| Zephyr `CONFIG_NROS_*` symbols | 78 |
| `NROS_*` env read in build scripts | 21 |
| `ZPICO_*` env | 9 |
| **knobs in the RFC-0049 ladder** | **3** |

**That table had no METHOD, so no wave could show it moved.** Every gate here
is stated as "measured the same way as the table at the top", and there was no
same way — two people counting by hand get two numbers. The method now exists:
`just check config-knob-census` (`scripts/check/config-knob-census.py`),
buildless, and it is on the fast line. Re-measured 2026-08-31:

| where configuration lives | count |
| --- | --- |
| **knobs in the RFC-0049 ladder** | **13** (8 executor · 3 zenoh.tx · 2 transport) |
| Kconfig `CONFIG_NROS_*` declarations | 85 |
| `NROS_*` env read by a `build.rs` | 31 |
| `ZPICO_*` env read by a `build.rs` | 5 |

Do NOT read `3 → 13` as this phase's delta: the 3 was counted by an unknown
method and the rows are not comparable term-for-term. What is now true is that
the number is REPRODUCIBLE, and the ladder count is ratcheted — it may rise,
never fall, because a knob leaving the ladder is a knob losing its platform and
board rung and its `nros config explain` row. The env rows are REPORTED and not
gated, deliberately: a migrated knob keeps its env name as the front-end, so
"the env count falls to zero" was never the goal.

Descriptors: 4 rmw · 7 platform · 11 board, across 12 package kinds. Only three
kinds have one.

**The concrete artefact to delete.**
`src/zephyr_entry/snippets/island-serial/serial.conf` in the safety-island tree
— fifteen hand-written Kconfig lines turning off an IP stack, three drivers and
three zenoh link features, because choosing a transport could not imply any of
them. When W3 lands that file is a transport stanza and nothing else.

---

## Waves

| | What | Gate | State |
| --- | --- | --- | --- |
| **W1** | Platform axis resolves like rmw: descriptor beside its crate, by name over a search path | a platform outside the tree builds without forking `config/` | |
| **W2** | De-name the platform sections — `[build.<rmw>]`, `[knobs.<rmw>.tx]` | a second backend receives platform knobs with neither side naming the other | **done** |
| **W3** | `transport` tenant + `requires` / `implies` / `exactly-one-of` in the resolver | serial-only image builds with no hand-written link or driver lines | **done** |
| **W4** | Provenance: `explain` reports implied and overridden | every knob prints rung, rule, and override | **done** |
| **W5** | Kconfig mirror — link symbols get `depends on`; drift test holds it | `NETWORKING=n` cannot leave `LINK_TCP=y` | **done** |
| **W6** | Migrate the sizing knobs, tenant by tenant | ladder knob count rises; env/Kconfig count falls | |
| **W7** | Core: audit for exclusive/negative features | no `compile_error!` stands in for exclusivity | **done — none found; premise was wrong** |
| **W8** | Retire the old paths | the retired mechanism no longer resolves anything | |

W1 and W2 are mechanical and unblock everything else; do them first and
together. W3 is the substance. W6 is the long tail and is deliberately last —
those knobs are scattered, not wrong.

---

## W1 — the platform axis resolves like the others

RFC-0049 opens by rejecting a central file, then implements one. rmw and board
descriptors live in their packages; platform descriptors live in
`config/<name>/` behind a single `--platforms-dir` root.

* Move `config/<name>/nros-platform.toml` →
  `packages/platform/nros-platform-<name>/nros-platform.toml`.
* Resolve by name over a search path of workspaces, reusing RFC-0071 D5's
  resolver rather than writing a second one.
* Keep `--platforms-dir` / `$NROS_PLATFORMS_DIR` as an explicit single-root
  override. It stops being the only way in.

**Trap.** `config/` also holds `git-settings.txt`, `rust-targets.txt` and a
README. This wave moves the platform descriptors only; `config/` does not
disappear.

**Gate.** A platform package outside the tree resolves and builds with
`config/` untouched.

---

## W2 — de-name the platform sections

All seven platform files carry `[build.zenoh]` and `[knobs.zenoh.tx]`.
RFC-0071 D8 calls this a violation; it has already cost time. Migrating to
zenoh-pico 1.10 meant editing `config/freertos` and `config/posix` — platform
files — because a vendored library moved `system/freertos/lwip/network.c` to
`system/socket/lwip.c`.

* `[build.zenoh]` → `[build.<rmw>]`, keyed on the resolved backend.
* `[knobs.zenoh.tx]` → `[knobs.<rmw>.tx]`.
* Loader reads the section matching the selected backend; an unmatched section
  is a warning naming both, not a silent skip.

**Gate.** A second backend receives platform build settings without the
platform file naming it.

---

## W3 — the transport tenant and the coupling verbs

The substance. RFC-0086 D1 and D2.

* `[transport]` in `system.toml`: `kind` (`exactly-one-of`) + `endpoint`.
* `[rmw.transport.<kind>]` in each backend descriptor: the locator template,
  `requires` (capabilities), `requires_links`.
* Resolver learns three verbs:
  * `requires` — hard. Failure is a build error naming both files.
  * `implies` — weak, Kconfig `imply` strength. A higher rung still wins.
  * `exactly-one-of` — group, from Gentoo `REQUIRED_USE`.

**The rule that must not be got wrong.** `implies` is `imply`-strength, never
`select`. Kconfig's own documentation records why: `select` forces a symbol and
can produce invalid configurations. A forcing verb here would let
`transport.kind = "serial"` silently stamp out an explicitly requested TCP link.

**Where the rules live.** In the resolver, not Kconfig. `depends on
NET_SOCKETS` fixes the Zephyr build and nothing else — cargo and CMake lanes
have no equivalent, so a board built through either still needs the lines
hand-written.

**Gate.** The safety-island serial image builds from a transport stanza alone.
Diff `serial.conf` before and after: fifteen lines to about three.

---

## W4 — provenance, or the resolver cannot be trusted

`nros config explain` prints value + rung today. It must also print, per knob:
the rung that set it; whether it was **implied** and by which rule; whether an
implication was **overridden** and by which rung.

Opaque layered merges are the failure mode of every layered-config system
RFC-0049 surveyed. This is not a nicety — it is how a wrong image gets
diagnosed without bisecting fragments.

**Gate.** For a serial image, `explain` shows `links.tcp = off (implied by
transport.kind=serial)`; setting `CONFIG_NROS_ZENOH_LINK_TCP=y` on top shows
`on (front-end, overriding implication)`.

---

## W5 — the Kconfig mirror

Give the link symbols the dependency they never had:

```kconfig
config NROS_ZENOH_LINK_TCP
    bool "TCP link"
    depends on NET_SOCKETS
    default y
```

RFC-0049 already mandates a drift test asserting the fragment mirrors the
platform toml; extend it to cover the new `depends on`.

**Note.** This does not replace W3 and must not be done instead of it. It is
the Zephyr-lane projection of a rule that lives in the resolver.

**Gate.** `NETWORKING=n` with `LINK_TCP=y` is unreachable in menuconfig, and
the drift test fails if the mirror and the resolver disagree.

---

## W6 — migrate the sizing knobs

The long tail: buffers, pools, entity caps, ring depths — the bulk of the 78 +
21 + 9. Tenant by tenant, each keeping its existing env/define name as the lane
front-end so nothing breaks at the call site.

Order by blast radius, largest first: `NROS_EXECUTOR_*` (8 read by
`nros-node/build.rs`), then the zenoh pools, then the per-entity caps.

**Gate.** Ladder knob count rises, measured by
`just check config-knob-census`. A tenant is migrated when `nros config
explain` prints it and the front-end still overrides it.

### `NROS_EXECUTOR_*` — DONE

All eight resolve over the ladder (`PlatformTree::resolve_executor`), both
production readers call it (`cmd/config.rs`, `orchestration/model_ingest.rs`),
and `nros config explain` prints all eight with the env name that is still
their front-end:

```
executor.max_cbs                   4          builtin  [NROS_EXECUTOR_MAX_CBS]
executor.arena_size                derived    builtin  [NROS_EXECUTOR_ARENA_SIZE]
$ NROS_EXECUTOR_MAX_CBS=17 nros config explain --platform posix
executor.max_cbs                   17         env      [NROS_EXECUTOR_MAX_CBS]
```

Two of the eight were invisible until 2026-08-31, and the reason is worth
keeping: their defaults are DERIVED, not constant, so they did not fit the
`&[(&str, usize)]` table the other six were listed in and were simply left out
— in a report whose entire job is to say where a value came from. Neither
derivation is duplicated to fix it: `action_clients` defaults to the RESOLVED
`max_cbs` (build.rs then clamps to it, so the default is the clamp), and
`arena_size` defaults to `0`, the documented Kconfig sentinel for "derive it",
printed as `derived`. `nros-node/build.rs` stays the one place that knows the
arena formula.

**The platform and board rungs reach a cargo build (2026-08-31).** They did
not when the tenant first landed: `nros-node/build.rs` resolved env → Kconfig →
default and never opened the platform or board TOML, so a build outside cmake
compiled at crate defaults — the failure `ExecutorKnobs`'s own doc comment
describes.

The obstacle was not the ladder. `nros-node` deliberately has NO `platform-*`
cargo feature (phase-248 C2: the core executor is platform-agnostic and reaches
the platform through the vtable), so its build script could not know which
platform it was compiling for — and reversing that to migrate a sizing knob
would be the tail wagging the dog.

Nor was "have the lane export the resolved values" available: that is issue
0460 exactly, where cmake's `set(ENV{...})` reaches the C lane and not the
cargo one.

What resolves it is the idiom this repo already uses everywhere else — the
lane exports a value and a POINTER, and the build script reads the file:

* `nros ws board-facts` now emits `NROS_PLATFORM_NAME` beside `NROS_BOARD_TOML`.
  The board descriptor already knew its platform; this is the seam that had
  already resolved it. `NROS_PLATFORM_NAME`, not `NROS_PLATFORM`, because
  cmake's `-DNROS_PLATFORM=cffi` names the platform LAYER — one variable
  meaning two things is how they start disagreeing.
* `corrosion_set_env_vars` already attaches those to the target's own build
  command, which is what actually runs cargo. No new channel.
* `nros-node/build.rs` resolves env → Kconfig → board → platform → builtin,
  taking a host-only build-dep on `nros-board-common` (`build-helpers`), which
  `nros-zpico-build` already takes for the zenoh tenant. No cycle.

Cost, measured rather than estimated: 18 leaf locks moved, and **16 of them
gained only the two path-dep lines** — they are board crates that already
depend on `nros-board-common`. Only `nros-verification` pulls the serde/toml/cc
chain fresh (182 lines), in a HOST build graph, never linked into firmware.

Every failure on that path is FATAL, not a fall-through to defaults. The first
version used `.ok()`, and a platform file with one rejected key compiled at the
crate defaults while reporting success — which is the shape the whole ladder
exists to remove. `nros-zpico-build` says the same thing about the same tree.

Verified end to end, all four paths: a platform rung applies (`max_cbs = 11`),
a board rung applies (`max_sc = 23`), the env front-end still wins over both
(`99`), and a malformed platform file panics naming the bad key. With no
`NROS_PLATFORM_NAME` — a bare `cargo build` with no lane — nothing changes:
with no board named there IS no platform rung to resolve.

### Remaining tenants

The zenoh pools, then the per-entity caps.

---

## W7 — core's exclusive features — DONE, and the premise was wrong

**Audited 2026-08-30. No work outstanding.** This wave was scoped from a claim
that did not survive checking, and the correction is recorded here rather than
quietly dropped.

The claim: `nros-node`'s `scheduler-fifo` / `-edf` / `-bucketed` / `-sporadic`
is a pick-one family expressed in an additive mechanism, guarded by five
`compile_error!` calls standing in for the constraint Cargo cannot express.

What the tree actually says, three lines above those declarations:

> Each flag is independent; multiple may be on simultaneously when runtime
> selection across classes is needed.

The `cfg` sites agree — they gate scheduler classes in, additively. The five
`compile_error!` guards are feature IMPLICATIONS (`param-services` needs
`alloc`), not exclusivity. Every `cfg(not(feature = …))` in core is the correct
no_std shape. And `packages/api/nros/src/lib.rs` records that a platform
mutual-exclusion `compile_error!` was deliberately *removed* — the tree has
already learned this lesson.

The error was inferring exclusivity from a naming family without reading the
comment above it.

**What survives.** RFC-0086 D5's rule stands on its own footing: Cargo features
are additive by contract, so they cannot express "off over an on-default",
which RFC-0049 requires of a front-end. That is a constraint on new knobs,
enforced at review. It simply has no backlog attached.

**Gate.** Satisfied on audit: no `compile_error!` in core stands in for an
exclusivity constraint, and no core feature encodes a negative.

---

## W8 — retire the old paths

Retirement is a wave, not a side effect, because a mechanism that still
resolves is a mechanism people still use.

* A migrated knob's old reader is **deleted**, not left as a fallback. A
  fallback that silently wins is how issue 0135/0316 happened: two consumers
  disagreeing about one value with no diagnostic.
* Env vars that were the *only* home for a knob before nano-ros #0749/#0752
  keep their names as front-ends. Env vars that duplicated a ladder knob are
  removed.
* `config/<name>/nros-platform.toml` stops being read after W1's grace period.
* A gate asserts no knob has two readers.

**Gate.** For every migrated knob, exactly one reader exists, and
`nros config explain` is the only way to learn its value.

---

## Risks, and the ones already realised

* **A fallback left in place wins silently.** Realised twice: issue 0135 and
  issue 0316, both "two consumers disagreed about a struct's size with no
  diagnostic". W8's one-reader gate exists for this.
* **A drift test that mirrors nothing.** RFC-0049's mirror test is only as good
  as its coverage; W5 must extend it to the new `depends on`, or the Kconfig
  projection silently diverges.
* **Migrating a knob without its coupling.** A sizing knob moved into the
  ladder with its implications left in a `.conf` file is worse than not moving
  it — the value looks authoritative and is not. Move the rule with the knob.
* **`implies` implemented as `select`.** The single most likely wrong turn, and
  the one the surveyed prior art most clearly warns against.

## What this phase does NOT do

* It does not change the four-rung ladder or its order. RFC-0049's precedence
  is correct.
* It does not touch the runtime seam (`nros_rmw_vtable_t`, RFC-0035).
* It does not introduce a constraint solver. `requires` validates and `implies`
  enforces; picking a satisfying assignment reintroduces the opacity W4 exists
  to prevent.
* It does not migrate RFC-0045's boot-config resolution, which is the runtime
  half of the same story and lands separately.
