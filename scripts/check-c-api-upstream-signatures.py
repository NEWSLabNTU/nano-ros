#!/usr/bin/env python3
"""phase-492 W1.b — a C name upstream also has must have upstream's contract.

RFC-0089 rule 2 (2026-10-12): when upstream has the name and the contract does
not conform, the row "never takes the upstream name with a different
contract". `scripts/api-parity.py` compares NAMES, so a spelling match was
taken as conformance: the W1 rename map adopted sixteen `systematic` rows onto
upstream names with a different arity or callback contract
(`rcl_publisher_fini(pub)` against upstream's `(pub, node)`, `rcl_timer_init`
whose callback receives our context where rcl's receives the time since its
last call). This gate compares SIGNATURES.

Upstream is ROS 2 humble's rcl, rcl_action, rcl_lifecycle, rclc,
rclc_lifecycle and rclc_parameter. No lane has those headers installed, so
their prototypes are a committed snapshot,
`docs/reference/upstream-c-signatures.tsv`, regenerated with
`--snapshot <dir holding ros2/rcl and ros2/rclc checkouts at humble>`.

For every function and callback typedef the public `nros-c` headers declare
under a name the snapshot has:

* a FUNCTION conforms when an upstream call site compiles against ours
  unchanged: same return type, same arity, and each parameter the same type or
  ours adding `const` to the pointee (top-level `const` on a by-value parameter
  is not part of the caller's contract);
* a CALLBACK typedef conforms only when identical: we call the user's
  function, so the user's signature is the contract.

A name that does not conform must either take a distinct `rclc_` spelling or
be listed in `.config/c-api-upstream-signature-deviations.txt` with the ruling
that allows it (a ledger row or an RFC section). The list is a ratchet in both
directions: an entry that now conforms, or that no longer exists, fails too,
so a ruling cannot outlive the deviation it excused.

Run:  python3 scripts/check-c-api-upstream-signatures.py
      python3 scripts/check-c-api-upstream-signatures.py --snapshot <upstream>
Gate: just check c-api-upstream-signatures
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOT = ROOT / "docs" / "reference" / "upstream-c-signatures.tsv"
BASELINE = ROOT / ".config" / "c-api-upstream-signature-deviations.txt"
OUR_HEADERS = ROOT / "packages" / "api" / "nros-c" / "include"
UPSTREAM_PACKAGES = ("rcl", "rcl_action", "rcl_lifecycle", "rclc", "rclc_lifecycle", "rclc_parameter")

COMMENT = re.compile(r"/\*.*?\*/|//[^\n]*", re.S)
PROTO = re.compile(
    r"([A-Za-z_][\w\s\*]*?)\b([a-z_][a-z0-9_]*)\s*\(([^;{}()]*(?:\([^)]*\)[^;{}()]*)*)\)\s*;", re.S
)
FNPTR = re.compile(
    r"typedef\s+([\w\s\*]+?)\(\s*\*\s*([a-z_][a-z0-9_]*)\s*\)\s*\(([^;]*?)\)\s*;", re.S
)
NOISE = {
    "RCL_PUBLIC", "RCL_WARN_UNUSED", "RCLC_PUBLIC", "RCL_ACTION_PUBLIC", "RCL_LIFECYCLE_PUBLIC",
    "RCLC_LIFECYCLE_PUBLIC", "RCLC_PARAMETER_PUBLIC", "RCUTILS_WARN_UNUSED",
    "RCLC_PARAMETER_WARN_UNUSED", "NROS_PUBLIC", "extern", "struct", "enum",
}
# Upstream spells some types through rcutils/rmw typedefs whose rcl name is the
# one a caller writes; compare under the rcl name.
ALIAS = {
    "rcutils_allocator_t": "rcl_allocator_t",
    "rcutils_ret_t": "rcl_ret_t",
    "rmw_ret_t": "rcl_ret_t",
    "rcl_time_point_value_t": "int64_t",
    "rcl_duration_value_t": "int64_t",
    # RFC-0089 (2026-10-12): `ROSIDL_GET_*_TYPE_SUPPORT` is a pass-through that
    # yields OUR descriptor, and the rosidl struct stays refused (its members
    # are its contract). So the parameter upstream types as a rosidl
    # typesupport is, by ruling, our descriptor: a ported call site compiles
    # unchanged because the macro produces it.
    "rosidl_message_type_support_t": "rclc_message_type_t",
    "rosidl_service_type_support_t": "rclc_service_type_t",
    "rosidl_action_type_support_t": "rclc_action_type_t",
}
KEYWORDS = {"if", "while", "return", "sizeof", "for", "switch"}


def norm_type(t: str) -> str:
    t = re.sub(r"\[[^\]]*\]", "*", t)
    toks = [ALIAS.get(x, x) for x in re.findall(r"[A-Za-z_]\w*|\*", t) if x not in NOISE]
    # `char const *` and `const char *` are one type: move a `const` that
    # follows the base type in front of it.
    if len(toks) >= 2 and toks[1] == "const" and toks[0] != "*":
        toks[0], toks[1] = toks[1], toks[0]
    return " ".join(toks)


def split_params(p: str) -> list[str]:
    p = p.strip()
    if p in ("", "void"):
        return []
    parts, depth, cur = [], 0, ""
    for ch in p:
        depth += ch == "("
        depth -= ch == ")"
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    parts.append(cur)
    out = []
    for a in (x.strip() for x in parts):
        m = re.match(r"(.*?)([A-Za-z_]\w*)\s*(\[[^\]]*\])?$", a)
        ty = m.group(1) + (m.group(3) or "") if m and m.group(1).strip() else a
        out.append(norm_type(ty))
    return out


def harvest(files) -> dict[tuple[str, str], tuple[str, list[str]]]:
    """{(kind, name): (return type, [param types])}, kind `fn` or `cb`."""
    out: dict[tuple[str, str], tuple[str, list[str]]] = {}
    for f in files:
        text = COMMENT.sub(" ", f.read_text(errors="replace"))
        text = re.sub(r"^\s*#.*$", " ", text, flags=re.M)
        for m in FNPTR.finditer(text):
            out.setdefault(("cb", m.group(2)), (norm_type(m.group(1)), split_params(m.group(3))))
        for m in PROTO.finditer(text):
            ret, name, params = m.groups()
            # `return f(...);` inside a `static inline` forwarder is a CALL.
            if name in KEYWORDS or "typedef" in ret or "(" in ret or "return" in ret.split():
                continue
            out.setdefault(("fn", name), (norm_type(ret), split_params(params)))
    return out


def strip_top_const(t: str) -> str:
    return t[len("const "):] if "*" not in t and t.startswith("const ") else t


def conforms(kind: str, up: tuple[str, list[str]], ours: tuple[str, list[str]]) -> bool:
    (ur, up_params), (orr, our_params) = up, ours
    if ur != orr or len(up_params) != len(our_params):
        return False
    for u, o in zip(up_params, our_params):
        u, o = strip_top_const(u), strip_top_const(o)
        if u == o or (kind == "fn" and o == "const " + u):
            continue
        return False
    return True


def render(sig: tuple[str, list[str]]) -> str:
    return f"{sig[0]} ({', '.join(sig[1])})"


def write_snapshot(upstream: Path) -> int:
    files = [
        p
        for pkg in UPSTREAM_PACKAGES
        for p in sorted(upstream.rglob(f"{pkg}/include/**/*.h"))
    ]
    if not files:
        print(f"check-c-api-upstream-signatures: no {UPSTREAM_PACKAGES} headers under {upstream}",
              file=sys.stderr)
        return 1
    sigs = harvest(files)
    lines = [
        "# phase-492 W1.b — ROS 2 humble C prototypes, generated by",
        "# scripts/check-c-api-upstream-signatures.py --snapshot; do not hand-edit.",
        "# kind\tname\treturn\tparams (`;`-separated)",
    ]
    for (kind, name), (ret, params) in sorted(sigs.items()):
        lines.append(f"{kind}\t{name}\t{ret}\t{';'.join(params)}")
    SNAPSHOT.write_text("\n".join(lines) + "\n")
    print(f"check-c-api-upstream-signatures: wrote {len(sigs)} upstream prototypes to "
          f"{SNAPSHOT.relative_to(ROOT)}")
    return 0


def read_snapshot() -> dict[tuple[str, str], tuple[str, list[str]]]:
    out = {}
    for line in SNAPSHOT.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        kind, name, ret, params = line.split("\t")
        out[(kind, name)] = (ret, params.split(";") if params else [])
    return out


def read_baseline() -> dict[tuple[str, str], str]:
    out = {}
    if not BASELINE.exists():
        return out
    for line in BASELINE.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        kind, name, why = line.split("\t", 2)
        out[(kind, name)] = why
    return out


def check(upstream, ours, baseline) -> list[str]:
    errors = []
    deviating = set()
    for key in sorted(set(upstream) & set(ours)):
        if conforms(key[0], upstream[key], ours[key]):
            continue
        deviating.add(key)
        if key not in baseline:
            errors.append(
                f"{key[1]} ({'callback type' if key[0] == 'cb' else 'function'}) takes upstream's "
                f"name with a different contract:\n      upstream: {render(upstream[key])}\n"
                f"      ours:     {render(ours[key])}"
            )
    for key, why in sorted(baseline.items()):
        if key not in deviating:
            errors.append(f"{key[1]}: listed as a ruled deviation but it conforms or no longer "
                          f"exists — delete its line ({why})")
    return errors


def self_test() -> bool:
    up = {("fn", "f"): ("int", ["T *"]), ("fn", "g"): ("int", ["const uint64_t"]),
          ("cb", "cb_t"): ("void", ["const void *"])}
    ours = {("fn", "f"): ("int", ["const T *"]), ("fn", "g"): ("int", ["uint64_t"]),
            ("cb", "cb_t"): ("void", ["const uint8_t *", "size_t"])}
    errs = check(up, ours, {})
    ok = len(errs) == 1 and errs[0].startswith("cb_t")
    stale = check(up, {("fn", "f"): ours[("fn", "f")]}, {("fn", "f"): "x"})
    ok = ok and len(stale) == 1 and "conforms or no longer exists" in stale[0]
    if not ok:
        print(f"check-c-api-upstream-signatures: SELF-TEST FAILED: {errs} / {stale}", file=sys.stderr)
    return ok


def main(argv) -> int:
    if "--snapshot" in argv:
        return write_snapshot(Path(argv[argv.index("--snapshot") + 1]))
    if not self_test():
        return 1
    upstream = read_snapshot()
    ours = harvest(sorted(OUR_HEADERS.rglob("*.h")))
    shared = set(upstream) & set(ours)
    if len(shared) < 50:
        print(f"check-c-api-upstream-signatures: only {len(shared)} shared names — the harvest "
              f"broke; refusing to pass on a vacuous comparison", file=sys.stderr)
        return 1
    errors = check(upstream, ours, read_baseline())
    if errors:
        print("check-c-api-upstream-signatures: FAIL — RFC-0089 rule 2: a name upstream also has "
              "takes upstream's contract, or a distinct rclc_ name\n  " + "\n  ".join(errors),
              file=sys.stderr)
        return 1
    print(f"check-c-api-upstream-signatures: OK — {len(shared)} name(s) shared with upstream; "
          f"every one conforms or carries a ruling")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
