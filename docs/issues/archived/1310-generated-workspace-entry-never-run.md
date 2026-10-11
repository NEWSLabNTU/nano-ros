---
id: 1310
title: "A generated workspace entry is BUILT and symbol-checked but never RUN — the RMW axis has no runtime coverage except zenoh"
status: resolved
type: tech-debt
area: [testing, ci, rmw]
related: [1295, 0831, 1058, 1819, 1732, rfc-0065, phase-445]
resolved: 2026-10-12
---

# What is missing

`examples/fixtures.toml` carries three generated Rust workspace entry rows over
one workspace — `workspace-rust-native` (zenoh), `workspace-rust-native-cyclonedds`
and `workspace-rust-native-xrce`. The fixture lane BUILDS all three. Exactly one
of them is ever EXECUTED by a test:

| what looks at a generated entry | what it actually does |
| --- | --- |
| `rmw_coordinate_truth` | `nm` — proves the declared backend was LINKED. Never runs the binary. |
| `deployed_native_system_e2e`, `rust_multi_node_per_node_graph` | RUN `native_entry` — **zenoh only** |
| the RMW × language matrix cells | run the HAND-WRITTEN per-example binaries (`build_native_rust_example_rmw`), not a generated entry |
| `scaffold-journey`, `acceptance` | scaffold SINGLE-PACKAGE leaves (`--platform baremetal` / `--platform native --use-case talker`); neither builds a `--workspace --lang rust` scaffold |

So for cyclonedds and xrce the chain stops at "it linked".

# Why it matters: this already shipped a bug, twice, on the same two rows

- **Issue 0831**: those two rows built ZENOH while their coordinate said
  cyclonedds/xrce. Fixed, and `rmw_coordinate_truth` was written so it could not
  recur.
- **Issue 1295**: the same two rows, now linking the right backend — this gate
  green, 350+ `dds_` symbols present — and the Cyclone entry could not create a
  publisher at all. The `nros` umbrella never received the `rmw-cyclonedds`
  MARKER (`needs-type-descriptors`), so the generic path registered no type
  descriptor.

0831's prescription was *"add a runtime assertion rather than trusting the
coordinate — the artifact knows"*. It was implemented as **inspect the
artifact**, and an inspection cannot see 1295: symbols prove linking, never
behaviour. The book's own Rust quick start (`nros new --workspace --lang rust`,
whose scaffold declares `rmw = "cyclonedds"`) was broken end to end and no lane
said so.

This is issue 1058's shape one level up — there, scaffold output was grepped and
never built; here, an entry is built and never run.

# Fix

A second test beside the symbol one, in `rmw_coordinate_truth.rs`, that RUNS
each `[[workspace_fixture]]` row's binary:

- reaching `NodeRegister` (the 1295 signature) **FAILS** — a defect in the image
  whatever else is on the bus;
- never reaching node registration (no router, no XRCE Agent) is reported as a
  **precondition, per row**, so a skip cannot read as coverage;
- the bus is pinned to loopback through `dds_isolation::apply_to_command`
  (issues 1009/1137) and each row gets its own domain, so one row cannot read a
  neighbour's traffic — or the LAN's — as its own.

The split is deliberate: `nm` answers "was it linked" for every row cheaply; the
run answers "does it work" for the rows this host can start.

# A caveat the negative control found

Proving the new test catches 1295 needed the facade REGENERATED, and reverting
`facade.rs` + `just setup-cli` + the fixture lane was not enough: the lane
rebuilt the entry from the *existing* generated manifest, which still carried
the marker, so the test passed and proved nothing. Deleting
`generated/nros-selection/<entry>/` first is what forced the writer to run.

So a generated facade can outlive a change to the CLI that generates it — the
issue-1018 class (an emitter whose output is not keyed on its tool). It did not
cost a wrong verdict here because the control was checked against its own
premise (the regenerated manifest was read back, and the first run's marker gave
it away), but anyone testing a facade change by hand will meet it.

# Not covered by this fix

- **A `--workspace --lang rust` scaffold is still never built in CI.** Both
  scaffold journeys do single packages. 1295's first repro was the scaffold, and
  it would still ship.
- The run asserts node REGISTRATION, not delivery. Delivery needs a peer per
  RMW; that is the matrix's job and it runs different binaries.

# Resolution (2026-10-12)

The runtime test above exists (`a_rows_entry_registers_its_nodes_at_runtime`)
and was RUN for the first time over every Linux Rust row:
`scripts/build/workspace-fixtures-build.sh linux rust` built all 24.

| peers running | rows that registered |
| --- | --- |
| none | 1 — the cyclonedds row (DDS needs no peer), i.e. 1295's row, now green at runtime |
| `rmw_zenohd` + `MicroXRCEAgent` on loopback | 22 |
| plus `NROS_LOCATOR` on the two xrce rows | 24 (the xrce pair checked by hand) |

Running it found two entry kinds that never exit, so a bounded observer could
only report them as HUNG:

- **Tiered Rust entries ignored `NROS_ENTRY_SPIN_MS`.** `nros-board-linux`'s
  `spin_forever` looped until a signal; it now also stops at the deadline the
  knob sets (`entry_spin_deadline`), as the single-executor road and the C/C++
  tiered runner already did.
- **Bridge entries were a bare `loop {}`** that ignored both the deadline and
  SIGTERM — issue 1732's termination guard never reached the bridge, so a
  signal killed it with its sessions open. `nros::main!`'s bridge arm now
  installs the guard and calls `nros_rmw_bridge::run_from_config_str_until`,
  which stops on either and closes the executor.

What this does not cover moves to **issue 1819**: the test starts no peers of
its own and passes no locator, so on a host without them 23 of 24 rows are a
stderr note rather than a red. The scaffold-build gap above is unchanged.
