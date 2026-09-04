# Phase 206 — The user's own RMW config reaches the backend

*(Filename keeps the original `multi-homing-transport-interfaces` slug; the phase
NUMBER is the identity. The scope was rewritten 2026-09-04 — see "Rescoped".)*

**Goal.** Give nano-ros the ROS 2 experience: **the OS/board owns the devices,
and the user configures the middleware in the middleware's own language.** On
ROS 2 a user writes `CYCLONEDDS_URI` or a zenoh config naming an IP or a device,
and the application code knows nothing about NICs. nano-ros should be the same —
Ethernet/serial/CAN brought up by the board before ROS exists, and the user's
Cyclone XML or zenoh config attached on top, parsed by the backend that owns the
format.

**Multi-homing is then not a feature of nano-ros at all.** It is
`<General><Interfaces>` in a Cyclone config the user wrote. That is the whole
reason this phase was rescoped: the old plan built a nano-ros abstraction over
something the backends already have words for.

**Status (2026-09-04). Proposed, and its foundation is GONE.** Still unstarted
as proposed 2026-05-29 — but the "Landed" section below is no longer true, and
the way it stopped being true is the useful part.

**172.K.7's half really did land. Three separate retirements took most of it
back, and none of them knew it.**

| landed 2026-05-29 | removed by | when |
| --- | --- | --- |
| generator emits `c.set_interfaces(&[…])`, + the test `multi_homed_interfaces_emit_set_interfaces_call` | `11a00b0f8` — #202, "retire the dead standalone-package pipeline" (it deleted `orchestration/generate.rs` whole) | 2026-07-16 |
| `NanoRosConfig.cmake` + `NanoRosReadConfig.cmake` parsing `interfaces` → `NROS_CONFIG_INTERFACES` | `f473b78b4` / `53f2ddce7` — phase-212 M-F.10, "retire cmake codegen of `NROS_APP_CONFIG`" | phase-212 |
| the `config.toml` readers those parsers lived in | phase-256 W9, the legacy `config.toml` reader removal | — |

Each retirement had a good local reason and each was right on its own terms.
Together they dismantled this phase one plank at a time, and nobody updating
those commits was looking at this doc.

**What actually survives, verified on `main` 2026-09-04:**

* `PlanTransport.interfaces: Vec<String>` — `orchestration/plan.rs:684`
* its ethernet/wifi-only validation — `plan.rs:854-856`
* its copy into the IR — `model_ingest.rs:1457`
* three of the four tests — `plan.rs:1022, 1036, 1052`, all **parse/validate
  only**; none tests emission, because there is no emission
* `BoardTransportConfig::set_interfaces` — `nros-platform/src/board/config.rs:108`,
  a default no-op with **zero overrides and zero call sites tree-wide**

So the value parses, validates, reaches the IR, is written into
`<bake>/nros-plan.json` — **and is read by nothing.** `nros explain` cannot even
print it (`explain.rs:384-396` prints `ip`/`device`/`baudrate` only).

**This phase is now an instance of the class it would have to fix.** phase-349
names the shape while flagging its sibling: `NROS_NETSTACK` is *"emitted too …
and nothing reads it — the same declared-but-unread shape."* `set_interfaces` is
the second live instance. Any revival that adds another declaration without a
consumer repeats the defect.

*Original status, 2026-05-29:* Proposed. Extracted from Phase 172.K.7 (archived)
— its schema + plumbing half landed; this phase is the deferred
**wire-emission** half.

**Priority.** P2 — no shipped capability depends on it; meaningful only once a
multi-NIC target exists. Cyclone is the one backend where it's both meaningful
*and* testable today.

**Depends on.** Phase 172.K.7 schema/plumbing (landed); Phase 175.A (native
Cyclone build path) for the Cyclone config seam (206.3).

## Rescoped 2026-09-04 — from "multi-homing wire emission" to "config passthrough"

The original phase asked *"how does nano-ros express a NIC list and lower it to
each backend?"* Every answer to that question requires nano-ros to own a second
vocabulary for interfaces — plus a resolver, a gate, and a per-platform story
that only Linux can satisfy. `"eth0"` is a name in Linux's namespace; Zephyr's
`net_if_get_default()` cannot be named at all, and smoltcp has exactly one
`Interface` and no names. A portable `interfaces = ["eth0","eth1"]` is issue
0623's shape one layer up: a value authored in one vocabulary and resolved in
another, with nothing checking they agree.

**The principle that replaces it:**

> **nano-ros transports the user's backend config VERBATIM; it does not
> re-model it.** Devices are the board's job and are up before ROS exists. The
> middleware's configuration is the middleware's own format, parsed by the
> middleware's own parser.

This is what ROS 2 does, and it is why a ROS 2 user never writes an interface
list into their node.

## Overview

`[[transport]].interfaces = ["eth0", "eth1"]` already parses → `PlanTransport.interfaces:
Vec<String>` and validates (ethernet/wifi only). **The rest of this paragraph
was true in May and is not now** — there is no generator emitting
`set_interfaces` and there are no CMake parsers; see the status table above.
**Nothing binds an actual NIC**, and the seam is not merely inert, it is
unreachable: the only surviving consumer of the parsed value is the serializer
that writes it into `<bake>/nros-plan.json`.

Three blockers gate real binding, in dependency order: a multi-endpoint runtime
`SessionSpec` (206.1), the per-backend mapping (206.2 zenoh decision, 206.3
Cyclone `<Interfaces>` emission), and a multi-NIC target to verify against
(206.4). Distinct from Phase 172.K.5 (multi-domain = *segregate* sessions); this
is *merge* — one session, many NICs.

Design: [`docs/design/0004-configuration-and-transports.md`](../design/0004-configuration-and-transports.md)
("Two axes" taxonomy, cases B/C).

## Landed (Phase 172.K.7 schema + plumbing, 2026-05-29) — MOSTLY SINCE REMOVED

Read this list as the record of what K.7 built, not as the state of the tree.
The status table above says what took each piece back; **✓** survives, **✗**
does not.

- **✓** `PlanTransport.interfaces: Vec<String>` (serde default, skip-when-empty);
  `validate_transports` rejects it on serial/can (ethernet/wifi only).
  (`plan.rs:684`, `:854-856`)
- **✗** Generator emits `c.set_interfaces(&[…])` (mirrors `set_ssid`/`set_mac`),
  backed by a default-no-op `BoardTransportConfig::set_interfaces` seam.
  — the generator went with `orchestration/generate.rs` in `11a00b0f8`. The
  no-op trait method survives with no callers; `set_ssid` and `set_mac` lost
  their emitters in the same commit, so all three are dead together.
- **✗** Both CMake parsers (`NanoRosConfig.cmake`, nros-c `NanoRosReadConfig.cmake`)
  accept `interfaces = ["eth0","eth1"]` (legacy scalar `interface` mirrored in) →
  `NROS_CONFIG_INTERFACES` list var.
  — both files are gone; `NROS_CONFIG_INTERFACES` appears in no source or build
  output today, only in this doc and archived phase-172.
- **✓/✗** Tests: `transport_tests::{multi_homed_interfaces_parse_and_validate,
  interfaces_absent_round_trips_empty_and_skips_serialization,
  interfaces_are_ethernet_wifi_only}` **survive** (`plan.rs:1022, 1036, 1052`) —
  `multi_homed_interfaces_emit_set_interfaces_call` **does not**, deleted with
  `tests/orchestration_e2e.rs` in `11a00b0f8`.

> **Anyone reviving this phase should re-plan rather than resume.** 206.2 and
> 206.3 below name a "generator `set_interfaces` seam" as the thing to emit
> from, and that seam does not exist — an hour spent looking for it is the
> predictable cost of leaving this section unmarked. The surviving architecture
> reaches a backend by other routes (the RFC-0049 knob ladder already carries
> `NROS_BOARD_TOML` into `nros-zpico-build`), and the per-backend picture has
> changed too: zenoh-pico is structurally a client with one locator (peer mode
> is refused, `MULTICAST_TRANSPORT = false`), which bears directly on 206.2's
> open decision. None of that is decided here; this note only records that the
> plan below is written against a tree that no longer exists.

## What blocks it today — two measured defects

### Defect 1 — Cyclone: the user's config REPLACES the tuned baseline

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/session.cpp:310-313`:

```cpp
const char* user_uri = env_lookup("CYCLONEDDS_URI");
const char* cyc_config = (user_uri != nullptr && user_uri[0] != '\0') ? user_uri
                         : (kKconfigCycloneConfig[0] != '\0')         ? kKconfigCycloneConfig
                                                                       : kEmbeddedCycloneConfig;
dds_entity_t domain = dds_create_domain(domain_id, cyc_config);
```

A three-way ternary: exactly one string wins. Set `CYCLONEDDS_URI` and the whole
baked baseline is **gone** — `<Threads>` stack sizes (64 KiB for `dq.builtins`,
`recv`, `dq.user`), `<Sizing>` receive buffers, `<Internal>
<MultipleReceiveThreads>false`, and the platform's `AllowMulticast` choice
(`session.cpp:89-183`).

On FreeRTOS and ThreadX those stack sizes are load-bearing. **So a user doing the
exact thing this phase exists to support — attaching their own Cyclone config —
silently detunes their RTOS image today.** That is a live bug, not a missing
feature.

And it is gratuitous: **Cyclone composes natively.** Verified in the pinned tree
we link (`third-party/dds/cyclonedds`, `src/core/ddsi/src/ddsi_config.c:2538-2570`):

```c
copy = ddsrt_strdup (config);
cursor = copy;
while (*cursor && (isspace ((unsigned char) *cursor) || *cursor == ','))
  cursor++;
while (ok && cursor && cursor[0]) {
    if (tok[0] == '<')  qx = ddsrt_xmlp_new_string (tok, cfgst, &cb);   /* inline XML */
    else if ((fp = config_open_file (tok, &cursor, domid)) == NULL) ...  /* or a file */
```

Every comma-separated token parses into the **same `cfgst`**. Upstream's own
tests rely on it (`"${CYCLONEDDS_URI}${CYCLONEDDS_URI:+,}<Discovery>…"`).

### Defect 2 — zenoh: there is no user config surface off hosted-Rust

Two independent cut points:

* `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs:370` — the `ZENOH_*`
  env block is `#[cfg(feature = "std")]`, so **no embedded target reads it**.
* `packages/rmw/cffi/src/rust_adapter.rs:517` — the C boundary builds
  `RmwConfig { …, properties: &[] }`, hardcoded. So a **C or C++ entry gets
  nothing even on Linux**, and `rmw_session_options_t` is passed `nullptr` by
  every caller in the tree anyway (`packages/rmw/cffi/src/lib.rs:1884-1893`).

Only a hosted Rust caller building `TransportConfig` by hand can set `listen`,
`multicast_scouting` or `multicast_locator` — all of which zenoh-pico supports
and the shim already maps (`zpico-sys/c/zpico/zpico.c:1213-1237`).

### What is already right: device bring-up

The board contract already exists and already runs before ROS does — boot order
pinned at `nros-platform/src/board/entry.rs:15-20`:

    init_hardware -> init_transport -> wait_link_up -> open executor -> setup -> spin

`TransportBringup::init_transport()` (`board/transport.rs:28`) and
`NetworkWait::wait_link_up()` (`board/network.rs:14`) are exactly "the OS has the
devices ready". This phase states that as the contract and finishes it per
board; it does not invent it.

## Can we use the backends' NATIVE parsers? — measured 2026-09-04

The answer differs per backend, and the difference decides the design.

### Cyclone — YES, completely

`dds_create_domain(domain_id, config)` accepts **inline XML**: any comma token
beginning with `<` goes to `ddsrt_xmlp_new_string`, Cyclone's own parser
(`ddsi_config.c:2549-2551`). So a baked file's bytes can be handed over verbatim
as one token and **nano-ros parses no XML at all**. Composition and native
parsing come from the same mechanism.

### zenoh-pico — there is NO native config document to parse

zenoh-pico's entire configuration API is three functions
(`include/zenoh-pico/api/primitives.h:360-385`):

```c
z_result_t  z_config_default (z_owned_config_t *config);
const char *zp_config_get    (const z_loaned_config_t *config, uint8_t key);
z_result_t  zp_config_insert (z_loaned_config_t *config, uint8_t key, const char *value);
```

The key is a **`uint8_t`**, not a string: ~22 numbered constants
(`include/zenoh-pico/config.h:224-314`, `Z_CONFIG_MODE_KEY 0x40` through the TLS
block). There is no `zc_config_from_str`, no config file reader, and **no JSON5
parser** — `include/zenoh-pico/utils/json_encoder.h` is an *encoder* for the
admin space, inherited from upstream, not a config parser.

**JSON5 is zenoh-rs / zenoh-c's format, and this tree has no zenoh-rs backend** —
every crate under `packages/rmw/zenoh/` is zenoh-pico (`zpico-sys`,
`nros-rmw-zenoh`, `zpico-*`).

So: **adding a JSON5 parser would mean nano-ros owning a parser for a format its
own backend does not speak.** That is the exact mistake the principle forbids,
wearing a friendlier hat. zenoh-pico's *native* config language IS the numbered
key/value table, so the verbatim unit for zenoh is a **flat `key = value` file**
whose keys are the zenoh-pico key names the shim already maps. The work for
zenoh is therefore **unblocking a path that exists**, not building one.

*If a zenoh-rs backend is ever added, JSON5 becomes its native format and this
decision should be revisited for that backend only — never retrofitted onto
pico.*

## Work Items

### 206.W1 — Cyclone composes; the baseline always survives
- [ ] Replace the three-way ternary at `session.cpp:310-313` with composition:
      the baked baseline first, then the Kconfig blob, then the user's config,
      joined by `,`. Cyclone merges them into one `cfgst` and later tokens
      override earlier keys, so the user still wins on anything they state —
      they just stop silently losing everything they did not.
- [ ] **Acceptance:** on a FreeRTOS or ThreadX image, a user config that sets
      only `<Discovery>` leaves `<Threads>` stack sizes intact — asserted by
      reading them back, not by the image merely booting. The current code fails
      this test.
- [ ] Verify override precedence in the pinned Cyclone rather than assuming it;
      if later tokens do NOT win, the order flips and the acceptance test is what
      tells us.

### 206.W2 — one config-passthrough seam, present on every target
- [ ] A bringup names a config FILE per backend; the build bakes its bytes into
      the image. The user writes real Cyclone XML — not a nano-ros re-spelling,
      and not a Kconfig string with single-quoted attributes.
- [ ] Hosted keeps `CYCLONEDDS_URI` working as the outermost rung. Embedded
      needs the baked file, because `env_lookup` returns `nullptr` on
      freestanding targets (`env_compat.hpp:38-45`) — **the env rung is
      structurally dead there**, which is why `CONFIG_NROS_CYCLONE_CONFIG_XML`
      exists at all (issue 0367).
- [ ] `CONFIG_NROS_CYCLONE_CONFIG_XML` stays as the Zephyr-native rung; the
      baked file is the portable one. Both compose under W1, so they are rungs
      rather than rivals.
- [ ] **Acceptance:** the same Cyclone XML file, byte-identical, takes effect on
      a hosted build and on one RTOS build.

### 206.W3 — zenoh gets a config surface at all
- [ ] Unhardcode `properties: &[]` at the C boundary
      (`rust_adapter.rs:517`) so a C/C++ entry can carry properties.
- [ ] A baked `key = value` file for embedded, keys being the names
      `zpico.c:1213-1237` already maps to the numbered constants. Unknown keys
      are currently **silently ignored** (`zpico.c:1238-1240`) — this phase makes
      an unknown key an error, because a silently dropped config line is the
      failure mode the whole phase is about.
- [ ] **Acceptance:** `listen` reaches `Z_CONFIG_LISTEN_KEY` from a C entry on
      Linux and from a baked file on an RTOS.

### 206.W4 — device bring-up stated as the board's contract
- [ ] Document `init_transport` + `wait_link_up` as *the* device contract, with
      the boot order, in the board-authoring docs; a board that cannot bring a
      device up says so rather than leaving a default no-op.
- [ ] Audit which boards implement each. Today `nros-board-zephyr` is the only
      `NetworkWait` impl in the tree, and it hard-codes `net_if_get_default()`.
- [ ] **Acceptance:** every board either implements the contract or declares it
      does not apply, and the survey is in the doc.

### 206.W5 — delete nano-ros's interface abstraction
- [ ] Remove `BoardTransportConfig::set_interfaces`
      (`nros-platform/src/board/config.rs:108`) — zero callers, wrong layer
      (RFC-0049's duty rule: *platform toml = software-stack facts, board toml =
      hardware facts*), and a silent default no-op.
- [ ] Remove `PlanTransport.interfaces` and its validation, or keep it only as a
      deprecated alias that ERRORS with a message naming the backend config file.
      It currently parses, validates, reaches the IR, is serialized into
      `<bake>/nros-plan.json`, and is read by nothing.
- [ ] `set_ssid` / `set_mac` / `set_gateway` / `set_password` lost their emitter
      in the same commit (`11a00b0f8`) and are dead by the same argument. Sweep
      them together or state why not — the repo's fix-the-class rule.

### 206.W6 — Fast DDS whitelist (unchanged: out of scope)
- [ ] Not actionable; no Fast DDS backend exists. Under the new principle it
      needs no nano-ros work at all when one does — a Fast DDS user writes a Fast
      DDS profile, and W2's seam carries it.

## Acceptance

- [ ] A user attaches a Cyclone XML naming an IP or a device, and it takes
      effect **without losing the baked baseline** — proven on an RTOS target,
      where losing it is destructive.
- [ ] The same file works byte-identically hosted and embedded.
- [ ] A zenoh user sets `listen` from a C entry and from an embedded image.
- [ ] Multi-homing is demonstrated **with no nano-ros feature involved**: a
      hosted Cyclone node reachable on `lo` and one real NIC, configured purely
      by `<General><Interfaces>` in the user's own XML. This is the original
      phase's acceptance criterion, met by deleting the mechanism it proposed.
- [ ] `nano-ros parses neither XML nor JSON5.` Cyclone's parser reads the XML;
      zenoh's key/value table takes the pairs.

## What this phase DELETED, and why

**206.1, the multi-endpoint `SessionSpec`.** Multi-homing on Cyclone is a config
property of ONE participant, not N endpoints on one session, and no backend here
expresses it as multiple runtime endpoints. `open_multi` already exists and
correctly SEGREGATES — it is phase-172 K.5, and it should stay that way
(`nros-node/src/executor/spin.rs:300-388`).

**Growing `rmw_session_options_t`.** It is
`{u8 localhost_only; u8 _reserved[7]; const char *enclave;}` and **nothing in
the tree ever passes it non-NULL**. A list-shaped field would break the struct,
force a bindgen regen, and touch the hand-mirrored C++ FFI structs that have
already drifted three times (CLAUDE.md). Config does not need it.

**An intermediate design (drafted and discarded 2026-09-04):** multi-homing as
an `[rmw.capabilities]` entry, a `[board.links]` inventory modelled on
`[board.priority_plan]`, and a per-platform name resolver. It was better placed
than `set_interfaces` and still wrong — it kept nano-ros in the business of
modelling interfaces. Recorded because the reason it was discarded is the
principle at the top of this doc.

## The constraint any revival must satisfy

`set_interfaces` and `NROS_NETSTACK` are **two live instances of declared-but-
unread** — phase-349 names the shape while flagging its own:
*"`NROS_NETSTACK` is emitted too … and nothing reads it — the same
declared-but-unread shape."* Every work item above pairs a declaration with a
consumer AND an acceptance test that fails today. A third instance is the one
outcome that would leave the tree worse than not doing this phase.

## Notes

- **Merge vs segregate.** phase-172 K.5 (multi-domain) opens one session *per*
  domain and is correct as-is. Under this phase there is no "merge" primitive to
  build: merging NICs is something Cyclone does inside one participant when its
  config says so.
- **Why the original framing was reasonable.** In May 2026 the tree had a
  `config.toml` reader and a generated standalone package, so a nano-ros-owned
  interface list had somewhere to live. Three retirements removed that world (see
  the status table). The rescope follows the architecture, it does not overrule
  the original author.
- **phase-419's gate saw this doc and let it pass.** Its own W3 measurement
  records `phase-206  ticked=0  landed-marks=3  weak — likely prose, not a
  claim`. The marks were a real claim. R1 keys on ticked boxes and this doc has
  none, so the miss is structural rather than a tuning error — worth knowing
  before anyone widens that rule.
