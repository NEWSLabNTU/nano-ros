# Test Harness

This page covers nano-ros' test runner conventions, with a focus on
how the harness distinguishes **environment-conditional skips** from
**real failures** in JUnit reports.

For a recipe-level cheat-sheet (which `just <plat> test` exists,
which platform groups run under `just test-all`, where junit lives,
etc.) see `book/src/reference/build-commands.md`.

## Skip vs. failure tally semantics

### The skip contract

CLAUDE.md → "Practices" mandates:

> Tests must fail on unmet preconditions. `assert!()` / `bail!()` for
> missing env/binary. `nros_tests::skip!` panics with `[SKIPPED]` (OK).
> Bare `eprintln!` + `return` reports PASS — never. Same rule at
> runtime: panic, not silent early-return.

A test that quietly returns when its preconditions are not met
**reports PASS** — masking every CI regression in that area. The
project's response is to **panic with a `[SKIPPED]` marker** for
environment-conditional skips: missing zenohd binary, missing QEMU,
missing cross toolchain, missing fixture. The `nros_tests::skip!`
macro (defined in `packages/testing/nros-tests/src/lib.rs`) expands
to:

```rust
panic!("[SKIPPED] {}", format_args!($($arg)*))
```

The literal `[SKIPPED] ` prefix is **load-bearing**: every downstream
consumer (the post-processor, the failure counters, the rerun-failed
filterset, CI dashboards) keys off it. Do not change the prefix or
re-format the message without sweeping every consumer.

### Why the panic surfaces as `<failure>`, not `<skipped>`

Nextest exposes three test outcomes in JUnit XML:

| nextest concept | JUnit element | Triggered by |
| --- | --- | --- |
| pass | `<testcase>` w/ no children | normal return |
| skip | `<skipped>` | `#[ignore]` attribute |
| fail | `<failure>` | panic, non-zero exit, timeout |

The `<skipped>` channel is reserved for the `#[ignore]` attribute,
which is a **compile-time** decision (the test never runs). nextest
has no built-in runtime-skip — and `[SKIPPED]` is a runtime decision,
so the panic lands in `<failure>` like any other panic.

### The JUnit post-processor (Phase 214.R.1)

`scripts/test/rewrite-skipped-junit.py` rewrites the junit.xml after
each test recipe to convert `[SKIPPED]` failures into native
`<skipped>` entries. For each `<testcase>` whose every `<failure>`
contains the `[SKIPPED]` marker (message attribute or panic body),
the script:

1. Replaces each `<failure message="[SKIPPED] …">` with
   `<skipped message="[SKIPPED] …">`, preserving the message
   attribute and panic body verbatim.
2. Decrements `failures="N"` and increments `skipped="N"` on the
   enclosing `<testsuite>` and the top-level `<testsuites>`
   element.
3. Atomically writes the file back (`tmp + os.replace`) so a crash
   mid-write leaves the original intact.

A testcase with **mixed** failures (one real + one `[SKIPPED]`) stays
a `<failure>` — only purely-`[SKIPPED]` cases are rewritten.

The script is idempotent and exits 0 on missing or unparseable
inputs, so it can be safely chained at the tail of any test recipe.

### Recipe hook points

The post-processor runs at every test recipe's tail, **before** the
`_count-real-failures` / `_test-summary` chain reads the file:

* `justfile::test` — workspace fast tier
* `justfile::test-all` — full matrix
* `justfile::test-failed` — rerun-failures workflow
* `justfile::_nextest-platform` — shared helper for the per-platform
  recipes (`just esp32 test`, `just freertos test`,
  `just nuttx test`, `just orin-spe test`,
  `just threadx-linux test`, `just threadx-riscv64 test`)
* `just/xrce.just::test` / `test-ros2` / `test-c`

Other plat-specific recipes (`just zephyr test*`, `just native
test*`, `just qemu-baremetal test*`, `just px4 test*`,
`just cyclonedds test*`) currently invoke nextest directly without a
skip-aware tail. When you add or refactor one, mirror the
`_nextest-platform` pattern:

```bash
set +e
cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
rc=$?
set -e
just _rewrite-skipped-junit || true
[ $rc -eq 0 ] && exit 0
real="$(just _count-real-failures)"
just _test-summary || true
if [ "$real" -ne 0 ]; then
    echo "ERROR: $real real (non-[SKIPPED]) test failure(s)."
    exit 1
fi
echo "All failures were [SKIPPED] preconditions — treating as pass."
```

### Tally consumers

| Consumer | What it reads | Effect of the rewrite |
| --- | --- | --- |
| `justfile::_count-real-failures` | `<failure>` count | sees 0 after rewrite |
| `justfile::_test-summary` | `<failure>` lines + `[SKIPPED]` markers | groups skips into a `Environment-skipped tests: N` line |
| `scripts/test/failed-filterset.py` | testcases with non-`[SKIPPED]` `<failure>` | unchanged (already filters by marker) |
| CI dashboards (junit-cli-report-viewer, GitHub Actions test report, …) | `<failure>` + `<skipped>` counts | sees skips as skips, not regressions |

### What a `[SKIPPED]` test is NOT

A `[SKIPPED]` testcase is **not a regression** and **not a bug** —
it is a precondition that the local environment does not satisfy.
Common reasons:

* `zenohd` binary missing under `build/zenohd/` (run
  `just zenohd setup`).
* `qemu-system-arm` / `qemu-system-riscv64` not on PATH
  (run `just qemu setup-qemu` or install via the distro package).
* Cross toolchain absent (e.g. `arm-none-eabi-gcc`,
  `riscv64-unknown-elf-gcc`) — the Phase 185.2 / 186.4 embedded
  Cyclone tests gate on these.
* `xrce-agent` not built (run `just xrce build-agent`).
* `ros2` not sourced / no `rmw_fastrtps` runtime
  (run `source /opt/ros/humble/setup.bash`).

A `Environment-skipped tests:` line in the test summary is
**informational** — it tells you which prerequisites are unmet on
the current machine, not which code paths have regressed. The
**Real failures: X / Y** line is the source of truth for "did
anything break?".

### Forbidden patterns

Per CLAUDE.md, never:

* **Bare `eprintln!` + `return`** in place of `skip!` — the test
  reports PASS, hiding the precondition gap.
* **Silent early return** in runtime code — panic, don't swallow.
* **Change the `[SKIPPED]` prefix** without sweeping every consumer
  listed above and CLAUDE.md.

The only blessed exception to the "panic, don't return" rule is
`rstest` `#[values]` matrix rows — the macro framework does not
support per-row skip via panic, and `skip_reason()` is the
documented escape hatch. Use it sparingly.

## See also

* CLAUDE.md → "Practices" — the parent contract.
* `packages/testing/nros-tests/src/lib.rs:51` — `skip!` macro.
* `scripts/test/rewrite-skipped-junit.py` — the post-processor.
* `scripts/test/failed-filterset.py` — the rerun-failures filterset.
* `book/src/reference/build-commands.md` — recipe cheat-sheet.
* `docs/roadmap/phase-214-antipattern-audit-findings.md` Track R —
  the phase that introduced this doc.
