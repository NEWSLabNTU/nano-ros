#!/usr/bin/env python3
"""What would the next toolchain bump cost? — issue 1642.

## Why this exists

Issue 1447 pinned `rust-toolchain.toml` to a version (PR #1556) after the
floating `stable` channel took `main` red for everyone on the day Rust 1.99.0
released. The pin trades that risk for drift: nothing says a bump is due, or
how big it will be, and a pin left alone for three releases arrives carrying
three releases of new lints at once. Clearing ONE release (1.98 -> 1.99) was
nine sites across three tools — 2 clippy, 5 rustdoc, 7 rustc future-incompat
errors (PR #1536).

This is PR #1536's A/B made routine: the same tree, the same gates, and the
toolchain the only variable —

    RUSTUP_TOOLCHAIN=<pin>        just check <gate>
    RUSTUP_TOOLCHAIN=<candidate>  just check <gate>

— and it reports the diagnostic SITES the candidate raises that the pin does
not. That set is the bump's price, visible before anyone pays it.

## It must never gate a merge

A gating lane on an unpinned toolchain blocks every pull request on a compiler
nobody chose — issue 1447 exactly. So the workflow that runs this
(`.github/workflows/next-stable.yml`) is `schedule` + `workflow_dispatch` ONLY,
lives outside `gate.yml` (whose `ci-ok` job — the `CI` context, the one
required check — can `needs:` only jobs in its own file), and its job is not
named `CI`, the bare context string the ruleset requires. `--selftest` checks
all three statically, because each is a one-line edit away from re-creating
1447 and none of them would fail anything on the day it was made.

## Three outcomes, not two (issue 1043's / 1158's shape)

    exit 0  VERDICT, FREE     the candidate raises nothing the pin does not —
                              or the candidate IS the pin. Take the bump.
    exit 1  VERDICT, PRICED   the candidate raises N new sites (listed). That
                              is what the next bump costs, today.
    exit 2  NO VERDICT        something stopped the A/B from measuring: the pin
                              arm is itself red (then the delta is not about the
                              toolchain), or a gate failed under the candidate
                              with no diagnostic this could parse (read the
                              log). Fix the lane, not the code.

## What the count is, precisely

A SITE is (gate, file:line:col, message) from a diagnostic header that carries
a `-->` location. The count is a LOWER BOUND: cargo aborts an invocation at the
first crate that fails, so a crate behind a failing sibling in the same
invocation is never linted. `test-targets` additionally lints every crate
ALONE, which is what made #1536's clippy sites findable at all; the rustdoc and
embedded gates are one invocation each. `--keep-going` would close that, and is
not passed: it is a per-invocation flag with no config key (measured — neither
`CARGO_BUILD_KEEP_GOING` nor `--config build.keep-going` is honoured), so
passing it means editing every recipe or the `scripts/bin/cargo` shim that
every cargo call in the tree goes through. Not worth that for an advisory
number; worth saying.

Usage::

    next-stable-delta.py [--candidate stable] [--install] [--out DIR]
                         [--gates g1,g2,...]
    next-stable-delta.py --selftest
"""

import argparse
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLCHAIN_FILE = ROOT / "rust-toolchain.toml"
WORKFLOW = ROOT / ".github" / "workflows" / "next-stable.yml"
GATE_WORKFLOW = ROOT / ".github" / "workflows" / "gate.yml"

# The gates a bump PR must clear that a TOOLCHAIN can move — i.e. the
# merge-gating compile gates that lint. Each is run by `gate.yml`'s `check`
# job on a merge-gating event, which is what makes it the bump's price:
#
#   test-targets        host clippy, --workspace and per crate, + cli-clippy
#                       (via `check workspace-all`, pull_request + merge_group)
#   workspace-embedded  thumbv7em clippy (via `check workspace-all`, same)
#   rustdoc-links       the published crates' rustdoc (pull_request)
#   rustdoc-workspace   every crate's rustdoc (pull_request)
#
# These are exactly the five that failed on 2026-10-01 (`workspace-all` being
# the parent of the first two). `compile-smoke` is not listed: it is
# `cargo check` over a subset of what `test-targets` clippies, with no
# `-D warnings`, so it cannot report a site the clippy gate misses.
# `--selftest` checks every entry is still invoked by gate.yml, so a renamed
# step drifts this list loudly rather than into measuring a gate nothing runs.
GATES = [
    ("test-targets", "workspace-all"),
    ("workspace-embedded", "workspace-all"),
    ("rustdoc-links", "rustdoc-links"),
    ("rustdoc-workspace", "rustdoc-workspace"),
]

# The job-name the ruleset requires. A job of this name in ANY workflow would
# report into the required context.
REQUIRED_CONTEXT = "CI"

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
HEADER = re.compile(r"^(error|warning)(?:\[(?P<code>[A-Za-z0-9_]+)\])?: (?P<msg>.+)$")
LOCATION = re.compile(r"^\s*--> (?P<loc>\S+?:\d+:\d+)\s*$")
# Header lines that are summaries, not diagnostics.
SUMMARY = re.compile(
    r"^(could not compile|aborting due to|build failed|recipe `|process didn't exit"
    r"|`[^`]+` \([^)]*\) generated \d+|\d+ warnings? emitted|could not document"
    r"|Compilation failed|failed to run custom build command)"
)
CLIPPY_URL = re.compile(r"rust-clippy/[^#\s]*#(?P<lint>[a-z0-9_]+)")
LINT_NOTE = re.compile(r"`(?:#\[(?:deny|warn)\(|-[DW] )(?P<lint>[a-z0-9_:]+)")


def pinned_channel():
    m = re.search(r'^channel\s*=\s*"([^"]+)"', TOOLCHAIN_FILE.read_text(), re.M)
    if not m:
        sys.exit(f"next-stable: no `channel = ...` in {TOOLCHAIN_FILE}")
    return m.group(1)


def rustc_version(toolchain):
    """`1.99.0` for a toolchain name, or None if rustup cannot run it."""
    try:
        out = subprocess.run(["rustup", "run", toolchain, "rustc", "-V"],
                             capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    m = re.match(r"rustc (\S+)", out)
    return m.group(1) if m else None


def install(toolchain):
    """Install a NAMED toolchain. Never touches the default or the pin."""
    subprocess.run(["rustup", "toolchain", "install", toolchain,
                    "--profile", "minimal", "--component", "clippy",
                    "--target", "thumbv7em-none-eabihf", "--no-self-update"],
                   check=True)


def normalise(loc):
    """Make a location comparable across arms: repo-relative, registry-trimmed."""
    root = str(ROOT) + "/"
    if loc.startswith(root):
        loc = loc[len(root):]
    m = re.search(r"/registry/src/[^/]+/(.+)$", loc)
    return f"<registry>/{m.group(1)}" if m else loc


def parse(text, gate):
    """{(gate, loc, msg): lint} — one entry per diagnostic SITE in a log."""
    sites = {}
    cur = None  # [msg, loc, lint]

    def flush():
        if cur and cur[1]:
            key = (gate, cur[1], cur[0])
            sites.setdefault(key, cur[2])

    for raw in text.splitlines():
        line = ANSI.sub("", raw)
        h = HEADER.match(line)
        if h:
            flush()
            msg = h.group("msg").strip()
            cur = None if SUMMARY.match(msg) else [msg, None, h.group("code")]
            continue
        if cur is None:
            continue
        loc = LOCATION.match(line)
        if loc and cur[1] is None:
            cur[1] = normalise(loc.group("loc"))
            continue
        if cur[2] is None:
            m = CLIPPY_URL.search(line)
            if m:
                cur[2] = "clippy::" + m.group("lint")
                continue
            m = LINT_NOTE.search(line)
            if m:
                cur[2] = m.group("lint")
    flush()
    return sites


def tool_of(gate, lint):
    if (lint or "").startswith("clippy::"):
        return "clippy"
    if (lint or "").startswith("rustdoc::") or gate.startswith("rustdoc"):
        return "rustdoc"
    return "rustc"


def run_gate(gate, toolchain, logdir):
    env = dict(os.environ, RUSTUP_TOOLCHAIN=toolchain)
    # gate.yml runs rustdoc-links strict because its lane provisions the
    # sources; this lane provisions the same set, so an absent source is a
    # lane defect here too, not a skip to wave through.
    env.setdefault("NROS_RUSTDOC_LINKS_STRICT", "1")
    log = logdir / f"{gate}.{toolchain}.log"
    t0 = time.monotonic()
    with open(log, "w") as fh:
        rc = subprocess.run(["just", "check", gate], cwd=ROOT, env=env,
                            stdout=fh, stderr=subprocess.STDOUT).returncode
    secs = time.monotonic() - t0
    return rc, log, secs


def report(pin, pin_ver, cand, cand_ver, rows, new_sites, out):
    """Markdown report. Returns (exit_code, text)."""
    no_verdict = [r for r in rows if r["pin_rc"] != 0 or
                  (r["cand_rc"] != 0 and r["new"] == 0)]
    if no_verdict:
        code, head = 2, "NO VERDICT — the A/B could not measure the toolchain"
    elif new_sites:
        code, head = 1, f"PRICED — the next bump costs at least {len(new_sites)} site(s)"
    else:
        code, head = 0, "FREE — the candidate raises nothing the pin does not; take the bump"

    by_tool = {}
    for (gate, _loc, _msg), lint in new_sites.items():
        t = tool_of(gate, lint)
        by_tool[t] = by_tool.get(t, 0) + 1

    lines = [
        "## Next-stable advisory (issue 1642)",
        "",
        f"**{head}.**",
        "",
        f"Pin `{pin}` (rustc {pin_ver}) vs candidate `{cand}` (rustc {cand_ver}), "
        "same tree, toolchain the only variable. This lane is advisory: it gates "
        "no merge.",
        "",
        "| gate | pin | candidate | new sites |",
        "| --- | --- | --- | --- |",
    ]
    for r in rows:
        lines.append(f"| `{r['gate']}` | {'ok' if r['pin_rc'] == 0 else 'FAIL'} "
                     f"({r['pin_s']:.0f}s) | {'ok' if r['cand_rc'] == 0 else 'FAIL'} "
                     f"({r['cand_s']:.0f}s) | {r['new']} |")
    lines.append("")
    if by_tool:
        lines.append("By tool: " + ", ".join(f"{n} {t}" for t, n in sorted(by_tool.items())))
        lines.append("")
    if new_sites:
        lines += ["| tool | lint | site | gate | message |",
                  "| --- | --- | --- | --- | --- |"]
        for (gate, loc, msg), lint in sorted(new_sites.items(),
                                             key=lambda kv: (tool_of(kv[0][0], kv[1]), kv[0][1])):
            m = msg.replace("|", "\\|")
            lines.append(f"| {tool_of(gate, lint)} | `{lint or '?'}` | `{loc}` | {gate} | {m} |")
        lines.append("")
        lines.append("A **lower bound**: cargo stops an invocation at the first failing "
                     "crate, so crates behind it were not linted. Re-run after fixing.")
        lines.append("")
    for r in no_verdict:
        if r["pin_rc"] != 0:
            lines.append(f"- `{r['gate']}` is RED UNDER THE PIN — `main` itself fails it, so "
                         "nothing here is attributable to the toolchain. See its pin log.")
        else:
            lines.append(f"- `{r['gate']}` failed under the candidate with no parsed "
                         "diagnostic — read `" + r["cand_log"].name + "` in the artifact.")
    if no_verdict:
        lines.append("")
    lines.append("Reproduce locally: `rustup toolchain install " + cand +
                 " --profile minimal -c clippy -t thumbv7em-none-eabihf` (a NAMED "
                 "toolchain — never `rustup update`), then `scripts/ci/next-stable-delta.py "
                 f"--candidate {cand}`.")
    text = "\n".join(lines) + "\n"
    (out / "report.md").write_text(text)
    return code, text


def measure(args):
    pin = pinned_channel()
    cand = args.candidate
    if args.install:
        install(cand)
    pin_ver, cand_ver = rustc_version(pin), rustc_version(cand)
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    if pin_ver is None or cand_ver is None:
        msg = (f"## Next-stable advisory (issue 1642)\n\n**NO VERDICT** — cannot run "
               f"`{pin if pin_ver is None else cand}` via rustup. Install it as a named "
               "toolchain (`--install`).\n")
        (out / "report.md").write_text(msg)
        print(msg)
        return 2
    if pin_ver == cand_ver:
        msg = (f"## Next-stable advisory (issue 1642)\n\n**FREE — the pin IS the current "
               f"`{cand}`** (rustc {pin_ver}). Nothing to bump; no gate was run.\n")
        (out / "report.md").write_text(msg)
        print(msg)
        return 0

    gates = args.gates.split(",") if args.gates else [g for g, _ in GATES]
    rows, new_sites = [], {}
    for gate in gates:
        print(f"next-stable: {gate} under {pin} ...", flush=True)
        prc, plog, ps = run_gate(gate, pin, out)
        print(f"next-stable: {gate} under {cand} ...", flush=True)
        crc, clog, cs = run_gate(gate, cand, out)
        base = parse(plog.read_text(errors="replace"), gate)
        cand_sites = parse(clog.read_text(errors="replace"), gate)
        new = {k: v for k, v in cand_sites.items() if k not in base}
        new_sites.update(new)
        rows.append(dict(gate=gate, pin_rc=prc, cand_rc=crc, pin_s=ps, cand_s=cs,
                         new=len(new), cand_log=clog))
    code, text = report(pin, pin_ver, cand, cand_ver, rows, new_sites, out)
    print(text)
    return code


# ---------------------------------------------------------------- selftest --

def _workflow_violations(wf_text):
    """Static guard: the advisory lane cannot reach the merge path."""
    bad = []
    on = re.search(r"^on:\s*\n((?:[ \t]+.*\n|\s*\n)+)", wf_text, re.M)
    events = set(re.findall(r"^  ([a-z_]+):", on.group(1), re.M)) if on else set()
    if not on:
        bad.append("no block-style `on:` found — cannot verify the triggers")
    for ev in sorted(events & {"pull_request", "pull_request_target", "merge_group",
                               "push", "workflow_run"}):
        bad.append(f"triggered on `{ev}` — an advisory lane on an unpinned toolchain "
                   "must run on schedule/workflow_dispatch only (issue 1447)")
    if "schedule" not in events:
        bad.append("no `schedule:` trigger — a lane nobody triggers reports nothing")
    for name in re.findall(r"^\s+name:\s*['\"]?([^'\"\n]+?)['\"]?\s*$", wf_text, re.M):
        if name == REQUIRED_CONTEXT:
            bad.append(f"a job/step is named `{REQUIRED_CONTEXT}` — the required context is "
                       "matched by NAME, so this would report into it")
    return bad


def _gates_still_run(gate_text):
    bad = []
    for gate, step in GATES:
        if not re.search(rf"^\s+just check {re.escape(step)}\s*$", gate_text, re.M):
            bad.append(f"`{gate}` is measured via gate.yml step `just check {step}`, "
                       "which gate.yml no longer runs — the list has drifted")
    return bad


def selftest():
    fails = []

    def chk(what, ok):
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")
        if not ok:
            fails.append(what)

    clippy_log = f"""\
\x1b[1merror\x1b[0m: the borrowed expression implements the required traits
    --> {ROOT}/packages/core/nros-node/src/executor/spin.rs:9112:27
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#needless_borrows_for_generic_args
     = note: `-D clippy::needless-borrows-for-generic-args` implied by `-D warnings`
error: could not compile `nros-node` (lib test) due to 1 previous error
"""
    rustdoc_log = """\
error: redundant explicit link target
 --> packages/api/nros/src/lib.rs:166:9
  |
  = note: `-D rustdoc::redundant-explicit-links` implied by `-D warnings`
error: redundant explicit link target
 --> packages/api/nros/src/node_runtime.rs:4:5
error: could not document `nros`
"""
    rustc_log = """\
error: trailing semicolon in macro used in expression position
   --> packages/cli/cargo-nano-ros/src/scaffold.rs:157:9
    |
    = note: `#[deny(semicolon_in_expressions_from_non_local_macros)]` on by default
warning: `cargo-nano-ros` (lib) generated 1 warning
"""
    s = parse(clippy_log, "test-targets")
    chk("ANSI-coloured clippy header is a site, located repo-relative",
        list(s) == [("test-targets", "packages/core/nros-node/src/executor/spin.rs:9112:27",
                     "the borrowed expression implements the required traits")])
    chk("clippy lint named from the per-site help URL",
        list(s.values()) == ["clippy::needless_borrows_for_generic_args"])
    s = parse(rustdoc_log, "rustdoc-links")
    chk("rustdoc: two sites, summary line ignored", len(s) == 2)
    chk("rustdoc sites classify as rustdoc even without a lint note",
        {tool_of(g, v) for (g, _, _), v in s.items()} == {"rustdoc"})
    s = parse(rustc_log, "test-targets")
    chk("rustc future-incompat site classifies as rustc",
        [tool_of(g, v) for (g, _, _), v in s.items()] == ["rustc"])
    chk("a summary-only log yields no sites",
        parse("error: could not compile `x` due to 2 previous errors\n", "g") == {})
    reg = normalise("/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/eyre-0.6.12/src/lib.rs:1:1")
    chk("registry paths normalise across hosts", reg == "<registry>/eyre-0.6.12/src/lib.rs:1:1")

    good = ("on:\n  schedule:\n    - cron: '0 3 * * 1'\n  workflow_dispatch:\n\n"
            "jobs:\n  next-stable:\n    name: next-stable (advisory)\n")
    chk("guard: schedule + dispatch, distinct name -> clean",
        _workflow_violations(good) == [])
    chk("guard: refuses pull_request",
        any("pull_request" in v for v in _workflow_violations(
            good.replace("  workflow_dispatch:", "  pull_request:"))))
    chk("guard: refuses merge_group",
        any("merge_group" in v for v in _workflow_violations(
            good.replace("  workflow_dispatch:", "  merge_group:"))))
    chk("guard: refuses a job named CI",
        any("named `CI`" in v for v in _workflow_violations(
            good.replace("next-stable (advisory)", "CI"))))
    chk("guard: refuses a missing schedule",
        any("schedule" in v for v in _workflow_violations(
            good.replace("  schedule:\n    - cron: '0 3 * * 1'\n", ""))))

    # The real tree.
    if WORKFLOW.exists():
        v = _workflow_violations(WORKFLOW.read_text())
        for x in v:
            print(f"       {WORKFLOW.name}: {x}")
        chk(f"{WORKFLOW.name} cannot reach the merge path", v == [])
    else:
        chk(f"{WORKFLOW.name} exists", False)
    v = _gates_still_run(GATE_WORKFLOW.read_text())
    for x in v:
        print(f"       {x}")
    chk("every measured gate is still run by gate.yml", v == [])
    chk("pin is a version, not a channel (issue 1447)",
        re.fullmatch(r"\d+\.\d+\.\d+", pinned_channel()) is not None)

    if fails:
        print(f"next-stable-delta selftest: {len(fails)} failure(s)")
        return 1
    print("next-stable-delta selftest: OK")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--candidate", default="stable",
                    help="toolchain to try against the pin (default: stable)")
    ap.add_argument("--install", action="store_true",
                    help="rustup-install the candidate as a NAMED toolchain first")
    ap.add_argument("--out", default=str(ROOT / "tmp" / "next-stable"),
                    help="directory for logs + report.md")
    ap.add_argument("--gates", help="comma-separated override of the gate list")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    return measure(args)


if __name__ == "__main__":
    sys.exit(main())
