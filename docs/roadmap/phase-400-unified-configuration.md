# Phase 400 — the unified config system: build it, migrate onto it, retire the rest

**Status (2026-08-30): not started.** Design is
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
| **W2** | De-name the platform sections — `[build.<rmw>]`, `[knobs.<rmw>.tx]` | a second backend receives platform knobs with neither side naming the other | |
| **W3** | `transport` tenant + `requires` / `implies` / `exactly-one-of` in the resolver | serial-only image builds with no hand-written link or driver lines | |
| **W4** | Provenance: `explain` reports implied and overridden | every knob prints rung, rule, and override | |
| **W5** | Kconfig mirror — link symbols get `depends on`; drift test holds it | `NETWORKING=n` cannot leave `LINK_TCP=y` | |
| **W6** | Migrate the sizing knobs, tenant by tenant | ladder knob count rises; env/Kconfig count falls | |
| **W7** | Core: retire the exclusive `scheduler-*` features | `compile_error!` guards deleted, not replaced | |
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

**Gate.** Ladder knob count rises and the env/Kconfig count falls, measured the
same way as the table at the top. A tenant is migrated when
`nros config explain` prints it and the front-end still overrides it.

---

## W7 — core's exclusive features

`nros-node` declares `scheduler-fifo`, `scheduler-edf`, `scheduler-bucketed`,
`scheduler-sporadic`: a pick-one family in an additive mechanism. Cargo
features are additive by contract — mutually exclusive features are officially
unsupported, and unification builds the union of what every consumer asked for.
Five `compile_error!` calls in `lib.rs` stand in for the constraint the
mechanism cannot express.

* Move the choice to a ladder knob with `exactly-one-of`.
* Delete the `compile_error!` guards rather than porting them; the resolver
  reports the conflict earlier and names both files.
* Leave every additive feature alone. This wave is about *exclusive* and
  *negative* configuration only.

**The rule this wave establishes.** Core may name names — it is ours. Core may
not own negative or exclusive configuration, because the mechanism it is
configured by cannot express either.

**Gate.** No `compile_error!` in core standing in for a config constraint.

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
