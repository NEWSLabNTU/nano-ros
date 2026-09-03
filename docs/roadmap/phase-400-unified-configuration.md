# Phase 400 — the unified config system: build it, migrate onto it, retire the rest

**Status (2026-09-01): W2-W5, W7 and W8 done; W6's `executor`, `transport`,
`zenoh.tx`, `memory`, `params`, `rmw`, `net`, `runtime`, `zenoh.wire`, `xrce` and `zenoh.limits` tenants done, its remaining tenants and W1
outstanding.** Design is
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

### `[knobs.memory]` — the platform heap and stack — DONE (2 of 3)

The tenant the re-scope below identified as the clearest one left: numbers that
genuinely vary by platform and board, that no derivation campaign owns.

`NROS_FREERTOS_HEAP_KB` and `NROS_FREERTOS_APP_STACK_KB` now resolve over the
full ladder and print in `nros config explain`. `NROS_ZEPHYR_HEAP_SIZE` is the
third and is NOT done — it is read by `option_env!` in `nros-platform`'s
source with its own build-script resolver, so it needs that crate to take the
`nros-board-common` build-dep first, the same decision phase-400 W6 already
made for `nros-node`.

**Stored in BYTES, always.** The front-ends disagree about units — the FreeRTOS
pair is KiB, Zephyr's is bytes — and a table where "heap" means one thing on
one platform and another elsewhere is a unit bug waiting to be written. The
env names keep their spellings and the rung converts, in one place.

The env-pointer dance is now written ONCE, as
`platform_config::BuildRungs::from_build_env()`. `nros-node/build.rs` grew its
own copy when the executor tenant landed; two build scripts resolving the same
rungs differently is precisely the drift `check-knob-single-reader` exists to
catch one level up, so the shared version is the one to use and that copy
should adopt it.

Verified against the running tool: builtin, platform rung, and the env
front-end converting KiB to bytes (`NROS_FREERTOS_HEAP_KB=4096` → 4194304).

### The zenoh tenant is NOT W6's — re-scoped 2026-09-01

Going to migrate it revealed that most of it belongs to a different campaign,
and this is the second family in a row where that is true.

`ZPICO_MAX_QUERYABLES` is already a CHECKED OVERRIDE over a derived default
(phase-392 W5.f: "stops being an independent opinion"). `ZPICO_MAX_SESSIONS`
is posed by that phase explicitly — "either it joins the model or it stays a
knob and this phase says so". `SERVICE_BUFFERS` is a product of the two. So the
zenoh entity caps and pools are phase-392's question, and giving them a
platform/board rung now would answer it the wrong way: a rung gives a global a
per-platform default, and phase-392 is deciding whether these stop being
globals at all.

Same shape as the buffers one family over (phase-403). The census marks both
`derived` and names the owning campaign, so the backlog count stops inviting
the mistake.

**W6's own backlog is therefore 23, not 31** (re-measured at 28 after the
census fix below), and its largest families are:

| family | knobs | note |
| --- | --- | --- |
| `NROS_MAX_*` | 5 | **DONE** — the `params` tenant, below. |
| `NROS_RUNTIME_*` | 4 | component caps. Worth asking whether these are derivable from the declared component set before migrating — the same question phase-392 asks of the zenoh caps. |
| platform heap / stack | 3 | `NROS_ZEPHYR_HEAP_SIZE`, `NROS_FREERTOS_HEAP_KB`, `NROS_FREERTOS_APP_STACK_KB`. Genuine platform facts with no derivation candidate — **the clearest W6 tenant left.** |
| `ZPICO_*` remainder | 7 | wire batch, fragmentation, reply staging, two transport-band priorities |
| singletons | 4 | keyexpr bound, LET buffer, service timeout, XRCE MTU |

### The singletons — five migrated, three with reasons not to

`[knobs.xrce]` (`custom_transport_mtu`, `stream_history`),
`[knobs.zenoh.limits]` (`keyexpr_string_size`, `subscriber_ring_depth`) and the
LET buffer folded into `[knobs.runtime]`.

**Three did NOT migrate, and each refusal is the interesting part.**

`NROS_SERVICE_TIMEOUT_MS` has TWO readers by design — `nros-rmw-zenoh/build.rs`
emits a Rust const and `nros-build-helpers`'s C emitter emits a `#define`, and a
comment asked the next editor to keep the defaults equal. I migrated it, and
`check-knob-single-reader` refused: a migrated knob gets exactly one reader, and
that rule is precisely what stops the pair drifting. Both would have resolved
through the same ladder and so could not disagree — but the gate cannot know
that, and weakening it to say so trades a checked invariant for a comment.
Migrating this knob means giving the pair ONE emission point first, which is a
change to where the value is emitted rather than to the ladder.

`NROS_ENTRY_SPIN_MS` is read by a proc macro, the C++ library and a C header.
Same shape, three readers, and no single build script to own it.

The two transport-band priorities stay out for the reasons in the `zenoh.wire`
section above.

Backlog after this: **5**, from 10.

### The `zenoh.wire` tenant — done, and the two priorities left out

Five wire sizes — the unicast and multicast batch buffers, the fragmentation
ceiling, the get-reply staging block and its poll interval — now resolve through
the ladder as `[knobs.zenoh.wire]`, beside the existing `[knobs.zenoh.tx]`.

This does NOT contradict "the zenoh tenant is not W6's" above. That re-scope was
about the entity CAPS and pools — `ZPICO_MAX_QUERYABLES`, `ZPICO_MAX_SESSIONS`,
`SERVICE_BUFFERS` — which phase-392 is deciding the shape of. These five are
sizes of the wire itself, and the same re-scope listed them as W6's remainder.

**The two transport-band priorities are deliberately NOT here.**
`ZPICO_READ_TASK_PRIORITY` and `ZPICO_LEASE_TASK_PRIORITY` look like the rest of
the family and are not: the build script's defaults MIRROR the `#define`
fallbacks in `zpico-sys/c/zpico/zpico.c`, and `FreertosScheduling` already
carries a per-board `zenoh_read_priority` / `zenoh_lease_priority` in raw
FreeRTOS units. A ladder rung would be a THIRD path to one number, which is the
drift `check-knob-single-reader` exists to prevent. Their real question is
ORDERING against the app tiers (issue 0623), which is a policy the board already
expresses — not a size a platform defaults.

The builtins stay the CALLER's: `nros-zpico-build` computes a batch and
fragmentation size from the platform's transport before any descriptor is read,
so `resolve_wire` takes them as its `defaults` and applies the rungs above them.

Verification note, stated plainly: the resolver is pinned by a unit test over
all four rungs. The build script's half is five straight-line assignments
mirroring the tx trio two lines above them, and it is NOT verified by an
emitted-artifact probe — the header it writes is produced inside example build
trees, and every probe I wrote observed the wrong tree.

Backlog after this: **10**, from 15.

### The `runtime` tenant — done

The four static pools the component runtime is carved from —
`NROS_RUNTIME_MAX_COMPONENTS`, `..._COMPONENT_SLOT_BYTES`,
`..._MAX_CLASS_INSTANCES`, `..._MAX_CELL_ENTITIES` — now resolve through the
ladder: `RuntimeKnobs`, `[knobs.runtime]`, `BuildRungs::runtime_rungs()`, and
one reader in `packages/api/nros/build.rs`.

**phase-391 is a CONSUMER, not the owner.** It emits `config::MAX_COMPONENTS`
and friends from these numbers and sizes the arena from them; it never decided
their values. That is the same relationship `nros-node/build.rs` had with the
executor knobs before this wave, and it is why the ownership check cleared:
reading a knob is not owning it. phase-412 does not claim them either — its W1
six and its W2 blocked-list both name other things.

Verified against the emitted constants:

    builtin        4 / 512 / 2 / 8
    platform rung  9 / 256 / 5 / 3
    env wins       MAX_COMPONENTS=12, the rest keep the rung

Backlog after this: **15**, from 19.

### The `net` tenant — done, and the reason the reader moved

Five smoltcp knobs — the TCP and UDP socket pools, the per-socket buffer, and
the connect/socket timeouts — now resolve through the ladder: `NetKnobs`,
`[knobs.net]`, `BuildRungs::net_rungs()`, and one reader in
`packages/drivers/net/nros-smoltcp/build.rs`.

**This tenant is why `nros-platform-config` exists.** `nros-smoltcp` could not
depend on `nros-board-common`: cargo counts an optional dependency when it looks
for cycles, and the board crate reaches back through `nros-platform ->
nros-platform-esp32-qemu -> nros-smoltcp`. Every driver was locked out of the
ladder. The reader moved to a leaf crate, and this is the first tenant to use it.

Two things the rung does NOT flatten. `max_udp_sockets`'s builtin is
FEATURE-derived (1 brokered, 4 with `rtps`), and it stays that way — a
descriptor naming the knob outranks it, but a board that says nothing still gets
the feature-appropriate number rather than a constant. And the deprecated
`ZPICO_SMOLTCP_*` spellings rank WITH env, above the rungs, which is what a
deprecated front-end should do.

Verified against the build script's own emitted constants:

    builtin        1 / 1 / 2048 / 10000
    platform rung  3 / 2 /  512 /   250
    env wins       MAX_SOCKETS=4, the rest keep the rung
    legacy alias   ZPICO_SMOLTCP_BUFFER_SIZE=99 also outranks the rung

Backlog after this: **19**, from 24.

### The `rmw` tenant — done, minus the one phase-412 took

Three static pools the CFFI registry and node table are carved from —
`NROS_RMW_MAX_BACKENDS`, `NROS_RMW_MAX_NODES`, `NROS_RMW_MESSAGE_INFO_SLOTS` —
now resolve through the ladder: `RmwKnobs`, `[knobs.rmw]`,
`BuildRungs::rmw_rungs()`, and one reader in `packages/rmw/cffi/build.rs`. The
build script keeps its RANGE checks, which belong to the array it carves rather
than to the ladder.

**`NROS_RMW_SUBSCRIBER_SLOTS` was in this family and is NOT in this tenant.** It
sits in the same build script, three lines away, and looks identical — but
phase-412 W1 landed on 2026-09-03 deriving it from the entity inventory
(`COUNT_SUBSCRIPTION`). A knob two campaigns resolve is exactly the drift issue
0938 cost, so it stays on env -> Kconfig -> builtin and takes its platform
answer from the derivation. The census now classes it `derived` and names the
owner.

That check is the third time it has changed the plan: the zenoh tenant belonged
to phase-392/403, the buffers to phase-403/408, and now a quarter of the `rmw`
family to phase-412. **Checking for an owning campaign is not a formality in
this repo — it has been right more often than the plan was.**

phase-412 also derives two knobs THIS wave already migrated
(`NROS_EXECUTOR_MAX_NODES`, `NROS_EXECUTOR_ACTION_CLIENTS`). That is not a
conflict: its precedence is env > Kconfig/board > derived > crate default, so
the ladder's rungs still win and the derivation fills the builtin slot. Worth
knowing before someone reads the two docs and assumes one must be wrong.

Backlog after this: **24**, from 28.

### The `params` tenant — done

The five `NROS_MAX_*` PARAMETER value bounds (not message bounds — that mislabel
is corrected in the census) now resolve through the ladder: `ParamKnobs`,
`[knobs.params]` in a platform or board toml, `BuildRungs::param_rungs()`, and a
single reader in `nros-params/build.rs` composing env -> Kconfig -> rung ->
builtin. `nros config explain` prints them beside the executor and memory
tenants.

No campaign owns these; the only doc hit was
`phase-292-asi-reference-consumer-revisit.md`, and it is a usage RECORD —
"consumer-side knobs that made it fit: `NROS_MAX_PARAMETERS=256`" — living in a
consumer's `build.sh`. That is the argument FOR a rung, not against one: a
consumer discovering a platform-appropriate value and having nowhere to put it
is the gap the ladder closes.

### The census had been under-counting by 29

Wiring this tenant's gate turned up a defect in the measuring instrument. The
census found readers by matching a fixed list of helper NAMES
(`env_usize`, `env::var`, ...), so `nros-params/build.rs` wrapping its rungs in
a local `knob()` took its five knobs out of the count while `--check` stayed
green — the gate only fails on a name it SEES and cannot classify.

Matching any call instead is the opposite error: `.define("ZPICO_X", ..)` emits
a C macro and `.with_env(..)` sets a CHILD's environment, and counting those
added 67 phantom names. So the matcher now carries `READ_CALLEES` and
`NON_READ_CALLEES`, and a callee in NEITHER is a failure — the same "a new knob
forces a decision" rule this census already applies to knobs, applied to the
idioms that read them.

That surfaced 29 names read through `req` / `list` / `env_get` / `flag`, which
the fixed list never knew: 18 infra (board descriptor facts, include and source
dirs) and **11 sizing** — the RMW static pools, the smoltcp driver pools and
timeouts, the XRCE stream depth, and the zpico subscriber ring depth. So W6's
backlog is **28, not 17**; it went up because the instrument stopped lying. Two
candidate tenants fall out of it, an `rmw` one and a `net` one, neither owned by
another campaign.

**The lesson, and it is not the same one as below:** a counting gate that
under-counts silently is worse than no gate, because a wrong baseline looks
measured. Both of this census's counters now have negative controls on synthetic
input for exactly that reason.

**The lesson for the wave, stated once:** "migrate the long tail into the
ladder" was the right instinct for the executor tenant and is the wrong default
for the rest. A number that can be DERIVED from what the image declares should
be, and the ladder is for the ones that genuinely vary by platform or board.
Check for an owning campaign before migrating a family.

### Ordering, from the census

The wave opened with "then the zenoh pools, then the per-entity caps", by
estimate. The census contradicts it twice over: the caps are not separate from
the pools (they are all zenoh's), and zenoh is not W6's to migrate at all — see
the section above. What is left after removing the derivation campaigns' work
is the table there, and the clearest tenant in it is the platform heap/stack
trio, which no campaign is deriving and which genuinely varies by platform.

Kconfig is unchanged by this: 85 declarations, largest family zenoh (24), then
XRCE (8), `NROS_MAX` (7), RMW (5), Zephyr (4). Whether those follow their env
counterparts into another campaign has not been checked.

### Thirteen knobs must NOT be migrated

Five are receive/transmit buffers (phase-403 / phase-408) and eight are the
zenoh entity caps and pools (phase-392) — see the re-scope above.

`NROS_SUBSCRIPTION_BUFFER_SIZE`, `ZPICO_SUBSCRIBER_BUFFER_SIZE`,
`ZPICO_SUBSCRIBER_LARGE_SIZE`, `ZPICO_SUBSCRIBER_SIZE_THRESHOLD` and
`ZPICO_PUBLISHER_TX_BUFFER_SIZE` are receive/transmit buffer sizes that
[phase-403](phase-403-type-bound-rx-sizing.md) and
[phase-408](phase-408-cpp-message-derived-buffers.md) are making **derived from
the message type**. A ladder rung would give each a per-platform default —
entrenching the global those phases exist to remove. The census classifies them
`derived` so nobody migrates one by following the backlog count.

`NROS_SUBSCRIPTION_BUFFER_SIZE` is the awkward one: it is ALREADY on the
ladder, from this wave's executor tenant, and that is fine — it stays as the
fallback for a type with no declared bound. It just must not be treated as
finished business.

### Two corrections to earlier numbers here

* **The census scanned only `build.rs`.** A build script that grew past a few
  lines moved its body into a helper crate and the knobs went with it: 21
  `ZPICO_*` names live in `nros-zpico-build/src/`. Both this doc's original
  "`ZPICO_*` env | 9" and the census's first "6" undercounted the zenoh surface
  roughly fourfold. It now scans `*-build` crates and `packages/tooling/` too.
* **The env row counted things that are not knobs.** 19 of the 64 names are
  paths, flags, or the ladder's own inputs — including `NROS_PLATFORM_NAME`,
  which this wave ADDED, so the raw count rose while the backlog fell. Names
  are classified explicitly now, and an unclassified name FAILS the gate: a new
  knob has to be decided about (ladder? derived? infra?) rather than guessed at
  by a heuristic, which is how a backlog number stops meaning anything.

---

<!-- Restored 2026-09-01. W7, W8 and the scope note below were deleted by
     accident in the phase-400 W6 census commit: a section replacement anchored
     on "### Remaining tenants" ran to END OF FILE and took everything after it.
     Recovered verbatim from caa634aff~1. -->

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
