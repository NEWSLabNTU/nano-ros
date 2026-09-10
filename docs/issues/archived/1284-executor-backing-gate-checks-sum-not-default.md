---
id: 1284
title: "`check-executor-backing-arena-pairing` checks that the numbers SUM, not that
  the stated backing MEETS the executor's default — three drifts in a week"
status: resolved
type: tech-debt
area: build, zephyr, ci
severity: medium
resolved_in: "fix(#1284): a stated executor backing is checked against the measured default"
related: [issue-1145, issue-1171, issue-1172]
---

## What happens

A Zephyr image that lowers `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` states the
executor backing it pays for: `CONFIG_NROS_EXECUTOR_BACKING_U64S`, with
`arena + 8 * backing == nros-arena-base` (issues 1145, 1171). Two checks guard it:

- `check-executor-backing-arena-pairing` (fast line): the three numbers sum.
- `executor::backing`'s const assertion: `EXECUTOR_BACKING_U64S >=
  ExecutorSizing::DEFAULT.u64_len()`. This is a COMPILE error, so it fires only
  when the image is built.

No merge-gating lane builds a Zephyr Rust image. So when the executor grows, the
twelve `examples/zephyr/rust/*/prj-{zenoh,cyclonedds}.conf` leaves stay exactly
paired, the fast-line gate stays green, and the images stop compiling on main.

## Measured drifts, one week

| when | default grew | leaves stated | caught by |
| --- | --- | --- | --- |
| phase-436 poll/wake revision | 11041 -> 11065 | 11041 | a W3.1 acceptance build, days later |
| #741 (issue 1172), `ActiveGroup` 40 -> 48 B | 11065 -> 11069 | 11045 (measured on a base that moved before merge) | the #741 follow-up, PR #898 |

Each value was measured with `ExecutorSizing::DEFAULT.u64_len()` under
`nros-node --features std,rmw-cffi`.

## Fix

Make the stated backing a checked claim against the MEASURED default, in a lane
that gates merges. One shape: a host test in `nros-node` that reads every
tracked conf stating `CONFIG_NROS_EXECUTOR_BACKING_U64S` and asserts it is
`>= ExecutorSizing::DEFAULT.u64_len()`, naming the file and both numbers on
failure. Then extend the pairing gate, or cross-reference it, so the rule has
one home.

The host figure is only valid for a conf whose image sets no executor-sizing
knob and whose target matches the host's pointer width. The twelve leaves build
for `native_sim/native/64` and set none. The check must REFUSE, not assume, for
a conf that breaks either condition: an unverifiable conf is a failure with a
reason, never a skip.

## Acceptance

- Mutation: state `11068` in one leaf. The new check goes red naming it, while
  the old pairing gate (re-paired arena) stays green.
- The check runs in a merge-gating lane. Show which one (`check-lane-contracts`
  / `check-default-gates-run-somewhere` agree).

## Resolution

A stated backing is now a CLAIM, checked against the measured default in
`just check node-std-tests`, which `gate.yml` runs on BOTH `pull_request` and
`merge_group`.

**The premise above was one conf short.** `examples/zephyr/rust/talker/prj-zenoh.conf`
is also built for `mps2_an385` (the `zephyr-cortex-m` fixture row), a 32-bit
board, so "the twelve leaves build for `native_sim/native/64`" held for eleven.
The host figure cannot vouch for that claim, and refusing it would have made the
check red by construction. So it is MEASURED instead, at its own width.

Three pieces, one home for the conf side:

- **`scripts/check-executor-backing-arena-pairing.py`** owns which confs claim
  what. Its new `--claims` mode lists every (conf, board) claim, attributing
  boards from the `examples/fixtures.toml` rows that build each conf, and
  REFUSES (fast line as well as `--claims`) any claim it cannot vouch for: no
  fixture row builds the conf; a board with no entry in `BOARD_TARGETS` (pointer
  width + measuring target); any fragment the image may merge (the row's own
  confs, the leaf's `boards/*.conf`, every shared `cmake/zephyr/*.conf` /
  `zephyr/*.conf`) sets an executor-sizing knob; the Zephyr platform descriptor
  sets an executor knob. The sizing-knob table is cross-checked against every
  `"NROS_*"` name `nros-node/build.rs` reads, in both directions, so a new knob
  cannot slip in unclassified.
- **Cross-width claims** (today: mps2 on `thumbv7m-none-eabi`): the
  `node-std-tests` recipe compiles `nros-node --lib` for the board's own target
  with `NROS_EXECUTOR_BACKING_U64S=<stated>` and `alloc,rmw-cffi`, so the crate's
  own const assertion rules at that pointer width. It first runs a NEGATIVE
  CONTROL (a 1-word backing must fail with the knob's message). Cost measured:
  11.3 s cold for the control, 2.0 s for the claim.
- **Host-width claims**: `packages/core/nros-node/tests/executor_backing_claims.rs`
  (`#[ignore]`, run by the recipe) compares each claim with
  `ExecutorSizing::DEFAULT.u64_len()` as compiled on the host, naming the conf,
  the board and both numbers. It refuses a cross-width claim the recipe did not
  list in `NROS_BACKING_CROSS_VERIFIED`, a sizing knob or resolution rung
  (`DOTCONFIG`, `NROS_PLATFORM_NAME`, `NROS_BOARD_TOML`, …) in its environment,
  and a `zephyr/Kconfig` sizing default that is neither a derive sentinel nor
  what the host build resolved.

The pairing gate's failure text, the const assertion's doc, the `Kconfig` help
and the test's doc each point to the others.

Measured on the base (`045c689f8`): `DEFAULT = 11069` words on the 64-bit host.
The lane went red there on exactly the twelve `native_sim/native/64` claims
(`11045 … 24 short`), while the mps2 claim passed its 32-bit compile. The
restatement to 11069 is PR #898's; this branch carries its commit unchanged so
the lane is green here, and it drops by patch-id if #898 lands first.

### Mutation

`examples/zephyr/rust/listener/prj-zenoh.conf` set to `11068`, arena re-paired
to `1048576 - 8 * 11068 = 960032`, everything else restated at 11069:

- `check-executor-backing-arena-pairing`: **OK** ("12 conf(s) pair the arena
  with a stated backing"), which is correct because the sum still holds.
- `just check node-std-tests`: **FAILED**:

      examples/zephyr/rust/listener/prj-zenoh.conf states
      CONFIG_NROS_EXECUTOR_BACKING_U64S=11068 for `native_sim/native/64`, but the
      executor's default measured on this 64-bit host is 11069 words (1 short).

The cross-width path has its own negative control on every run: a 1-word
backing on `thumbv7m-none-eabi` must fail with the knob's const-assertion
message, or the lane fails before it believes any pass.

### What is still assumed

- A Zephyr Rust image reaches `nros-node` with `alloc,rmw-cffi` (through `nros`)
  and with no `NROS_DECLARED_*` / `NROS_ENTITY_COUNT_*` from cmake, because
  `rust_cargo_application` builds its own environment (issue 0460). The check
  refuses those in its own environment, not in the image's. That the two agree
  is what the week's measurements show (11069 on the host, and the same number
  required by the images), not something the check proves.
- `BOARD_TARGETS` maps `mps2_an385` to `thumbv7m-none-eabi`, which is the
  board's Rust target, and treats any 64-bit host as `native_sim/native/64`'s
  width. A new board is refused until someone adds a row.
