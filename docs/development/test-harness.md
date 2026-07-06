# Test Harness

This page covers nano-ros' test runner conventions, with a focus on
how the harness distinguishes **environment-conditional skips** from
**real failures** in JUnit reports.

For a recipe-level cheat-sheet (which `just <plat> test` exists,
which platform groups run under `just test-all`, where junit lives,
etc.) see `book/src/reference/build-commands.md`.

## Build-fixtures ordering

Per-platform `test` always sequences `build-fixtures` first; the
harness fails loud on missing fixtures rather than rebuilding. Test
recipes assume fixtures are present + at the canonical paths from
`examples/fixtures.toml` — when missing, the harness asserts with
the source location and the expected fixture path, instead of
silently re-running cargo-build.

Wired in Phase 214.Q.1 (`d7e895228`) by adding `build-fixtures` as a
just-recipe prereq head form (`test verbose="": build-fixtures`) on
every per-platform module — native, zephyr, esp32, freertos, nuttx,
threadx-linux, threadx-riscv64, qemu-baremetal. Cyclonedds uses
CTest with `test: build-rmw` as its equivalent. The fail-loud
assertion lives in `packages/testing/nros-tests/src/fixtures/binaries/mod.rs::require_prebuilt_binary`
— it returns `TestError::BuildFailed("Test fixture binary not
prebuilt: <path>. Run `just build-test-fixtures` first.")` instead
of invoking cargo. Builds inside test bodies historically stretched
a 14 s test to 125 s on a saturated host racing against QEMU +
zenohd; the prereq-then-assert shape keeps build and run phases
cooperatively sequenced.

**Staleness (phase-278, issue #147).** Existence alone let a bare
`cargo nextest run` (which skips the `just test-all`
`_check-fixtures-stale` preflight) silently run a fixture whose source
had changed since it was built — the recurring hazard behind #146
(a pre-W4 `Int32_` listener vs the `String_` test), #129, #140. The
resolvers now also carry a DETECT-ONLY staleness probe that reads the
toolchain's own recorded dependency graph and mtime-compares against
the built binary — never invoking the compiler, so it cannot rebuild:
`require_prebuilt_binary_fresh` (cargo `<binary>.d` dep-info, native
rust + `bins/`), `require_prebuilt_binary_fresh_cmake` (`ninja -t deps`
on the C/C++ cell), and `require_prebuilt_binary_fresh_zephyr` (the
west staticlib's `.d` vs the linked `zephyr.exe`). A stale fixture
hard-fails `"… is STALE"` naming the newer source; the fix is
`just build-test-fixtures`, not `NROS_SKIP_FIXTURE_CHECK=1` (that
bypass exists for "built it another way"). A missing `.d`/`.ninja_deps`
falls back to existence-only, so non-cargo/non-ninja fixtures
(qemu/west-image/idf) are unaffected.

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
* `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` —
  `require_prebuilt_binary` (Build-fixtures ordering contract).
* `scripts/test/rewrite-skipped-junit.py` — the post-processor.
* `scripts/test/failed-filterset.py` — the rerun-failures filterset.
* `book/src/reference/build-commands.md` — recipe cheat-sheet.
* `docs/roadmap/phase-214-antipattern-audit-findings.md` Track R —
  the phase that introduced this doc.
