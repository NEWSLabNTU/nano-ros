# RFC-0086 — Unified configuration: the transport tenant, coupling verbs, and retiring the per-lane knob

**Status:** Draft (2026-08-30)
**Amends / refines:** RFC-0049 (hierarchical platform/board config) — gives its
ladder reach and adds knob-to-knob constraints; RFC-0071 (RMW backend
descriptor) — adopts D8's one-family rule and closes the leak D8 names.
**Motivated by:** bringing the safety island up on MR-CANHUBK344 over serial,
where selecting one transport meant hand-writing fifteen Kconfig lines that
nothing in the config system could derive.

## Summary

RFC-0049 built a four-rung ladder and `nros config explain` prints it. It
carries **three knobs**. The settings that actually shape an image travel by
three other roads: **78** Zephyr `CONFIG_NROS_*` symbols, **21** `NROS_*` and
**9** `ZPICO_*` environment variables read in build scripts.

This RFC does not propose a new mechanism. It gives the existing one reach, on
three axes:

1. **A tenant that crosses every descriptor** — `transport`, chosen because it
   is the one setting the backend, the platform and the board all have an
   opinion about.
2. **Knob-to-knob constraints** — `requires` / `implies` / `exactly-one-of`,
   with strengths borrowed deliberately from Kconfig and Gentoo.
3. **A rule for core packages** — they may name each other, but they may not
   own negative or exclusive configuration, because Cargo features cannot
   express it.

It also fixes an inconsistency in RFC-0049's own implementation and retires the
paths the tenant replaces.

## Problem

### The defect, in four lines

```kconfig
config NROS_ZENOH_LINK_TCP
    bool "TCP link"
    default y
    # no `depends on`, no `select`
```

`default y`, with no dependency on the IP stack. Three consequences, all
observed:

* With `CONFIG_NETWORKING=n` it stays on, and the build dies on `AF_INET` /
  `socklen_t` / `struct sockaddr`. That is the failure `Z_HAS_SOCKET_LINK`
  papers over *inside zenoh-pico*, because the config layer could not express
  it.
* Selecting serial does not turn TCP off.
* Nothing connects *serial only* to *Ethernet driver off*. The MAC, MDIO and
  PHY are `default y` behind devicetree nodes the board has enabled, so they
  arrive on their own and must be turned off by name.

The result is `src/zephyr_entry/snippets/island-serial/serial.conf` in the
safety-island tree: fifteen hand-written lines that a config system with
constraints would derive. That file is per-image ad-hockery, and the next
board repeats it.

### Three axes, three resolution stories

RFC-0049 opens by rejecting a central file: an out-of-tree platform *"cannot
join `zenoh_platforms.toml` without forking the tree."* The board and rmw axes
honour that. The platform axis does not.

| axis | descriptor lives | resolved by |
| --- | --- | --- |
| rmw | `packages/rmw/<b>/…/nros-rmw.toml` | name, over a search path of workspaces |
| board | `packages/boards/<b>/nros-board.toml` | registry name |
| **platform** | **`config/<name>/nros-platform.toml`** | **one directory root** (`--platforms-dir`) |

A third-party platform must fork `config/` or repoint the whole root and lose
the in-tree platforms with it.

### Core packages, and the one difference that is load-bearing

Core is ours; it may name its own parts. That is a fair reading and this RFC
does not disturb it. What it does not extend to is the *mechanism*.

Cargo features are **additive by contract**: enabling one must not disable
functionality, mutually exclusive features are officially unsupported, and
under feature unification a shared dependency is built with the union of what
every consumer asked for. There is no subtraction.

Two consequences, both already in the tree:

* **Core cannot be a front-end for "off".** RFC-0049 requires a front-end to
  express *off over an on-default* — the reason the Zephyr forward must pass
  `-DZPICO_X=0|1` rather than only passing on `y`. A Cargo feature cannot do
  this.
* **The tree does NOT currently violate this** — corrected 2026-08-30, after
  an audit that phase-400 W7 ran. An earlier draft of this RFC claimed
  `nros-node`'s `scheduler-fifo` / `-edf` / `-bucketed` / `-sporadic` was a
  pick-one family in an additive mechanism. It is not: the comment above those
  declarations states *"Each flag is independent; multiple may be on
  simultaneously when runtime selection across classes is needed"*, and the
  `cfg` sites agree. The five `compile_error!` guards in `lib.rs` are feature
  IMPLICATIONS (`param-services` needs `alloc`), not exclusivity. Every
  `cfg(not(feature = …))` in core is the correct no_std shape — start without,
  add capability — and `packages/api/nros/src/lib.rs` records that a platform
  mutual-exclusion guard was deliberately *removed*.

  The rule below therefore constrains future work rather than describing a debt
  to pay. It is stated because the pressure to encode an exclusive choice as a
  feature is real and recurring, not because the tree has yielded to it.

So the rule is not "core is exempt". It is: **core may name names; core may
not own negative or exclusive configuration.**

## Prior art

RFC-0049 surveyed Zephyr, ESP-IDF, cargo, systemd drop-ins and NixOS modules.
Four more bear directly on the two open questions.

| system | what it settles | what we take |
| --- | --- | --- |
| **Kconfig** (`depends on` / `select` / `imply`) | Three strengths, not one. `select` forces a symbol and can produce invalid configurations — the long-standing "select issue". `imply` is weak: it suggests, respects dependencies, and the user may still override. | The strengths, not the syntax. Our `implies` is `imply`-strength, **never** `select`. |
| **Gentoo** (`REQUIRED_USE`, GLEP 73) | Declarative constraints over flags with a group vocabulary — `any-of`, `exactly-one-of`, `at-most-one-of` — plus a specification for *automatically enforcing* them rather than only reporting. | The vocabulary. `transport.kind` is exactly `exactly-one-of`. Enforcement is the goal, not just validation. |
| **Cargo features** (RFC 3692) | Additive-only, unified across consumers, mutually exclusive unsupported. | A bound on the design: features pull code in, never configure it off. |
| **Yocto layers** (`BBFILE_PRIORITY`) | Numeric per-layer priority decides which recipe wins — but priority does *not* apply to `.conf` files, which fall back to list order. | A caution. A fixed, short, named ladder beats numeric priorities that apply unevenly by file kind. |

The convergent lesson: every system that got this right separates **facts**
from **policy** and gives constraints a **strength**. Kconfig learned it by
adding `imply` after `select` proved too strong; Gentoo learned it by
specifying enforcement after years of constraints that only complained.
RFC-0049 already has the fact/policy split — *"capabilities are facts; knobs
are policy"* — and already lands on the right side of both. What it lacks is
reach.

## Design

### D1 — `transport` as the first cross-cutting tenant

The user states intent once. Each axis contributes what only it knows.

```toml
# system.toml — the only place a human states intent
[transport]
kind     = "serial"          # exactly-one-of: serial | tcp | udp
endpoint = "uart2@115200"    # the board resolves the name

# nros-platform.toml — facts, not policy
[capabilities]
ip_stack = true              # this platform CAN do tcp, if asked
serial   = true

# nros-rmw.toml — the backend's own lowering
[rmw.transport.serial]
locator        = "serial/{device}#baudrate={baud}"
requires_links = ["serial"]

[rmw.transport.tcp]
locator        = "tcp/{host}:{port}"
requires       = ["ip_stack"]      # a capability, never a symbol
requires_links = ["tcp"]
```

Duty split, unchanged from RFC-0049 and extended to the RMW axis:

* **backend** knows how to *spell* an endpoint — a zenoh locator string, an
  XRCE `addr`/`port` pair, a Cyclone URI.
* **platform** knows what *exists* — `ip_stack`, `threads`.
* **board** knows what is *wired* — which UART reaches a connector, which PHY
  is populated.

This replaces three incompatible spellings of one concept:
`NROS_ZENOH_LOCATOR` (one string with the transport embedded),
`NROS_XRCE_AGENT_ADDR` + `NROS_XRCE_AGENT_PORT` (two symbols, IP-only), and —
for Cyclone — no nros symbol at all.

### D2 — three verbs, with strengths

```
transport.kind = "serial"
  requires  capabilities.serial
  implies   links.tcp = off, links.udp = off
  implies   drivers.ethernet = off, drivers.phy = off, drivers.mdio = off

transport.kind = "tcp"
  requires  capabilities.ip_stack
  implies   links.tcp = on
```

| verb | strength | on conflict |
| --- | --- | --- |
| `requires` | hard — a fact must hold | build error naming both files |
| `implies` | weak — Kconfig `imply` | a higher rung still wins; the override is **recorded and printed** |
| `exactly-one-of` | group — from `REQUIRED_USE` | the shape of `transport.kind` itself |

The strength distinction is the whole lesson from `select`. A forcing verb
would let `transport.kind = "serial"` silently stamp out an explicitly
requested TCP link and produce a configuration nobody asked for. Weak
implication keeps the escape hatch and makes the resolver say when it was used.

**Where implication lives, and why.** In the resolver, not in Kconfig.
`depends on NET_SOCKETS` would fix the Zephyr build and nothing else — the
cargo and CMake lanes have no equivalent, so a board built through either still
needs the rules hand-written. The rule belongs where every lane sees it; the
Kconfig `depends on` becomes a **mirror**, held by the drift test RFC-0049
already mandates.

### D3 — the platform axis resolves like the others

Platform descriptors move from `config/<name>/nros-platform.toml` to
`packages/platform/nros-platform-<name>/nros-platform.toml`, resolved by name
over a search path exactly as RFC-0071 D5 resolves backends.
`--platforms-dir` / `$NROS_PLATFORMS_DIR` remain as an explicit override for a
single root, but stop being the only way in.

This is what makes RFC-0049's own porter promise true: *2 crates + 2 tomls,
edits nothing central.*

### D4 — close the D8 leak

All seven platform files carry `[build.zenoh]` and `[knobs.zenoh.tx]` —
backend-named sections in a platform file, which RFC-0071 D8 already names as a
violation. It is not theoretical: migrating to zenoh-pico 1.10 required editing
`config/freertos` and `config/posix`, platform files, for a vendored-library
path change.

Key them on the resolved backend: `[build.<rmw>]`, `[knobs.<rmw>.tx]`. A
platform then declares "here are my settings for whichever backend is
selected", and a third-party RMW can receive platform settings without either
side learning the other's name.

### D5 — the core-package rule

* Cargo features may **pull code in**. They may not express "off", and they may
  not encode a pick-one family.
* Any knob whose correct value is sometimes "off", and any exclusive choice,
  belongs in the ladder with a lane front-end that can carry `0` as well as `1`.
* No migration is outstanding. The audit found no exclusive or negative
  configuration expressed as a Cargo feature in core or api. The rule is a
  standing constraint on new knobs, enforced at review, not a backlog item.

### D6 — provenance is the acceptance test

`nros config explain` already prints value + rung. It must additionally print,
for every knob the resolver touched:

* the rung that set it (builtin / platform / board / front-end),
* whether it was **implied**, and by which rule,
* whether an implication was **overridden**, and by which rung.

Opaque layered merges are the known failure mode of every layered-config system
surveyed. A resolver that cannot explain itself is not finished.

## What this does NOT change

* The four-rung ladder and its order. RFC-0049's precedence is correct.
* The runtime seam. `nros_rmw_vtable_t` (RFC-0035) is untouched.
* Kconfig's role as a **front-end** for Kconfig-native frameworks. There is
  still no nros-owned Kconfig and no Kconfig generation.
* RFC-0045's boot-config resolution, which is the runtime half of the same
  story and lands separately.

## Verification

* `nros config explain` shows `transport.*` with provenance, including implied
  values and overrides.
* A serial-only image builds with **no hand-written link or driver lines** —
  `serial.conf` in the safety-island tree shrinks to the transport stanza.
* `transport.kind = "tcp"` on a platform whose `[capabilities]` lacks
  `ip_stack` fails the build naming both files, rather than failing at link
  time on `AF_INET`.
* A third-party platform package resolves without `config/` being forked, and a
  third-party RMW receives platform knobs without either naming the other.
* The Kconfig mirror drift test passes with the link symbols carrying
  `depends on`.

## Open questions

* **Enforcement vs validation.** GLEP 73 specifies automatic enforcement.
  `implies` is enforcement; `requires` is validation. Is there a case for
  solving — picking a satisfying assignment — or does that reintroduce the
  opacity D6 exists to prevent? Current position: no solver.
* **`at-most-one-of` and `any-of`** are in the Gentoo vocabulary and not yet
  needed here. Add on first real use, not before.
* **Board-level transport defaults.** A board that has exactly one wired UART
  could default `transport.endpoint`. Useful, but it puts policy in a facts
  file; deferred until a second board wants it.
