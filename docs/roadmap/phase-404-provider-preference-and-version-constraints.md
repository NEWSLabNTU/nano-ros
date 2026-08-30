# phase-404 — provider preference, version constraints, and saying which one won

**Implements:** [RFC-0062](../design/0062-unified-dependency-ssot.md) amendment 2
(2026-08-30).
**Status (2026-08-30). PLANNED — no code yet.** The design is decided; this
sequences it. W1 is the gate on everything else, because preference is
unimplementable without a version constraint.
**Informed by:** RFC-0075 (the zenohd precedent — a pinned copy drifting from
what ROS runs), issue 0500 (the store accumulates, so a stale prefix shadows a
pin), issue 0929 (`smoke`: presence is not the same question as works).

## Why

If a tool is available from the system, shipping a copy needs a reason. RFC-0062
amendment 2 gives the reason two forms — a build input must be PINNED, and an
interop peer must come from the SYSTEM — and leaves the rest as "prefer the
system if it is good enough".

Nothing can act on that today: `check` is presence-only, so "good enough" has no
expression in the index.

## Work

- [ ] **W1 — a version constraint in `check`.** A floor plus a declared way to
      read the installed version. Two sub-questions worth settling with evidence
      rather than taste:

      * **Where the extraction lives.** `--version` output is not uniform
        (`openocd` prints `Open On-Chip Debugger 0.12.0-g9ea7f3d`, ARM prints a
        parenthesised release id). A regex per entry is flexible and invites
        drift; a table of known shapes is rigid and honest. Prefer the table
        until a tool needs otherwise.
      * **What "satisfies" means.** A floor (`>=`) is right for tools where
        newer is fine (`openocd`), and WRONG where the pin is an interop
        contract (`cyclonedds` must match what ROS ships, not exceed it —
        issue 0507). So the constraint needs at least `min` and `exact`.

      Acceptance: an index entry can say "system `openocd` >= 0.12 satisfies
      this" and `nros setup --check` agrees with the host.

- [ ] **W2 — `providers` becomes real ordered preference.** The vocabulary
      already exists (`Provider::{System,Sdk,Source,Submodule}`, and
      `PrereqDep.providers` is already a `Vec`), unused since phase-327 W1.
      Resolution walks it in order and stops at the first provider that
      SATISFIES (W1), not the first that is merely present.

      `--offline` removes `system` from consideration rather than reordering —
      amendment 2 decides preference is a default and offline is an override.

      Acceptance: a host with a satisfying `openocd` uses it; the same host with
      `--offline` uses the dist; a host with 0.11 uses the dist either way.

- [ ] **W3 — every resolution reports its provider and version.**
      `nros setup --check` and `just doctor` say which provider satisfied each
      tool. Not cosmetic: without it a host that quietly used its own 0.11 makes
      "works on my machine" unfalsifiable, and the whole point of preferring the
      system is that we can then TELL when it was preferred.

      Acceptance: the check output distinguishes `system 0.12.0` from
      `sdk 0.12.0-nros2` for the same tool, and a fixture/test asserts it.

- [ ] **W4 — classify every `[tool.*]` against amendment 2's two questions**, in
      the doc, with the answer recorded per tool. Expected outcome from the
      measurement already in the amendment: `sccache`, `corrosion`, `espflash`,
      `xrce-agent` cannot move (not packaged anywhere yet); the compilers and
      `corrosion` must not move (build inputs); `openocd` and `genromfs` are the
      real candidates. NOTHING is deleted in this work item — it produces the
      list, and each deletion is its own change with its own evidence.

## Sequencing

W1 gates W2 (preference needs satisfaction) and W2 gates W3 (there is nothing to
report until more than one provider can win). W4 is doc-only and can land any
time; doing it FIRST is tempting and wrong, because a classification written
before the constraint exists is a guess that later reads as a decision.

## Non-goals

* Deleting any dist. Amendment 2 is explicit that no dist moves on the strength
  of one distro's archive, and the measured Ubuntu 22.04 table shows why: four
  of ten tools are not packaged at all and most of the rest are years behind.
* Relaxing question 1 (build inputs stay pinned). If that is ever revisited it
  needs its own evidence; the reproducibility argument rests on it entirely.
