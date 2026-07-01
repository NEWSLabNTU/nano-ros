---
rfc: 0047
title: "Unified sched-context binding via callback groups — code-declared groups, config-assigned tiers"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-272, phase-273]
supersedes: []
superseded-by: null
---

# RFC-0047 — Unified sched-context binding via callback groups

## Summary

Scheduling in nano-ros binds a callback to a **sched-context** (a tier: priority + spin period). This
RFC makes that binding **one mechanism across Rust, C, and C++**, at the granularity ROS 2 uses — the
**callback group** — with the group *structure* declared in code and the group→tier *policy* declared
in the workspace `system.toml`. A callback group is a first-class object created from the node
(rclcpp/rclrs shape); entities are created *in* a group; the workspace maps each group name to a tier;
the executor holds a config-seeded `group → sched_context` table and binds each callback to its
group's context at registration. No group names live in a package manifest — the group name is
declared in code (like a topic name) and referenced by `system.toml` (like a topic QoS override), so a
node package stays portable across workspaces.

**Delivery is in two phases.** phase-272 (landed) shipped the degenerate case — a `node_name →
sched_context` table looked up at the single `Executor::node_builder(name)` site, giving per-**node**
tier binding for C/C++ + rclcpp-shape (resolved #124). phase-273 generalizes it to per-**callback-
group** binding with the first-class group API + the `system.toml`-owned group→tier assignment, and
routes the Rust path through the same table.

## Motivation / problem

Two problems, one mechanism fixes both.

**1. Fragmented + coarse binding (phase-272 recap).** Tier binding was done four different ways
(Rust `run_tiers`/per-callback `bind_handle_to_sched_context`, C/C++ `NodeBuilder::sched`, rclcpp-shape
= nothing → #124). phase-272 unified the *node-level* case behind a `node_name → sched_context` table
resolved at `node_builder(name)` (every node funnels through it, RFC-0046). But that table is
**per-node** — one tier per node — while the Rust path is **per-callback**: a single node can put a
fast control loop and slow telemetry on different tiers. The per-node table can't express that. Unify
*up* to per-callback-group, not down to per-node.

**2. Non-portable coupling.** The group→tier binding lives in the **package manifest**
(`callback_groups = [{ id = "ctrl", tier = "high" }]`), but tier names (`[tiers.high]`) are defined in
the **workspace** `system.toml`. A reusable package (RFC-0026: packages are copy-out portable) thus
hardcodes a name that only exists in one workspace — move it elsewhere and the binding dangles. This
inverts ROS 2's own split.

**ROS 2 practice** (docs: Using Callback Groups; About Executors): a callback group is created **in
code** from the node (`create_callback_group(MutuallyExclusive | Reentrant)`); its type is about
*concurrency*, never priority; entities join a group via options (`options.callback_group = g`);
**where a group runs — thread/priority — is decided at composition/deployment** (`executor
.add_callback_group(g, node)` / a per-group executor thread), NOT in the package. Structure is code;
deployment is composition. nano-ros should match: **groups in code, tiers in `system.toml`.**

## Design

### The callback group (code, first-class — rclcpp/rclrs shape)
A group is created from the node and named; entities are created in a group; omitting the group uses
the node's default group.

```rust
// Rust (rclrs-shaped)
let ctrl = node.create_callback_group("ctrl");
node.create_timer_in(&ctrl, period, on_tick);
node.create_subscription_in(&ctrl, "/cmd", on_cmd);
```
```cpp
// C++ (rclcpp-shaped, ComponentNode)
auto ctrl = create_callback_group("ctrl");
create_timer(ctrl, 10ms, &Ctrl::on_tick);
```

The group **name** is the join key. It is declared in code — exactly like a topic name — and is the
only thing `system.toml` references. (The concurrency *type* — MutuallyExclusive/Reentrant — is a
follow-up; see Open Questions. Groups carry only a name + tier binding for now.)

### Group → tier lives in `system.toml` (deployment, by-name), never in a package manifest
The package declares **no** group list — the groups come from code, self-registering at runtime like
publishers/subscribers do. The workspace binds them, by name, the same way a launch QoS override
references a code-declared topic:

```toml
[[component]]
pkg = "ctrl_pkg"
name = "control_node"
group_tiers = { ctrl = "high", telem = "low" }   # a group with no entry → the default tier

[tiers.high]  spin_period_us = 1000   [tiers.high.posix]  priority = 80
[tiers.low]   spin_period_us = 10000  [tiers.low.posix]   priority = 10
```

A group named in code but absent from `group_tiers` runs on the default tier — harmless, exactly like
a topic with no QoS override. A typo'd name in `system.toml` simply never matches — same failure mode
(and same discipline) as topic-name overrides.

### The executor: a `group → sched_context` table, bound at registration
- The entry (deployment) resolves `system.toml`'s `group_tiers` + `[tiers.*]` to a
  `group_name → sched_context` mapping and **seeds it on the executor** before entities register —
  the per-group analog of the phase-272 node-name seed, same seed-by-name pattern as params/identity.
- Each entity registers **carrying its group name**; the executor sets that callback's
  `sched_context_binding` from the group table (falling back to the node default, then
  `SchedContextId(0)`). This extends `apply_node_default_sched` with a per-callback group override.
- **The node-name table (phase-272) is the degenerate case**: a node with no per-entity group is one
  implicit group = the node's default, so the phase-272 node-path keeps working unchanged; sub-node
  splitting is the new capability.

### Uniform across languages
Rust and C/C++ both: create named groups in code, create entities in a group, and let the executor
resolve group→sched_context from the config-seeded table. The Rust path drops its bespoke
per-callback `bind_handle_to_sched_context` loop in favor of the shared table (keeping `run_tiers`
for the *spin* structure); C/C++ gain the group-scoped create + carry the group across the register
FFI. One mechanism, ROS-shaped, portable.

### Precedence
Per callback: explicit group binding (from the seeded group table, via the entity's group) > the
node's default sched (phase-272 node-name table / `NodeBuilder::sched`) > `SchedContextId(0)`.

### Scope boundary — binding vs execution
Unchanged from phase-272: this RFC governs **binding** (which sched-context a callback uses). The
**execution** side — `run_tiers`, per-RTOS task priority (RFC-0016) — is orthogonal and stays.

> **Reconciliation with RFC-0015 Model 1 (2026-07).** RFC-0015 decided the single execution model for
> ALL languages is **Model 1** — one RTOS task per tier, `active_groups` gating (see its banner). Under
> Model 1 the **tiering is done by gating** (which tier task a callback registers on), NOT by
> `sched_context`. So this RFC's `group → sched_context` table is **re-scoped**: it is no longer the
> C/C++ *tiering* mechanism (phase-274 moves C/C++ onto per-tier executors + gating), but remains as
> (a) the single-`Executor`/no-tier fallback and (b) an optional **intra-tier** fine-scheduling knob
> (sporadic-server / per-callback priority *within* one tier task, RFC-0017). The **group API + config
> from this RFC (phase-273) are the durable surface** Model 1's gating consumes — code-declared groups
> + `system.toml group_tiers`. Net: the *binding* half of tiering is superseded by gating under Model
> 1; the group *declaration + config* half is the permanent user model.

## Alternatives considered

- **Per-node table only (phase-272 as the end state).** Rejected as the *final* design — it can't
  express sub-node tiering the Rust path already supports, and it keeps the manifest coupling. Kept as
  the shipped stepping stone + the degenerate default-group case.
- **Group→tier in the package manifest (status quo, with a per-node table).** Rejected — non-portable
  (a package hardcodes workspace tier names) and inverts ROS's structure-vs-deployment split.
- **Manifest lists group names (ids only) + `system.toml` binds tiers.** Better than the status quo,
  but still makes the author maintain a group-name list that duplicates what the code already
  declares — the same reason we don't list pub/sub names in a manifest. Rejected in favor of
  code-declared / config-referenced by name.
- **Abstract priority class in the package (`ctrl → realtime`), workspace maps class → tier.** Keeps
  portable author intent, but adds a second mapping layer. Deferred — revisit if authors want
  portable intent; by-name binding covers the need now.

## Open questions

1. **Concurrency type (follow-up).** rclcpp groups carry MutuallyExclusive/Reentrant. This RFC ships
   name + tier binding only; the concurrency type + its executor semantics (serialize-within-group)
   are a separate follow-up RFC/phase. The group object reserves room for it.
2. **`group_tiers` key shape.** By `(component, group)` under `[[component]]` (above) vs a flat
   `[group_tiers]` table keyed by `component.group`. Proposed: under `[[component]]` (co-located with
   the node's other deployment config); settle in phase-273.
3. **Default tier.** Unmapped group → the executor default sched-context (SC 0 / the node default).
   Proposed: yes.
4. **Namespaced identity.** Group binding keys on `(fully-qualified node name, group name)` to match
   the node-name table's namespaced key. Proposed: yes.

## Cross-references
- RFC-0015 (rtos-orchestration) + RFC-0016 (rtos-scheduling-features) — the tier/execution model.
- RFC-0046 (launch-authoritative node identity) — the single `node_builder(name)` funnel + the
  code-declares / config-references-by-name pattern.
- RFC-0026 (example-directory-layout) — packages are copy-out portable; the coupling this removes.
- RFC-0032 (entry-codegen-pipeline) — the tier resolver + entry seed site.
- #119 (C/C++ tiers) + #124 (rclcpp not sched-bound) — resolved by phase-272 (per-node); phase-273
  generalizes to per-group.
- ROS 2 docs — [Using Callback Groups](https://docs.ros.org/en/jazzy/How-To-Guides/Using-callback-groups.html),
  [About Executors](https://docs.ros.org/en/foxy/Concepts/About-Executors.html).

## Changelog
- 2026-07 (a) — created (Draft): node-name → sched-context table at `node_builder`, per-node binding.
  Delivered by phase-272 (#124 resolved).
- 2026-07 (b) — **revised to per-callback-group binding.** Groups are code-declared, first-class
  (rclcpp/rclrs shape); group→tier moves out of the package manifest into `system.toml` (by-name,
  like topic QoS overrides), removing the portability coupling; the executor gains a `group →
  sched_context` table bound at registration; the phase-272 node-name table becomes the degenerate
  default-group case. Concurrency type deferred (OQ1). Tracked by phase-273.
