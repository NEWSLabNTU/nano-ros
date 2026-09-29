# Phase 472 — gate reach sweep

**Status (2026-09-29). AUDIT LANDED; W1, W5, W7, W8 open.** (W2, W3, W4, W6, W9 done — see each.) An audit of every tracked
`scripts/check-*` gate against one question, the codebase-audit checklist's **I6
second-order** rule: *a gate must be able to fail on the case it names.*

**155 of 320 gates failed it** — each by a demonstrated mutation, not by
argument. That is roughly one hole per two gates. The holes are not scattered:
they fall into nine classes, and every class has a fix that is ONE shared helper
rather than 155 local edits. This phase is that sweep.

**Prior:** checklist I6 (a 2026-07-28 audit found four gates narrower than their
rule — #321, #328, #332, #334); issue 0196 (a gate's reach must equal the rule it
enforces); CLAUDE.md "Fix the CLASS, not the reported site". Issues 1362, 1379,
1388, 1401 and 1431 were five instances of this class found one at a time in the
fortnight before the audit; this phase exists because meeting it one gate at a
time was not keeping up.

**Source:** six parallel auditors on `origin/main` at `5d9fc529ec`, one bucket of
51–55 gates each, each in its own worktree so their mutations could not corrupt
one another. Fourteen findings were independently re-verified by the
coordinator; all fourteen held.

---

## Method, and the standard that made it worth trusting

For each gate: read the **rule** it states (docstring, header comment, and the
failure message it prints), then read the code that decides its **population**
(what it actually examines), and compare.

A finding is **CONFIRMED** only if the auditor constructed the case the gate
claims to catch, ran the gate, and it **passed (rc=0) when it should have
failed** — then restored the tree. Almost every confirmed finding also carries a
**positive control**: the same defect placed where the gate DOES read makes it
fail, which proves the gate was invoked correctly and the mutation was not
vacuous.

Anything believed but not demonstrated is recorded as SUSPECTED and is not
counted. Vacuous controls were the main hazard: a mutation whose anchor silently
did not match prints the same "OK" as a real hole, and this happened to the
coordinator twice and to two earlier agents during the fortnight that prompted
the audit. Every mutation was checked with `git diff` before the gate ran.

## Results

| bucket | gates | confirmed | suspected | could not run |
| --- | ---: | ---: | ---: | ---: |
| 00 `action-client-arena-budget` .. `cmake-verb-reachable` | 55 | 18 | 4 | 7 |
| 01 `codegen-tool-reconfigure` .. `feature-set-ssot` | 52 | 30 | 7 | 2 |
| 02 `ffi-struct-mirrors` .. `literal-domain-id` | 54 | 34 | 11 | 5 |
| 03 `manifests-parse` .. `qos-mask-derivation` | 53 | 28 | 5 | 7 |
| 04 `qos-profile-ssot` .. `template-copy-out` | 55 | 23 | 3 | 7 |
| 05 `test-domain-assignment` .. `zephyr-workspace-resolvers` | 51 | 18 | 6 | 7 |
| **total** | **320** | **155** | **36** | **35** |

By severity: **2 P1, 94 P2, 59 P3**, plus one dead gate
(`check-board-manifest-drift.sh`, which can only ever print "nothing to audit").

Of the 80 gates with no selftest, the ones in each bucket with a confirmed hole
are named per class below; in nearly every case a one-line negative control would
have caught it.

---

## The nine classes

Each work item names its member gates and ONE fix. Fixing members one at a time
is the failure mode this phase exists to end.

### W1 — composite actions are unread

Gates that read `.github/workflows/*.yml` and never `.github/actions/*/action.yml`.
Composite actions are real CI steps: `nightly.yml` calls `setup-nros-cli` "the SOLE"
CLI acquisition path for its jobs.

Members: `check-ci-cli-from-source` (P2), `check-workflow-repo-env` (P2),
`check-workflow-indexed-apt` (P2), `check-ci-no-fixture-tolerance`,
`check-ci-no-verb-fallback`, `check-workflow-setup-spelling`,
`check-git-dir-layout-assumptions`, `check-grep-q-error-conflation`,
`check-pipefail-sigpipe-assertions`.

**Live:** `.github/actions/setup-qemu-patched` runs `just` without sourcing
`activate.sh` and apt-installs three packages the index already declares (issue
1548). `check-workflow-just-provisioning` already reads composite actions — it is
the model.

**Fix:** make the shared loader `scripts/lib/workflow_commands.load_workflows`
include composite actions, and move every member onto it.

### W2 — just-file populations miss `just/check/` and the root `justfile`

Gates that glob `just/*.just` flat, or omit the extensionless root `justfile`.
`just/check/*.just` holds the 13 files of `mod check`.

Members: `check-just-recipe-refs` (P2 — also passes any `just <module> <bogus>`),
`check-just-recipe-paths` (P2), `check-named-lane-fails` (P2),
`check-skippable-tests-tolerant` (P2, **live**: `just/zephyr-setup.just:401` runs a
bare `cargo nextest` over 5 `skip!` calls), `check-nros-c-feature-agreement`,
`check-zephyr-module-binding` (misses the root `justfile`, workflows and Python —
and `colcon_nano_ros/task/nros/build.py` runs a configuring `west build` without the
module flag), `check-sysdep-remedies`, `check-sdk-guard-can-fire`,
`check-prose-issue-refs`, plus the siblings `check-lane-skip-protocol`,
`check-fixture-artifact-dir-inputs`, `check-build-profile-literals`,
`check-example-leaf-target-dirs` that share the same glob.

**Fix:** one population function — `scripts/lib` already has `check_just_sources()`
— and every member uses it.

**Status: DONE.** `scripts/lib/check_just_sources.py` now walks the justfile
GRAPH: `just_sources(root)` (every file `just` loads — root `justfile`, each
`mod`, each `import`, recursively, each once) and `just_modules(root)` (`{mod
name: [files]}`, keyed by the `mod` NAME, imports merged into their importer);
`check_just_sources()` is the `check` entry of it. A non-optional `mod`/`import`
whose file is missing raises — a broken graph is never a smaller population. A
CLI (`--list [--root DIR]`) serves shell gates. Its self-test runs on every use.
36 files today (the flat glob saw 23 or 24).

| member | mutation (confirmed applied by `git diff`) | before | after |
| --- | --- | ---: | ---: |
| `just-recipe-refs` (population) | a body calling an undefined root recipe, in `just/check/docs.just` | 0 | 1 |
| `just-recipe-refs` (module) | a body calling an undefined recipe of the `native` module, in `just/native.just` | 0 | 1 |
| `just-recipe-paths` | `bash scripts/no-such.sh` in `just/check/docs.just` | 0 | 1 |
| `named-lane-fails` | unsourced `nros_lane_skip` in `just/check/docs.just` | 0 | 1 |
| `skippable-tests-tolerant` | bare `cargo nextest … --test fvp_runtime_ws` in `just/check/lanes.just` (and the LIVE site) | 0 | 1 |
| `nros-c-feature-agreement` | `--features cffi-zenoh-cffi` in the root `justfile` | 0 | 1 |
| `zephyr-module-binding` | bare `west build -b native_sim <app>` in the root `justfile` (and the LIVE colcon site) | 0 | 1 |
| `sysdep-remedies` | `sudo apt install` in `just/check/tools.just` | 0 | 1 |
| `sdk-guard-can-fire` | `-z "${NUTTX_DIR:-}"` in `just/check/platform.just`; in the root `justfile`; unbraced `-z "$NUTTX_DIR"` in `just/nuttx.just` | 0/0/0 | 1/1/1 |
| `lane-skip-protocol` | `echo "skip: …"; exit 0` in `just/check/lanes.just` | 0 | 1 |
| `fixture-artifact-dir-inputs` | a `"" ""` packer in the root `justfile`; in `just/check/fixtures.just` | 0/0 | 1/1 |
| `provisioned-root-guard-reach` (sweep) | direct `check-zephyr-workspace-checkout.sh` call in `just/check/platform.just` | 0 | 1 |
| `lane-contracts` (sweep) | modules keyed by FILENAME (`threadx-linux::`, `qemu-baremetal::`, `zephyr-ci::`): the new selftest rows fail against the old keying | — | 1 |

Each member gained a normal-path negative control that builds a temp tree with a
`mod check` + `import` (or a `mod` file, or the root `justfile`) and asserts the
planted defect is read; `sysdep-remedies` had no self-test and left the
gate-selftests baseline (90 → 89). Positive control: every member green on the
tree. `lane-contracts` now resolves 24 CI lane invocations, not 21.

Already covered, verified by the same mutation (old gate rc=1):
`prose-issue-refs` (fixed by issue 1545), `build-profile-literals` (git pathspec
`just/*.just` matches `just/check/…`), `example-leaf-target-dirs` (index-based,
recursive). The sweep also found `preconditions-provisioned` listing `just/*.just`
into an unused argument (now the graph), and these already read the tree through
the git index recursively: `no-allow-multiple-def`, `ps-zombie-blind`,
`third-party-is-submodules`, `workflow-indexed-apt`, `lane-skip-interpreters`,
`xrce-one-vendored-compile`, `one-producer-per-tool`, `grep-q-error-conflation`,
`interop-cell-runners`.

Beyond population, fixed where the widened read made it cheap: `just-recipe-refs`
let a module followed by ANY second word pass; `named-lane-fails`' enumerated
`nros_lane_*` list had drifted (now harvested from `lane-skip.sh`);
`lane-skip-protocol` missed `|| { echo "…skip…"; exit 0; }`; `sdk-guard-can-fire`
read the braced spelling only; `skippable-tests-tolerant` printed the LAST file's
`--test` count ("0 scanned"). `zephyr-module-binding` now reads the root
`justfile`, `.github/**` and tracked Python (an argv list is judged over its
enclosing function).

Live defects fixed: `just/zephyr-setup.just` `verify-fvp-runtime` ran `fvp_runtime_ws`
(5 `skip!`) through a bare `cargo nextest` — now `_nextest-tolerant`;
`colcon_nano_ros/task/nros/build.py` configured `west build` with no module
flag — it now passes `-DZEPHYR_EXTRA_MODULES=$NROS_REPO_DIR` when an activated
shell names the checkout (a pip-installed plugin with no checkout keeps the
workspace manifest's module). Not in W2's reach: `fixture-artifact-dir-inputs`'
25-line cross-recipe exemption window (W8's shape).

### W3 — comments counted as evidence

Gates that match raw text, so a COMMENT satisfies a requirement.

Members: `check-entry-rmw-vocabulary` (P2, **live mask**: a doc comment in
`nros-rmw-xrce-cffi/src/lib.rs:27` counts as the xrce registration, so deleting
both real calls passes), `check-cyclone-backend-sources` (P2 — a commented-out
source name is issue 0984 exactly), `check-rmw-doc-slot-names` (P2 — its
motivating `try_recv_raw` resolves through comments elsewhere),
`check-required-features-reachable` (P2 — `--all-features` in a comment makes
every feature reachable), `check-platform-provider-features` (P2 — the issue-0617
row), `check-book-identifiers` (P2 — "occurs" is not "defined"),
`check-named-lane-fails`, `check-declared-fact-carriers`,
`check-workflow-repo-env` (`activate.sh` in a comment exempts).

**Fix:** one comment-stripper per language family in `scripts/lib`, applied before
every match that stands for "the code does X".

**Status: DONE.** `scripts/lib/comments.py` — `strip_comments(text, lang, *,
strings=False)`, `code_mask(text, lang)`, `lang_for(path)`, and a CLI for shell
gates (`python3 scripts/lib/comments.py [--strings] [--lang L] FILE…`). Two
families: C (`c`, `cpp`, `rust` — `//`, `/* */` nested in Rust only, C line
splices, string/char literals, Rust raw strings and lifetimes, C++ raw strings
and digit separators) and `#` (`sh`, `just`, `python`, `toml`, `yaml`, `cmake` —
per-language quoting, shell word-start `#` and heredocs, just's indented heredoc
terminators, YAML block scalars, CMake bracket comments/arguments). Output keeps
length and newlines, so offsets and line numbers still index the file; an
unterminated block comment runs to EOF (less evidence, fails closed); an
unknown language is an error, never a pass-through. Its own 63-assertion self-test
(literals holding `//`/`#`, nested and unterminated blocks, raw strings, heredocs)
runs on the normal path of every gate that imports it, and was mutation-checked
(disarming nesting, heredoc termination, YAML quote-start, digit separators, the
shell word-start rule or CMake bracket comments each fails it).

"Defined" for `check-book-identifiers`: the identifier occurs, word-bounded, in
the CODE of a tracked file in scope — comments AND string contents blanked, in a
file whose language the stripper knows. Not "has a declaration" (a second parser
per language); a `nano_ros_*()` call must still be a `function()`/`macro()`.

| member | mutation (confirmed applied by `git diff`) | before | after |
| --- | --- | ---: | ---: |
| `entry-rmw-vocabulary` | delete both real xrce registrations in `vtable.c` (doc comment in `nros-rmw-xrce-cffi` kept) | 0 | 1 |
| `cyclone-backend-sources` | comment out `"publisher.cpp"` in `build.rs` | 0 | 1 |
| `rmw-doc-slot-names` | cite `` `w3_probe_comment_only` `` in `rmw_ret.h`, name it in a `//` comment in `cffi/src/lib.rs` | 0 | 1 |
| `required-features-reachable` | an unreached `required-features` + `# never pass --all-features` in `justfile` | 0 | 1 |
| `platform-provider-features` | `# "global-allocator",` in the issue-0617 NuttX row | 0 | 1 |
| `book-identifiers` | quote `` `nros_board_init_clocks` `` (it "existed" in this gate's docstring) | 0 | 1 |
| `named-lane-fails` (rule 3) | `true # ; nros_lane_platform px4` | 0 | 1 |
| `named-lane-fails` (rule 4) | delete two of three `NROS_LANE_INCLUDED=` assignments (the comments kept the count at 3) | 0 | 1 |
| `declared-fact-carriers` | `// println!("cargo:rerun-if-env-changed=…")` in `nros/build.rs` | 0 | 1 |
| `workflow-repo-env` | `true # && source ./activate.sh` then `just setup tier2` (shared `command_lines`) | 0 | 1 |

Each member gained negative-control rows on its normal path (three —
`required-features-reachable`, `platform-provider-features`, `book-identifiers`
— had no self-test at all and left the gate-selftests baseline, 93 → 90), and
disarming its strip call fails each one. Positive control: all green on the tree.

Live defects the fixed gates surfaced, fixed here: `rmw-doc-slot-names` — 14
backticked names resolved only through comments elsewhere; 9 were ours and stale
(`pub_discard`, `loan_publish`/`commit_publish`, `entity_view` retired, the
event names abbreviated, and two ping primitives that exist in no library,
`z_send_ping` and `uxr_ping_agent_session_until_timeout`), fixed in the headers
(+ regenerated `generated.rs`) and `nros-rmw` `traits.rs`; 5 are other projects'
and joined the external-names baseline. `book-identifiers` — 15 quotes of retired
or never-existing names (`nros_platform_clock_ms`, `…_time_ns`,
`…_time_now_ms`, `…_clock_us`, `nros_rmw_ret_t`, `nros_init`) that "existed" in
comments and strings; the book pages are corrected, three deliberate citations of
absent names are EXEMPT with reasons, and a span ending in `_` is a prefix.

Private strippers retired (54 functions in 52 files, now thin wrappers keeping
their names): every C-family and `#` stripper whose semantics matched, verified by
running each owning gate before and after — identical output except
`rmw-ret-sign`, whose old stripper DELETED block comments and so reported
`service.rs:1115` for code on line 1133 (now correct). Two needed the new
`code_mask` (`nested-cargo-lock-discipline`, `message-crate-identity`): "stripped
== original" marks whitespace inside a string as code, which found a `fn` inside
a raw string. Kept, different semantics: `gate-selftests`' `_sh_mask` (masks
heredocs and multi-line strings for block extent while its guard test must see
quoted `--self-test`), `rmw-agnostic`'s `strip_c` (classifies string literals as
prose or value) and `strip_cfg_test`, `interop-cell-runners`' tokenizer-driven
`_blank_comments`, `markdown-links`' `strip_code`, `codegen-tool-reconfigure`'s
`blank_blocks`/`blank_strings`, `codegen-version-surface`'s attr/body strippers,
and the QoS-mask readers (not comment strippers). `knob-ends` and
`nros-c-feature-agreement` keep a whole-line-`#` fallback for Kconfig/`.conf`
and other suffixes no stripper models. Per-LINE wrappers
(`cpp-subscription-bound-supplied`, `no-std-entry-emission`,
`test-precondition-guards`' signature reader) cannot see a `/*` opened on an
earlier line, as before.

Found by the sweep beyond the list, same shape, fixed (each 0 → 1 under its
mutation): `check-dds-isolation-symmetry` (a commented-out `apply_to_command`
was the pin), `check-executor-stack-floor` (`#if 0 /* CONFIG_MAIN_STACK_SIZE <
… */` was the guard), `check-rmw-force-link-anchor.sh` (a commented-out
`force_link_backend!` was the anchor; now reads through the CLI — it still owes
a self-test). `check-std-census`' `guarded_features` counted a `compile_error!`
in a comment; fixed, not mutation-proved.

Suspected, NOT fixed (shell gates reading raw text): `check-rmw-required-slots.sh`
(a commented-out `expect("rmw vtable: …")` is read), `check-capability-slot-counts.sh`
(a commented-out `pub const X: usize = N` can be the first match). The migration
path is the CLI above. The sweep candidates were triaged by grep, not audited one
by one.

### W4 — an empty population reads as OK

Gates that print success over zero examined items. This is the most dangerous
class, because the gate is not merely narrow — it is off.

Members:
- **`check-cargo-custom-command-depfile` (P1).** Issue 1304 respelled the cargo
  program as `"${_ffi_cargo}"`; the lookbehind rejects `_`, so the gate examines
  zero commands, prints no count, and says "every cargo custom command has a
  DEPFILE". Deleting `DEPFILE` at all three sites it names still passes. Verified
  by the coordinator.
- `check-profile-board-mirror` (P2) — points at the retired `packages/codegen` and
  skips with rc=0 on every run, blaming a submodule that no longer exists. Verified.
- `check-nested-workspace-excludes` (P2) — the nested roots it reads are generated
  and untracked now, so it examines zero.
- `check-issue-index` (P2, **live**) — its duplicate-resolved-row arm reads the
  generated `open.md`, which has no such rows, so it can never fire. Verified.
- `check-no-std-entry-emission`, `check-host-triple-literals`,
  `check-interop-verdicts` (a missing tracked ledger reads as empty, erasing 25
  verdicts), `check-workflow-runner-isolation` and `check-required-contexts-reportable`
  (a missing PyYAML exits 0), `check-board-manifest-drift` (dead).

**Fix:** every gate prints the count it examined and FAILS on zero unless the
empty population is declared and stated. `check-vendor-fetch-pinned`'s "NOTHING TO
CHECK … not a pass" is the model; `nros_check_skip` is the ledger.

**Status: DONE.** One helper per language: `require_population(n, what, *, gate,
declared_empty=None)` in `scripts/lib/population.py`, and its shell spelling
`nros_require_population` in `scripts/lib/population.sh` (plus
`nros_require_population_self_test`). It prints the count; zero fails unless the
call site passes a reason, which prints `NOTHING TO CHECK … not a pass`. A MISSING
TOOL is not an empty population: those gates now fail (the siblings that parse
workflows already did). Each member also had its zero's ROOT cause fixed, and
gained a negative control on its normal path:

| member | root cause fixed | mutation | before | after |
| --- | --- | --- | ---: | ---: |
| `cargo-custom-command-depfile` (P1) | program regex also matches cargo held in a variable (`${…cargo}`, or bound by `nros_rust_tool(<v> cargo)`); 3 commands examined | delete `DEPFILE` at all 3 sites | 0 | 1 |
| `nested-workspace-excludes` | rewritten (`.py`): population = tracked packages under `examples/{workspaces,templates}`, cargo PREFIX semantics; 28 examined | delete root `"examples/workspaces"` exclude | 0 | 1 |
| `issue-index` | duplicate-digest arm reads `docs/issues/README.md` (325 digests), not generated `open.md` | make two digests name one id | 0 | 1 |
| `no-std-entry-emission` | each producer root must exist and hold files (4 + 11) | move both producer roots | 0 | 1 |
| `host-triple-literals` | M3 checks the tier-1 host set + live host, so no `rustc` no longer disables it; M2 (4) and M3 (19) counted | host-triple `[build] target`, `rustc` off PATH | 0 | 1 |
| `interop-verdicts` | a missing TRACKED ledger is an error (only a `--ledger` override may start empty); cells (28) and verdicts (25) counted | delete the ledger | 0 | 1 |
| `workflow-runner-isolation` | PyYAML missing fails; 17 workflows / 46 jobs counted; `runs-on: ${{…}}` fails closed | no PyYAML | 0 | 1 |
| `required-contexts-reportable` | PyYAML missing fails; empty `HOSTED_CHECKS` fails; unparseable workflow is an error; a `pull_request` `branches`/`paths` filter is refused (PR #71) | no PyYAML; empty array | 0 / 0 | 1 / 1 |
| `board-manifest-drift` | RETIRED: `check-board-descriptor-single-source` forbids its input (and its `--check-drift` verb no longer exists) | tracked `board.cmake` added | — | 1 (single-source) |
| `profile-board-mirror` | RETIRED: the generator's `PlatformProfile` table it mirrored is gone; the descriptor's `board_crate` is held to its own package by `check-derived-descriptor-fields` | `board_crate` renamed | 0 | 1 (derived-fields) |

Found by the sweep beyond the list, same shape, fixed: `check-cpp-ffi-error-mapping`
and `check-cpp-destroy-shape` (a missing tracked `nros-cpp/src` printed NOT CHECKED
with exit 0), `check-nextest-binary-filters` (a missing tracked `.config/nextest.toml`
was "nothing to check"). Each: 0 → 1 under the mutation.

Found and NOT fixed (tool/artifact-missing skips that exit 0 without the
`nros_check_skip` ledger, mostly build-tier): `check-archive-lang-items` (no `nm`),
`check-cli-fresh`, `check-launch-resolve-fresh`, `check-px4-archive-header-pairing`
(no cmake), `check-rust-targets-installed` (no rustup), `check-weak-symbols-image`
(no nm / no images), `check-artifact-identity-budget` (no tree), and
`check-dist-runtime-deps` (no store). Their fix is the ledger, not a population
count.

### W5 — scan roots that stop short of the tree

Gates rooted at `cmake/` or `packages/<some>/` that the rule's subject has since
spread beyond — usually into `zephyr/`, `packages/**/cmake`, `examples/`,
`integrations/`, or the root `justfile`.

Members (selection): `check-board-facts-delivery` (P2, possibly a **live bug** —
the NuttX lane delivers no board facts, issue 1541), `check-cc-build-policy` (P2,
**live**, issue 1542), `check-weak-symbols` (P2, **live**, issue 1543),
`check-feature-set-ssot` (P2, **live**, issue 1547), `check-emitter-just-spelling`
(P2, **live**), `check-cpp-no-std-stdio` (P2 — 212 of 285 C/C++ files outside,
including the 62 public nros-cpp headers), `check-zenohd-router-skips` (P2,
**live**), `check-no-vacuous-tests` (P2, **live**: 11 print-only tests in `src/`),
`check-doc-recipe-refs` (P2, **live**: 24 dead `just` references),
`check-markdown-links` (P2, **live**: RFC-0034's 8 links to a moved issue),
`check-no-unbounded-condvar-wait`, `check-build-profile-literals`,
`check-entry-locator-ssot`, `check-third-party-is-submodules`,
`check-nuttx-shared-tree-headers`, `check-host-platform-vocabulary` (depth 1 only),
`check-rust-targets-covered` (its own config file says its reach is narrow).

**Fix:** derive populations from `git ls-files` by FILE KIND, never from a
directory list. A gate that legitimately scopes narrower states the scope and why.

### W6 — first match, or any match, where the rule is per item

Gates that read one occurrence per file when the rule is about every occurrence.

Members:
- `check-config-header-producers` + `check-config-fallback-macros` (P2, **live
  precondition**) — `nros_config_generated_nuttx.h` defines `NROS_CODEGEN_VERSION`
  at lines 28 AND 113. The gates read the first; GCC uses the last. A version bump
  that edits line 28 alone passes both gates while every NuttX image compiles
  against line 113 — issue 1115's shape, armed. Verified. Issue 1540.
- `check-board-cargo-config-shape` (first blob only; the NuttX riscv blob is
  unchecked), `check-executor-stack-floor`, `check-dds-isolation-symmetry`,
  `check-tier-spin-gap` (a file-level `extern` prototype satisfies the rule — removing
  the gap from BOTH Zephyr tier loops passes), `check-generated-schema-coverage`,
  `check-image-paths-apply-policy`, `check-literal-domain-id` (truncates at the
  first `mod tests` substring, so `mod tests;` at `executor/mod.rs:127` hides 57
  lines of shipped code).

**Fix:** per-item matching, and refuse a duplicate definition outright where the
language makes the last one win.

**Status: DONE.** `scripts/lib/per_item.py` splits a text into its ITEMS, on
text already blanked by `comments.strip_comments` (offsets and lines hold):
`blocks(code, head)` (each head's brace-matched body; a `;` first means a
declaration, not an item), `segments(text, start)` (each match to the next),
`call_args`, `c_defines` (EVERY `#define`, with its conditional depth; the
include guard is not an arm) + `duplicate_defines` (the last-one-wins case),
and `rust_cfg_test_blank` (each `#[cfg(test)]` / `cfg(all(test, …))` ITEM
blanked — never a cut at the first one). Its self-test runs on every member's
normal path.

| member | mutation (confirmed applied by `git diff`) | before | after |
| --- | --- | ---: | ---: |
| `literal-domain-id` | `.with_domain(0)` in shipped code after `executor/mod.rs`'s `mod tests;` | 0 | 1 |
| `config-header-producers` / `config-fallback-macros` | second `#define NROS_CODEGEN_VERSION 9` in the NuttX snapshot | 1 / 1 | 1 / 1 |
| `board-cargo-config-shape` | `runnner` in the SECOND (riscv) blob of `nros-board-nuttx-qemu` | 0 | 1 |
| `executor-stack-floor` | the second `cpp.rs` emitter's `#if CONFIG_MAIN_STACK_SIZE < …` → `#if 0` | 0 | 1 |
| `dds-isolation-symmetry` | delete the pin in `cyclone_a_peer_leaving_fires_the_graph_change_guard` (the file's other pin kept) | 0 | 1 |
| `tier-spin-gap` | remove the gap step from BOTH Zephyr C tier loops (prototype kept) | 0 | 1 |
| `generated-schema-coverage` | `set_parameters.rs`: Response's `const FIELDS` renamed; separately, its `begin_dheader` | 0 / 0 | 1 / 1 |
| `tier-priority-plan` (sweep) | a second, smaller `#define configMAX_PRIORITIES` in `FreeRTOSConfig.h` | 0 | 1 |

The two config gates were already fixed by issue 1540 (verified, rc=1 before);
they now read through `c_defines` / `duplicate_defines` with identical output.
The rules as judged per item: a blob per `[[board]]` (8 now, was 7); a guard per
emitted `#define NROS_EXECUTOR_MAIN_STACK_MIN` (segment to the next define); a
pin per FUNCTION that both starts a pinned peer and spawns a `Command` (in the
function, or via a same-file function that pins — a function is the grain that
needs no dataflow); the gap TAKEN in each loop body (`nros_tier_spin_gap_step(`
or `.after_spin(`), not named in the file; FIELDS per struct with fields (unit
markers exempt) and a DHEADER per `fn serialize` (75 of each; the count used to
be files, 63). Each gained normal-path negative controls;
`generated-schema-coverage` had no self-test and left the baseline (89 → 88).

Per-loop judging surfaced three single-executor entry loops in files that also
run tiers (`nros-board-freertos::app_task_entry_runtime`,
`nros-board-nuttx::run_entry`, `nros-board-threadx::run_app_thread`); they are
exempt under the gate's own single-tier rule, keyed on (file, FUNCTION) so the
exemption cannot cover a tier loop added beside them.

Ruled NOT a hole: `image-paths-apply-policy` "per file, not per target".
`nros_apply_panic_policy` sets a GLOBAL property on the one nros-c/nros-cpp
staticlib every image in a build links, so one call covers every target in the
file; the mutation (a second raw `add_executable` linking the umbrella) is
covered by construction. Its population (1 file) is W5's business.

Sweep, moved onto `rust_cfg_test_blank` (output identical on the tree):
`entry-session-name` and `config-knob-census` CUT the file at the first
`#[cfg(test)]`; `zenoh-source-manifest`'s private stripper took the next `{`
even across a `;`, so `#[cfg(test)] mod t;` blanked the following item;
`rmw-agnostic`'s `strip_cfg_test` is now a wrapper. `qos-profile-ssot` reads
first-match `#define`s from ROS's own `rmw/time.h`, upstream, left as is.

### W7 — authored lists where the population should be harvested

Gates whose population is a hand-maintained list that has gone stale.

Members: `check-ffi-struct-mirrors` (2 of 3 mirrored structs; the unchecked one
carries `callback_group`, the field that drifted twice), `check-fixture-binary-names`,
`check-lane-contracts` (5 `require_*` names; the ~320 `build_*` resolvers emit the
exact failure it exists to prevent), `check-decoupling` (lists `posix-c`, which does
not exist; misses four real platform crates), `check-atomic-sync-writes`,
`check-msg-dep-is-path` (**live**: `px4_msgs` and `custom_msgs`),
`check-rmw-ret-sign` (43 of 66 status slots; and its headline rule has no failing
path at all), `check-codegen-tool-reconfigure`, `check-cxx-compat-shim-coverage`,
`check-sdk-store-not-enumerated` (**live**, issue 1546).

**Fix:** harvest from the source of truth; turn the authored list into an
EXEMPTION list whose entries carry reasons.

### W8 — exemptions wider than their rationale

Members: `check-test-precondition-guards` (**P1, live** — exempts a helper that
returns a real value "because the caller will `skip!`", which nothing checks; issue
1539), `check-one-producer-per-tool` (forwarding ANY tool excuses producing any
other), `check-posix-platform-purity` (any `__linux__` within 12 lines counts as a
guard, even after `#endif`), `check-lane-scope-consumers` (exempts `native_*`
files, which the host lane runs), `check-tier-has-ci-owner` (a step `name:` line
counts as an owner; `just ci matrix build` counts as owning tier 2),
`check-fixture-artifact-dir-inputs`, `check-rmw-agnostic`, `check-ros2-daemon-queries`,
`check-retired-submodule-refs`.

**Fix:** key each exemption on the exact thing its rationale names, and give every
exemption a selftest row showing it does NOT cover the neighbouring case.

### W9 — selftest discipline, including the meta-gate

- **`check-gate-selftests` misclassifies (P2, live).** It does not recognise
  `if args.self_test:` as a flag guard, so 17 gates that run their selftest ONLY
  behind a flag are counted compliant — among them `check-no-vacuous-tests`, which
  the meta-gate reports as running its selftest on the normal path. Verified. It
  also mis-baselines `check-image-paths-apply-policy`, which runs its controls
  inline.
- **`check-sdk-store-not-enumerated`'s docstring says "the self-test below is the
  point, not a formality" — and there is no self-test.** Verified.
- **Ratchets never forced down** — `check-wait-evidence-discarded` baselines
  counts no file is held to (a file can regrow to its old count); suspected in
  `check-unsafe-census` and `check-kconfig-overridden-values`.
- **Selftests that exercise a copy** — `check-fixture-require` and
  `check-self-pkg-package-xml` test a different matcher than the one the gate runs.

**Fix:** the meta-gate recognises flag guards; every gate with a confirmed hole
above gains the negative control that would have caught it; a ratchet fails when
a count rises above ANY recorded value, not only above its own row.

**Results (W9 landed).**
- `check-gate-selftests` now asks REACHABILITY, not line shape: Python on the AST
  (module statements + every module function they call), shell by block extent
  (function bodies, flag-guarded `if`/`case` arms; comments, heredocs and
  multi-line strings masked; `python3 - <<'PY'` bodies classified as Python). A
  guard is any condition that NAMES the flag (`--self-test`, `args.self_test`,
  `$X_SELFTEST`), never one that calls the routine; a default-True recursion
  stopper and `[ -z "$X_SELFTEST" ]` are the normal path. 34 always-run rows
  cover every guard spelling both ways. It reclassified exactly the audit's 17
  flag-only gates; all 17 now run their selftest on the normal path (quiet on
  success), and 9 recipe lines that invoked `--self-test` a second time are
  gone. `check-image-paths-apply-policy` and `check-wait-evidence-discarded`
  converted too and left the baseline (95 -> 93).
- `scripts/lib/ratchet.py` (`judge` + `fell_instructions`): a count above its row
  fails, and a count BELOW its row fails too, naming the exact baseline edit.
  Members: `check-wait-evidence-discarded` (LIVE: 77 sites vs 87 recorded — ten
  sites of regrowth room; baseline lowered here), `check-unsafe-census`,
  `check-kconfig-overridden-values` (stale entries), and by the class sweep
  `check-grep-q-error-conflation` and `check-fixture-require`, which printed the
  same "shrink it" note. Each selftest drives its gate's own `verdict()`.
- Copy selftests: `check-self-pkg-package-xml` now drives `is_violation`, the
  predicate its scan runs. `check-fixture-require` was already fixed by issue
  1544 (verified: blanking the syntactic-bypass scan fails its selftest).
- `check-sdk-store-not-enumerated` (PR #1439) verified: its selftest runs on the
  normal path and fails when the shape-2 scan is blanked; it gained a shape-1-only
  BAD case, because blanking `LITERAL` alone left it green.

---

## Live defects filed separately

The classes above are gates that fail to guard. These are things **broken in the
tree today**, which the audit found because the gates meant to catch them could
not:

| issue | defect |
| --- | --- |
| 1539 | **P1.** Five zenoh integration tests `let Some(_) = router() else { return };` — PASS having run nothing on a host without zenohd |
| 1540 | `NROS_CODEGEN_VERSION` defined twice in the NuttX config header; gates read the first, GCC the last |
| 1541 | the NuttX cargo lane delivers no board facts — needs a ruling on whether it must |
| 1542 | an ungoverned `cc::Build` in `examples/mps2-an385-baremetal/c/talker/build.rs:94`, on issue 0478's compiler |
| 1543 | three weak-symbol sites no audit list covers |
| 1544 | test-harness violations the gates missed: 18 fixture `match` bypasses, 11 print-only `src/` tests, a zombie-blind `ps` scan, 6 `ZenohRouter::start*().expect()` |
| 1545 | dangling references outside every gate's scope: 24 dead `just` recipes, RFC-0034's 8 broken links, 6 dangling issue ids, ledger citations, and `book/src/reference/c-api.md:72` (red on `main` in a non-gating lane) |
| 1546 | the SDK store enumerated newest-version-first in two places — pending a ruling on whether issue 0500's ordering survives phase-365 |
| 1547 | `ros-humble` hardcoded 11 times in `zephyr/CMakeLists.txt` |
| 1548 | the `setup-qemu-patched` composite action bypasses the repo-env and indexed-apt rules |

## Acceptance

Per work item: the named fix lands as ONE shared helper, every member gate moves
onto it, each member gains the negative control that would have caught its
recorded hole, and the mutation recorded here is re-run and now FAILS.

For the phase: re-running this audit's method over the same gates finds no
confirmed hole in any class W1–W9. The audit is repeatable by design — six
buckets, one question, mutation-confirmed — and should be re-run whenever a class
fix lands, because a class fix that reached only its reported members is how this
phase came to be necessary.
