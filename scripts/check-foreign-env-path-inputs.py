#!/usr/bin/env python3
"""A build script that reads a FOREIGN environment variable classifies it.

issue 1588 — the third subject of RFC-0101 D3, and the one neither of the first
two can express.

# What this adds to `check-build-script-path-resolution`

That gate asks whether a build script resolving a **path-valued SDK variable**
goes through `nros_build_paths`, the one implementation of issue 1280's
three-valued rule. Its POPULATION is derived; its SUBJECT is not. A variable is
in its subject because somebody wrote a row for it — in `just/sdk-env.just` or a
board descriptor's `cargo_config [env]` — and two path-valued inputs have no row
and can have none (`ZENOH_PICO_DIR` names a user's own install prefix;
`NV_SPE_FSP_DIR` ships under an EULA that forbids vendoring). Issue 1560 fixed
both call sites and nothing stopped the next one.

# Why this is NOT "a name whose value is used as a path"

That was issue 1588's own proposed direction, and **measuring it is what refuted
it**. It fails in both directions at once:

* **Too wide.** `OUT_DIR`, `CARGO_MANIFEST_DIR` and `DEP_*` are read and joined
  as paths in 55 places. They are cargo's OWN namespace — set per build,
  absolute, and incapable of naming another checkout — so re-rooting them would
  be wrong, not missing. A type-flow test reports every one of them.
* **Too narrow.** `NUTTX_LINKER_SCRIPT`, `APP_MAIN_CPP`, `APP_INCLUDE_DIRS`,
  `APP_FFI_LIBS_FILE` and four more are paths that never become a `Path` — they
  are carried as `String` into a `cc` flag or written into a file. "A path
  resolver says so in its types" is true of a HELPER, which returns `PathBuf`;
  it is false of a variable read in a script that handles paths as strings.

So path-ness is not decidable here, and this gate does not try. It asks the
question that IS decidable, and leaves the classification to the one place it is
known — the call site.

# The subject, three derived predicates

A read is FOREIGN when all three hold:

1. **no row** — the name is not in `scripts/nros-build-wiring.py`'s
   `path_vars()`, so neither existing producer declares it;
2. **not cargo's** — the name is outside cargo's documented build-script
   namespace (prefix `CARGO_`/`DEP_`/`RUSTC`/`RUSTDOC`, or one of the fixed
   names cargo sets). Prefix-shaped, so this is a rule and not a list of ours;
3. **nothing here sets it** — no `export`/`set(ENV{})`/assignment anywhere in
   `just/`, `cmake/`, `scripts/`, `zephyr/`, `packages/cli/` or `examples/`.
   A name this repo produces is an INTERNAL channel whose value we chose;
   a name nothing here sets arrived from outside.

Predicate 3 is what separates `THREADX_PORT` (set by
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`) and `NUTTX_LD_SCRIPT` (set
by `scripts/nuttx/riscv-env.sh`, and RELATIVE, so outside issue 1280 by
construction) from `APP_MAIN_CPP`, which NuttX's own apps build hands us.

# The rule

A foreign read goes through `nros_build_paths::env_path` (or a documented
sibling), **or** it carries a baseline row saying what it is. Not a path, a
count, a flag, a compiler string — any of those is a fine answer; having no
answer is not. That is RFC-0101 D3 read literally: the rule is about the CALL,
not about which value happens to arrive.

`.config/foreign-env-path-inputs-baseline.txt` is a RATCHET — it may only
SHRINK. It is seeded with the 19 names the measurement found, each with its
classification, so a twentieth cannot appear unclassified. Seeding rather than
migrating is deliberate: the `APP_*` family is the NuttX apps channel and
changing how those resolve needs a NuttX build to accept it, which is a
different piece of work from stopping the next one.

Run:  python3 scripts/check-foreign-env-path-inputs.py [--self-test] [--write-baseline]
"""

from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CENSUS = ROOT / "scripts/nros-build-wiring.py"
BASELINE = ROOT / ".config/foreign-env-path-inputs-baseline.txt"

# Module-level so `check-baseline-shape` can hold the file to it.
BASELINE_HEADER = (
    "# issue 1588 — FOREIGN environment variables a build script reads without\n"
    "# routing them through `nros_build_paths`. A RATCHET: it may only SHRINK.\n"
    "#\n"
    "# Foreign means all three: no `sdk-env.just`/board-descriptor row, outside\n"
    "# cargo's own namespace, and nothing in this repo sets it. So the value arrives\n"
    "# from outside, and an inherited one outranks the checkout being built (1280).\n"
    "#\n"
    "# Each row is `<NAME>  <classification>`. The classification is what the value\n"
    "# IS, because that is the thing the call site knows and no probe can infer —\n"
    "# issue 1588 measured both failures of inferring it (cargo's own path-valued\n"
    "# variables report as violations; paths carried as `String` are invisible to a\n"
    "# type test).\n"
    "#\n"
    "# A row marked `path, unrouted` is a KNOWN defect of 1560's class, kept visible\n"
    "# rather than fixed blind: accepting a change to how the NuttX apps channel\n"
    "# resolves needs a NuttX build, which is separate work. Every run prints how\n"
    "# many remain.\n"
    "#\n"
    "# A row leaves when the call site routes through `nros_build_paths::env_path`\n"
    "# (or `env_path_list` for a `;`-separated one), or when the name gains a\n"
    "# producer in this repo and stops being foreign.\n"
    "#\n"
    "# Regenerate ONLY to record a name that is gone:\n"
    "#     python3 scripts/check-foreign-env-path-inputs.py --write-baseline\n"
)

# Cargo's own build-script namespace. Prefix-shaped plus the fixed names cargo
# documents; not a list of OUR variables, which is what keeps this a rule.
CARGO_PREFIX = ("CARGO_", "DEP_", "RUSTC", "RUSTDOC")
CARGO_EXACT = {
    "OUT_DIR", "TARGET", "HOST", "PROFILE", "NUM_JOBS", "OPT_LEVEL", "DEBUG",
}

# Where this repo could set a variable. A name set here is an internal channel.
PRODUCER_DIRS = ["just/", "cmake/", "scripts/", "zephyr/", "packages/cli/", "examples/"]


def load_census():
    spec = importlib.util.spec_from_file_location("nros_build_wiring", CENSUS)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def cargo_owned(name: str) -> bool:
    return name.startswith(CARGO_PREFIX) or name in CARGO_EXACT


def repo_produces(name: str) -> str | None:
    """The first file in this repo that SETS `name`, or None.

    Several spellings because the producers are in different languages, and
    missing one would make a name read as foreign when we chose its value —
    reporting a site that has no defect (issue 1452's direction).
    """
    patterns = [
        rf"export {re.escape(name)}=",
        rf"\bset\s*\(\s*ENV\{{{re.escape(name)}\}}",
        rf"\bENV\{{{re.escape(name)}\}}",
        rf"^\s*{re.escape(name)}\s*=",
        rf'["\']{re.escape(name)}["\']\s*,',
    ]
    for pat in patterns:
        r = subprocess.run(
            ["git", "grep", "-lE", pat, "--", *PRODUCER_DIRS],
            cwd=ROOT, capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            return r.stdout.split()[0]
    return None


def foreign_reads(census) -> dict[str, set[str]]:
    """{name: {file, …}} for every FOREIGN read, by the three predicates."""
    rows = census.build_scripts() + census.build_script_libs()
    subject = set(census.path_vars())
    out: dict[str, set[str]] = {}
    for r in rows:
        code = census.strip_line_comments(
            (ROOT / r["path"]).read_text(errors="replace"))
        for m in census.RAW_ENV_READ.finditer(code):
            name = m.group(1)
            if name in subject or cargo_owned(name):
                continue
            # Already routed? `env_path` takes the name as a LITERAL there, so a
            # read that reaches the resolver is not a bare read at all.
            if re.search(rf'nros_build_paths::env_path(?:_list|_watched)?\(\s*"{re.escape(name)}"',
                         code):
                continue
            if repo_produces(name):
                continue
            out.setdefault(name, set()).add(r["path"])
    return out


def read_baseline() -> dict[str, str]:
    if not BASELINE.is_file():
        return {}
    rows = {}
    for line in BASELINE.read_text().splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        name, _, why = line.partition(" ")
        rows[name] = why.strip()
    return rows


def self_test() -> int:
    bad = 0

    def chk(label: str, ok: bool) -> None:
        nonlocal bad
        print(f"  {'ok' if ok else 'FAIL'}  {label}")
        if not ok:
            bad = 1

    # Predicate 2, both directions. These are the false positives that refuted
    # issue 1588's original "used as a path" formulation, so they are asserted
    # rather than described.
    chk("cargo's own namespace is not foreign (OUT_DIR, CARGO_*, DEP_*)",
        all(cargo_owned(n) for n in
            ("OUT_DIR", "CARGO_MANIFEST_DIR", "DEP_DDSC_INCLUDE", "TARGET", "RUSTC")))
    chk("…and an SDK variable is not mistaken for one of cargo's",
        not any(cargo_owned(n) for n in
                ("THREADX_DIR", "ZENOH_PICO_DIR", "NV_SPE_FSP_DIR", "APP_MAIN_CPP")))

    # Predicate 3 — measured against the two names the probe separated.
    chk("a name this repo sets is an internal channel, not foreign",
        repo_produces("THREADX_PORT") is not None)
    chk("…and one nothing here sets is foreign",
        repo_produces("NROS_DEFINITELY_NOT_A_REAL_VARIABLE_1588") is None)

    # The baseline must parse into rows that carry a reason. A row with no
    # classification is the thing this gate exists to refuse, so an empty
    # second field must not read as a valid entry.
    base = read_baseline()
    chk("the baseline parses and is non-empty", len(base) > 0)
    chk("every baseline row carries a classification",
        all(v for v in base.values()))

    census = load_census()
    chk("the census still exposes what the subject is derived from",
        bool(census.path_vars()) and bool(census.build_scripts()))
    return bad


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    census = load_census()

    if "--write-baseline" in sys.argv:
        found = foreign_reads(census)
        old = read_baseline()
        lines = [BASELINE_HEADER]
        for name in sorted(found):
            why = old.get(name) or "UNCLASSIFIED — say what this value is"
            lines.append(f"{name}  {why}\n")
        BASELINE.write_text("".join(lines))
        print(f"wrote {BASELINE.relative_to(ROOT)} with {len(found)} row(s)")
        return 0

    # The control runs on the NORMAL path (check-gate-selftests): a negative
    # control nobody invokes reads as coverage while proving nothing.
    if self_test() != 0:
        print("[FAIL] the self-test did not pass, so nothing below is trustworthy.",
              file=sys.stderr)
        return 1

    found = foreign_reads(census)
    base = read_baseline()

    unlisted = sorted(set(found) - set(base))
    gone = sorted(set(base) - set(found))
    unclassified = sorted(n for n in base if not base[n]
                          or base[n].startswith("UNCLASSIFIED"))

    rc = 0
    if unlisted:
        rc = 1
        print("[FAIL] build script(s) read a FOREIGN environment variable without "
              "stating what it is:", file=sys.stderr)
        for n in unlisted:
            print(f"       {n}", file=sys.stderr)
            for p in sorted(found[n]):
                print(f"         {p}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  Foreign means: no `sdk-env.just`/descriptor row, not cargo's own, and", file=sys.stderr)
        print("  nothing in this repo sets it — so its value arrives from outside and", file=sys.stderr)
        print("  an inherited one outranks the checkout being built (issue 1280).", file=sys.stderr)
        print("", file=sys.stderr)
        print("  If it names a PATH: resolve it through", file=sys.stderr)
        print("      nros_build_paths::env_path(\"<NAME>\")", file=sys.stderr)
        print("  If it does not: add a row to", file=sys.stderr)
        print(f"      {BASELINE.relative_to(ROOT)}", file=sys.stderr)
        print("  saying what it is (a count, a flag, a compiler string). Having no", file=sys.stderr)
        print("  answer is what this refuses — RFC-0101 D3 is a rule about the CALL.", file=sys.stderr)

    if unclassified:
        rc = 1
        print(f"[FAIL] {len(unclassified)} baseline row(s) have no classification:",
              file=sys.stderr)
        for n in unclassified:
            print(f"       {n}", file=sys.stderr)

    if gone:
        # Not a failure: the ratchet falling is the point. Reported so the row
        # is removed rather than vouching for a site that no longer exists.
        print(f"{len(gone)} baseline row(s) no longer apply — remove them "
              f"(`--write-baseline`):")
        for n in gone:
            print(f"    {n}")

    if rc == 0:
        unrouted = sorted(n for n in base if base[n].startswith(("path,", "mixed")))
        files = len({p for ps in found.values() for p in ps})
        print(f"check-foreign-env-path-inputs: OK — {len(found)} foreign read(s) "
              f"across {files} file(s), every one classified; baseline "
              f"{len(base)} (ratchet, falls only).")
        if unrouted:
            # Printed on every run, not only when it changes: a known defect
            # parked in a baseline is only safe while somebody can still see it.
            print(f"  {len(unrouted)} of those name a PATH and are not yet routed "
                  f"through nros_build_paths (issue 1560's class, issue 1588 §backlog):")
            for n in unrouted:
                print(f"    {n}  — {base[n]}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
