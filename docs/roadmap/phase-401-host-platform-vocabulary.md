# phase-401 — `native` / `posix` / `linux`: three words, two questions

**Status (2026-08-30). W1, W2 and W3 are landed — the rule, the host board, the
gate, a 71-fix tree-wide audit across six scopes, and the renames it identified.
W4 is proposed and unstarted, and is a product change nobody has asked for.** Implements the CLAUDE.md "Naming" rule; no RFC — this is a
vocabulary correction, not a design change.

## The rule

| word | question it answers | meaning |
| --- | --- | --- |
| `native` | **role** | this is the HOST build, not a cross build |
| `posix` | **reach** | works on any POSIX-compliant system |
| `linux` | **reach** | works only on Linux |

`native` sits beside either reach. The two reaches exclude each other.

## What was wrong

The three were used interchangeably. The host board descriptor read

```toml
names = ["linux", "native", "posix"]
```

— all three of one board, so every later reader could pick whichever word they
had in mind, and one of them was false.

**Which one is false was MEASURED, and the first answer was wrong.** The
platform crate looked like the place to check, and it is clean:
`nros-platform-posix` uses `sched_yield`, `sched_get_priority_{min,max}`,
`pthread_setschedparam` and `SCHED_FIFO`; its one `__linux__` selects
`MSG_NOSIGNAL` behind a portable `#else`; `pthread_setname_np` is gated on
`_GNU_SOURCE` rather than the OS. So `posix` looked right.

The BOARD crate is not clean. `nros-board-linux::apply_tier_affinity` — the
placement dim's realization — calls `sched_setaffinity` with `cpu_set_t` and
`CPU_SET`, **ungated by `cfg(target_os)`**, and libc 0.2.189 defines those for
linux, android, freebsd, dragonfly, fuchsia and cygwin and **not for apple**.
The crate cannot build on macOS. Its own doc-comment called that call "the
placement dim's POSIX `Native` realization" — the collapse, in one line.

So `posix` was the false claim and `linux` the true one, and the two layers
legitimately carry different words: the platform names software-stack facts,
the board names what we support.

Strictly the reach is "Linux and some BSDs, not macOS". `linux` is the closest
of the three available words.

**The tree already said so, in the one place nobody cross-checked.** AGENTS.md
has carried this since 2026-06-18 (phase-260):

> **Supported hosts: Linux (primary) and \*BSD (POSIX path). macOS is NOT
> supported** … no macOS CI runner means macOS-specific link/section paths ship
> un-run, so the project does not carry them.

That is exactly the reach libc's coverage implies — linux plus freebsd and
dragonfly, not apple — arrived at from the opposite direction, and it is what
the board's `posix` claim contradicted. The collapse was not a gap in what the
project knew; it was two statements of the same fact that nothing compared.

Making `posix` true would mean cfg-gating the affinity call to a loud no-op off
Linux — a product change, listed as W4.

### A refinement W2 turned up: POSIX-conformant is not macOS-portable

The audit found `docs/reference/platform-sync-abi.md` giving macOS a green
compile-check tick, and disproved it: `nros-platform-posix/src/timer.c` calls
`timer_create(CLOCK_MONOTONIC, SIGEV_THREAD)` with **no `__APPLE__` fallback**,
while its sibling `platform.c` carries five. macOS does not implement POSIX
timers, so the crate does not build there either.

This does NOT contradict "the platform crate is POSIX-clean" — `timer_create`
IS POSIX (the Timers option). It sharpens what the word buys: **`posix` is a
claim about the STANDARD, not a promise of portability to every system that
calls itself POSIX.** macOS is an incomplete POSIX implementation, so the
crate's name stays right and the host reach stays `linux` — for a second,
independent reason, one layer below the affinity call.

## Waves

- [x] **W1 — the rule, the host board, the gate.**
      CLAUDE.md "Naming" states it. The descriptor is `["native", "linux"]`
      with `platform = "posix"`; `package.xml` matches (caught by
      `check-provider-announcements`, which is what it is for). The single
      `deploy = "posix"` consumer — a board-AGNOSTIC run-plan fixture — moved to
      `native`, the role word it was actually contrasting against its
      `freertos_entry` sibling. `POSIX_CORE_PIN{,_FALLBACK}_MARKER` renamed to
      `LINUX_…` (values unchanged, so no fixture output moved).
      Gate: `check-host-platform-vocabulary` — a board may not claim both
      reaches. Its first version also had `native` conflicting with `linux`,
      which was the same mistake one level up: a board can be the host build AND
      support only Linux, which is exactly what this one is.

- [x] **W2 — the tree-wide audit. 71 fixes across six disjoint scopes.**
      Comments, doc-comments, prose and printed strings only; no identifier,
      file, crate, feature or token renamed. Gates green
      (`check-book-links`, `check-doc-refs`, `check-abi-bindings`,
      `check-sched-matrix`, `check-fixture-id-guard`, `check-deploy-board-resolves`,
      `check-host-platform-vocabulary`), `just ci-l1` green.

      **The three shapes it found**, none of which a grep could have separated:

      1. *Right conclusion, wrong reason.* `nsos_netx.h` said "Linux only (uses
         POSIX `<sys/socket.h>`)". Linux-only is correct — but because
         `translate_sockopt` hardcodes Linux's numeric `IP_MULTICAST_*` values
         (32–36) where macOS/BSD use 9–13. A correct verdict resting on the
         wrong evidence teaches the wrong test.
      2. *A comment promising reach the code never had.*
         `nros-zpico-build/runner.rs` said "POSIX hosts (Linux / macOS / \*BSD)"
         while its `auto_posix` is
         `matches!(target_os, "linux"|"freebsd"|"netbsd"|"openbsd"|"android")` —
         macOS absent from the match all along. Same shape in
         `cyclonedds-sys/build.rs`, which links `-lrt` unconditionally on
         "hosted POSIX"; macOS ships no `librt`.
      3. *The collapse written as one word.* `native/POSIX` with a slash, in six
         files, where the distinction actually being drawn was role.

      **A claim that had gone false.** `nros-board-linux` printed
      `posix tier \`core\` is advisory (not applied natively)` while
      `apply_tier_affinity` was called for every tier and did pin. The comment
      directly above it documents fixing exactly that staleness for the
      *priority* half — "a reader would believe a declaration was inert while it
      was being honoured" — and left the `core` half. Removed rather than
      reworded: both dims already report their own per-tier outcome, which
      cannot rot the way a blanket up-front note can.

      **A generated row claiming both reaches.** `gen-sched-matrix.py` rendered
      `Linux / POSIX host` — the `["linux","native","posix"]` shape the gate
      forbids, one layer over, with an `affinity` cell delivered by
      `sched_setaffinity`. Now `Linux host`; generator and page moved together.

      **Names checked and DEFENDED, not renamed:** `nros-platform-posix`,
      `nros-board-freertos-posix` (its upstream FreeRTOS port carries real
      `__APPLE__` arms — verified in the vendored source), `transport_posix_*`,
      `zpico_posix_*`, `LinkPolicy::posix()`, and every `platform-posix` feature.
      Refusing a rename on evidence is as much the point as making one.

      **One over-correction, caught and reverted.** The docs scope first deleted
      `*BSD` as a false claim, then read AGENTS.md — "Supported hosts: Linux
      (primary) and \*BSD (POSIX path)" — and restored all four sites. `*BSD`
      was never in the false class; macOS and "any POSIX" were.

      **Not renamed by design** — each is a separate reviewable change: files,
      directories, crate names, cargo features, cmake variables, just recipes,
      board tokens, test names and fixture ids. Several are matched elsewhere,
      and this repository has a documented history of a rename silently
      skipping a test. W3 carries what W2 identified.

- [x] **W3 — the renames W2 identified.** Three candidates; two renamed, one
      checked and kept.

      * **`book/src/platform-guides/native-posix.md` → `native-host.md`** — the
        two collapsed words in one filename. Page and its 3 inbound links
        (`SUMMARY.md`, `deployment.md`, `workflow-by-platform.md`) moved in one
        commit; `check-book-links` green.
      * **`plat_str` in `sched_dims_applied_e2e.rs`** — a local match that was a
        second spelling of `PlatformId::fixture_tokens` and DISAGREED with it
        (`MP::Linux` printed `posix`; the SSoT says `linux`). Now delegates.
        Every other variant was already identical, so the only behavioural
        change is the one wrong label — and a new `PlatformId` variant is now a
        compile error here instead of a silent `"?"`.
      * **`tests/fixtures/board-workspace/…/boards/posix/nros-board.toml`** —
        `names = ["native", "posix"]`, which looks like the pre-rename spelling
        and is not. That fixture board is CRATE-LESS: no board crate, so no
        `sched_setaffinity`, so `posix` is a true reach claim for it, and it is
        now the only place still exercising the `posix` board-name arm. Kept,
        with the reason written into the file so the next reader does not
        "fix" it.

      Also fixed under W3, from #0916: the `native`/`posix` scaffold aliases
      produced different trees because two sites asked `platform != "native"` —
      a literal comparison against one of two spellings of one platform. Now
      `needs_scaffolded_nros_toml()` asks the KIND, with a test that the two
      spellings agree.

- [ ] **W4 — make `posix` true, if anyone needs it.** `cfg(target_os)`-gate
      `apply_tier_affinity` to a loud no-op off Linux, the way it already
      handles a rejected pin. Then the host board's reach becomes `posix` and
      the crate builds on macOS. Nobody has asked for this; it is recorded so
      the option is not lost.

## What is deliberately NOT touched

* **`native_sim`, `native_posix`, `native-posix`** — Zephyr's own board names,
  not ours to rename, and by far the most common raw hit.
* **`CONFIG_POSIX_*`, `_POSIX_*`, `POSIX_API`, `_GNU_SOURCE`** — Kconfig and
  feature macros.
* **libc/POSIX function names** (`posix_memalign`, `posix_spawn`, …).
* **`board = "native"` / `deploy = "native"` / `[image.native]`** — the accepted
  spelling in 33 manifests. `native` is the role there and is correct.
* **Archived issues and roadmap docs** — a historical record; correcting history
  is not an improvement.
* **Prose, by the gate.** A gate cannot tell a correct "on Linux" in a sentence
  from a careless one, and one that guessed would train people to phrase around
  it rather than to mean it. The `names` list is where the collapse is
  mechanical, so that is where the gate is; prose is W2's human pass.

## Evidence

```console
$ grep -rl 'pub fn sched_setaffinity' ~/.cargo/registry/src/*/libc-0.2.189/src
  fuchsia/  teeos/  unix/bsd/freebsdlike/dragonfly/  unix/bsd/freebsdlike/freebsd/
  unix/cygwin/  unix/linux_like/android/  unix/linux_like/linux/
# apple: absent
```

Counts across both keys, re-measured (the first count used `board =` alone and
missed `deploy =`): `native` 33, `posix` 1, `linux` 0.
