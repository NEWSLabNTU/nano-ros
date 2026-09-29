#!/usr/bin/env python3
"""Issue 0656 — an entity declared with a LITERAL domain instead of the session's.

`ServiceInfo` / `TopicInfo` / `ActionInfo` default to domain 0, and the keyexpr
is `<domain_id>/<name>/…`. So a chain that omits `with_domain`, or hardcodes a
number, declares on domain 0 whatever `ROS_DOMAIN_ID` says. A peer that honours
the domain queries `42/…`, matches nothing, and every goal times out with no
diagnostic on either side.

It stayed invisible because BOTH peers in every existing action test took paths
that dropped the domain identically, so they agreed on `0/…` and passed. The
mismatch needs one side that honours the value — which is what the phase-354 W3
polling probe was.

WHAT THIS CHECKS

No `with_domain(<integer literal>)` in the runtime crates. The domain must come
from a binding (`self.domain_id`, `domain_id`, `config.domain_id`, …), because
that is the only spelling that can be wrong in a way the type system or a test
would notice.

It deliberately does NOT try to prove every chain HAS a `with_domain`: that
needs dataflow, and a half-working version would either miss chains or flag the
many legitimate `Info` values that never reach a `create_*`. A literal is the
mechanical half, and it is the half that was actually written — five times in
`action.rs` alone.
"""

import re
import subprocess
import sys
from pathlib import Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
import per_item  # noqa: E402  phase-472 W6 — per-item matching

ROOT = Path(__file__).resolve().parent.parent

# The runtime crates that declare entities. Bindings and examples are USER-ish
# and may legitimately pin a domain from their own config.
SCOPE = (
    "packages/core/nros-node",
    "packages/core/nros-rmw",
    "packages/api/nros-c",
    "packages/api/nros-cpp",
)

LITERAL_DOMAIN = re.compile(r"\.with_domain\(\s*(\d+)\s*[,)]")


def strip_comments(text):
    """Drop `//` comments, keeping newlines so line numbers stay true.

    A gate that flags the prose explaining its own history gets bypassed —
    issue 0555's checker records the same rule."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "rust")


def tracked_rs():
    args = ["git", "-C", str(ROOT), "ls-files", "--"]
    args.extend(f"{d}/**/*.rs" for d in SCOPE)
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout.split()
    # `traits.rs` DEFINES `with_domain` and its defaults. `tests/` and
    # `benches/` legitimately pin a domain — an integration test asserting
    # behaviour ON domain 42 must say 42, and that is the opposite of the
    # defect, which is SHIPPED code that cannot express the session's domain.
    return [
        f
        for f in out
        if not f.endswith("/traits.rs")
        and "/tests/" not in f
        and "/benches/" not in f
        and "/examples/" not in f
    ]


def offenders(files):
    hits = []
    for rel in files:
        try:
            raw = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        # `#[cfg(test)]` code may pin a domain to assert the formatting; the
        # defect is in what ships. Each cfg(test) ITEM is blanked — this used to
        # truncate the file at the first `mod tests` SUBSTRING, so a `mod tests;`
        # DECLARATION (executor/mod.rs) hid every shipped line after it
        # (phase-472 W6).
        code = per_item.rust_cfg_test_blank(strip_comments(raw))
        for m in LITERAL_DOMAIN.finditer(code):
            line = code[: m.start()].count("\n") + 1
            hits.append((rel, line, m.group(1)))
    return hits


def self_test():
    import tempfile

    per_item.self_test()  # the shared helper's own controls (phase-472 W6)

    tmp_root = ROOT / "tmp"
    tmp_root.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp_root) as d:
        probe = Path(d) / "probe.rs"
        rel = str(probe.relative_to(ROOT))

        probe.write_text("let i = ServiceInfo::new(a, b, c).with_domain(0);\n")
        if not offenders([rel]):
            sys.stderr.write("self-test: a literal domain was NOT reported\n")
            sys.exit(2)

        probe.write_text("let i = ServiceInfo::new(a, b, c).with_domain(self.domain_id);\n")
        if offenders([rel]):
            sys.stderr.write("self-test: a bound domain WAS reported\n")
            sys.exit(2)

        probe.write_text("// .with_domain(0) was the bug; see issue 0656.\nlet x = 1;\n")
        if offenders([rel]):
            sys.stderr.write("self-test: a COMMENT was reported\n")
            sys.exit(2)

        # phase-472 W6 — a `mod tests;` DECLARATION ends nothing: shipped code
        # after it is still read, and a cfg(test) BODY is still exempt.
        probe.write_text("#[cfg(test)]\nmod tests;\nfn ship(i: I) -> I { i.with_domain(0) }\n"
                         "#[cfg(test)]\nmod t { fn f() { x.with_domain(7) } }\n")
        got = offenders([rel])
        if [(line, v) for _r, line, v in got] != [(3, "0")]:
            sys.stderr.write(f"self-test: per-item cfg(test) blanking is wrong: {got}\n")
            sys.exit(2)


def main():
    self_test()
    files = tracked_rs()
    if not files:
        sys.stderr.write("[FAIL] no runtime sources scanned — this gate would pass vacuously.\n")
        return 1
    hits = offenders(files)
    if hits:
        sys.stderr.write("[FAIL] entity declared with a LITERAL domain (issue 0656):\n")
        for rel, line, val in hits:
            sys.stderr.write(f"         {rel}:{line}: with_domain({val})\n")
        sys.stderr.write(
            "\n       The keyexpr is `<domain_id>/<name>/…`, so this declares on that\n"
            "       domain whatever `ROS_DOMAIN_ID` says. A peer that honours the\n"
            "       value matches nothing and times out with no diagnostic on either\n"
            "       side. Pass the session's domain instead.\n"
        )
        return 1
    print(f"literal domain ids: OK ({len(files)} runtime source(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
