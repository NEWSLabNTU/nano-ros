#!/usr/bin/env python3
"""A backend may only advertise a QoS policy its code honours — phase-428 W9.

WHAT WENT WRONG

`Session::supported_qos_policies` returns a bitmask, and until W9 every one of
them was a hand-written constant sitting several files away from any code that
implemented anything. So the mask said whatever it had said when it was
written, and drifted the only way an unbound constant can: toward claiming
more.

Measured over the tree on 2026-09-11:

* `LIVELINESS_MANUAL_BY_NODE` was advertised by the zenoh shim AND by the cffi
  route, and implemented by nobody. The zenoh publisher matches
  `ManualByTopic | ManualByNode` together, so a per-node request got per-topic
  assertion; `nros_rmw_cyclonedds`'s `qos.cpp` folds the value onto
  MANUAL_BY_TOPIC in as many words; xrce lowers no liveliness field at all.
  The lie is in the losing direction — an application that correctly asserted
  its node's liveliness watches its other publishers expire.
* The zenoh shim's four CORE policies had exactly one read between them, in
  `QosKeyExpr::to_qos_string`, which builds a DISCOVERY keyexpr. Reliability,
  durability, history and depth changed no local behaviour whatever; the
  comment above the mask said all four were "honoured at the subscriber buffer
  level".

Neither is findable by reading a mask. Both are obvious once the question is
"which line of code does this?".

WHAT THIS CHECKS

A bit may be advertised only if the backend carries a `nros-qos-honours:
<BIT>` claim, sited on the code that honours the policy, in a file that READS
the mapped profile field. Honouring means applying the request or refusing a
value the backend cannot serve — both read the field, which is what makes the
rule mechanical. Passing the field to someone else does not: a file marked
`nros-qos-discovery-only:` contributes no evidence, because that was exactly
the shape of the second finding above.

WHY THE SUBJECT LISTS ARE ALL DERIVED

This campaign's recurring failure is a gate whose own list of things to check
is authored: the RMW parity map read green for 28 slots that had moved, and
the layout gate checked three type names out of ninety. So nothing here is
typed twice:

* the POLICY VOCABULARY is read from `QoSPolicyMask`'s `pub const` block;
* what each bit MEANS — which field, which variant — is read from
  `QoSProfile::required_policies`, the function that decides when a caller has
  asked for it, and a bit that function can never set is an error here;
* the BACKEND LIST is every `impl Session for` in `packages/**/src`, so a new
  backend is in scope the moment it exists;
* a multiplexing session (the cffi route) names the crates it routes to and
  its union is checked against THEIR claims.

An implementation that honours nothing says so with `nros-qos-exempt:` and a
reason — the mock and the metadata recorder, neither of which carries traffic.

THE NEGATIVE CONTROL IS THE LIVE TREE

Synthetic inputs prove the parser reads Rust. They do not prove the gate would
notice a real regression, because a synthetic file is written by the same hand
that wrote the matcher. So the controls below MUTATE THE REAL SOURCES in
memory — delete the depth read from the zenoh admission function, delete a
claim, move a claim into the discovery-only file, re-advertise the withdrawn
liveliness bit — and require a red each time. They run on the normal path
(`check-gate-selftests`), so "someone once demonstrated a red" stays a
measurement.

Usage::

    check-qos-mask-derivation.py             # the gate (+ its selftest)
    check-qos-mask-derivation.py --audit     # per backend, per bit, never fails
    check-qos-mask-derivation.py --verbose-selftest
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRAITS = ROOT / "packages" / "core" / "nros-rmw" / "src" / "traits.rs"

SOURCE_SUFFIXES = (".rs", ".c", ".cc", ".cpp", ".h", ".hpp")

CLAIM = re.compile(r"nros-qos-honours:\s*([A-Z][A-Z0-9_]*)")
EXEMPT = re.compile(r"nros-qos-exempt:\s*(\S.*)")
MUX = re.compile(r"nros-qos-mux:\s*(\S.*)")
DISCOVERY_ONLY = re.compile(r"nros-qos-discovery-only:")

IMPL_SESSION = re.compile(r"^\s*impl\s+(?:[\w:]+::)?Session\s+for\s+(\w+)", re.M)


class Fail(Exception):
    pass


# --------------------------------------------------------------------------
# Reading the definitions out of traits.rs
# --------------------------------------------------------------------------


def parse_bits(traits: str) -> tuple[dict[str, int], dict[str, set[str]]]:
    """`pub const NAME: Self = ...` inside `impl QoSPolicyMask`.

    Returns (atomic bits -> value, alias -> the atomic names it unions).
    Split by SHAPE, not by a list of known names: `Self(1 << n)` is a policy,
    `Self(A.0 | B.0)` is an alias for a set of them, `Self(0)` is the empty
    mask. A fourth shape would be reported rather than guessed at.
    """
    block = _impl_block(traits, "impl QoSPolicyMask {")
    if block is None:
        raise Fail("traits.rs: no `impl QoSPolicyMask` block — the vocabulary has moved")

    atomic: dict[str, int] = {}
    alias: dict[str, set[str]] = {}
    for m in re.finditer(r"pub const ([A-Z][A-Z0-9_]*)\s*:\s*Self\s*=\s*([^;]+);", block):
        name, rhs = m.group(1), " ".join(m.group(2).split())
        shift = re.fullmatch(r"Self\(1\s*<<\s*(\d+)\)", rhs)
        if shift:
            atomic[name] = 1 << int(shift.group(1))
            continue
        if re.fullmatch(r"Self\(0\)", rhs):
            alias[name] = set()
            continue
        parts = re.findall(r"Self::([A-Z][A-Z0-9_]*)\.0", rhs)
        if parts and re.fullmatch(r"Self\(\s*Self::[A-Z0-9_]+\.0(\s*\|\s*Self::[A-Z0-9_]+\.0)*\s*\)", rhs):
            alias[name] = set(parts)
            continue
        raise Fail(
            f"traits.rs: `QoSPolicyMask::{name}` has a shape this gate cannot read ({rhs!r}).\n"
            "  Bits are `Self(1 << n)`, aliases are a union of `Self::X.0`, empty is `Self(0)`."
        )
    if not atomic:
        raise Fail("traits.rs: `impl QoSPolicyMask` declares no `Self(1 << n)` policy bits")
    return atomic, alias


def _impl_block(text: str, header: str) -> str | None:
    """The braced body following `header`, by brace matching."""
    i = text.find(header)
    if i < 0:
        return None
    i = text.index("{", i)
    depth, j = 0, i
    while j < len(text):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i + 1 : j]
        j += 1
    return None


def parse_policy_fields(traits: str) -> dict[str, tuple[str, str | None]]:
    """bit -> (profile field, enum variant or None), from `required_policies`.

    That function is the definition of what a bit MEANS: it is what decides,
    for a given profile, whether the caller asked for the policy. Reading the
    mapping out of it rather than restating it here is the point — a policy
    whose meaning moves moves this gate's expectations with it.
    """
    body = _fn_body(traits, "pub fn required_policies(&self) -> QoSPolicyMask {")
    if body is None:
        raise Fail("traits.rs: `required_policies` not found — the bit->field mapping has moved")

    out: dict[str, tuple[str, str | None]] = {}
    field: str | None = None
    variant: str | None = None
    for line in body.splitlines():
        stripped = line.strip()
        # A new `if`/`match` on a profile field rebinds the subject.
        subject = re.search(r"\b(?:if|match)\b[^/]*?\bself\.(\w+)", stripped)
        if subject:
            field = subject.group(1)
            variant = None
        # An arm label may be a path (`QoSDurabilityPolicy::Volatile =>`) or a
        # bare variant, and an or-pattern names several
        # (`None | Automatic | ManualByTopic => {}`) — those set no bit, so the
        # last name is as good as any.
        arm = re.match(r"([\w:|\s]+?)\s*=>", stripped)
        if arm:
            variant = arm.group(1).split("::")[-1].split("|")[-1].strip()
        for bit in re.findall(r"mask \|= QoSPolicyMask::([A-Z][A-Z0-9_]*)", stripped):
            if field is None:
                raise Fail(
                    f"required_policies: `{bit}` is set with no `self.<field>` subject in scope"
                )
            out[bit] = (field, variant)
        if stripped.endswith(",") or stripped == "}":
            # An arm ends; the next `mask |=` without its own arm belongs to
            # the enclosing `if`, not to this variant.
            if arm is None and variant is not None and stripped.startswith("}"):
                variant = None
    return out


def _fn_body(text: str, signature: str) -> str | None:
    i = text.find(signature)
    if i < 0:
        return None
    return _impl_block(text[i:], signature)


# --------------------------------------------------------------------------
# The tree
# --------------------------------------------------------------------------


def load_tree() -> dict[str, str]:
    """Every tracked source file under a `packages/**/src/`.

    `git ls-files`, not a walk: an index lookup skips the build output and the
    generated trees for free, and `check-no-tracked-file-find` is right that a
    walk stats every directory it considers pruning.
    """
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--", "packages"],
        capture_output=True,
    )
    if out.returncode != 0:
        raise Fail(
            "`git ls-files` failed, so the backend list would be derived from nothing. "
            "A gate that examines no files reports a pass it never established."
        )
    files: dict[str, str] = {}
    for rel in out.stdout.decode("utf8", "replace").split("\0"):
        if not rel or "/src/" not in rel or not rel.endswith(SOURCE_SUFFIXES):
            continue
        if "/generated/" in rel:
            continue
        try:
            files[rel] = (ROOT / rel).read_text(encoding="utf8", errors="replace")
        except OSError:
            continue
    if not files:
        raise Fail("no tracked sources under packages/**/src — the file scan is broken")
    return files


def crate_root(rel: str) -> str:
    """The directory holding the `src/` this file lives under."""
    parts = rel.split("/")
    for i in range(len(parts) - 1, -1, -1):
        if parts[i] == "src":
            return "/".join(parts[:i])
    return os.path.dirname(rel)


class Backend:
    def __init__(self, file: str, type_name: str):
        self.file = file
        self.type_name = type_name
        self.crate = crate_root(file)
        self.bits: set[str] | None = None  # None = the trait default
        self.all_bits = False  # `QoSPolicyMask(u32::MAX)`
        self.exempt: str | None = None
        self.mux: list[str] = []

    def scopes(self) -> list[str]:
        return [self.crate] + self.mux

    def __repr__(self):
        return f"<{self.type_name} in {self.crate}>"


def find_backends(files: dict[str, str], atomic, alias) -> list[Backend]:
    out: list[Backend] = []
    for rel, text in sorted(files.items()):
        if not rel.endswith(".rs"):
            continue
        for m in IMPL_SESSION.finditer(text):
            block = _impl_block(text[m.start() :], m.group(0))
            if block is None:
                continue
            be = Backend(rel, m.group(1))
            _read_mask(be, block, atomic, alias)
            out.append(be)
    return out


def _read_mask(be: Backend, block: str, atomic, alias) -> None:
    sig = "fn supported_qos_policies(&self) -> "
    i = block.find(sig)
    if i < 0:
        return  # inherits the trait default
    # The doc comment above the fn carries the exempt / mux declarations.
    head = block[:i]
    doc = head[head.rfind("\n\n") :] if "\n\n" in head else head
    ex = EXEMPT.search(doc)
    if ex:
        be.exempt = ex.group(1).strip()
    mx = MUX.search(doc)
    if mx:
        be.mux = [d.strip().rstrip("/") for d in mx.group(1).split() if d.strip()]

    body = _impl_block(block[i:], sig)
    if body is None:
        raise Fail(f"{be.type_name}: could not read the body of supported_qos_policies")
    if re.search(r"QoSPolicyMask\(\s*u32::MAX\s*\)", body):
        be.all_bits = True
        be.bits = set(atomic)
        return
    names = re.findall(r"QoSPolicyMask::([A-Z][A-Z0-9_]*)", body)
    if not names:
        if re.search(r"QoSPolicyMask\(\s*0\s*\)", body):
            be.bits = set()
            return
        raise Fail(
            f"{be.type_name} ({be.file}): supported_qos_policies returns an expression this\n"
            f"  gate cannot read. Write a union of `QoSPolicyMask::<BIT>` terms."
        )
    bits: set[str] = set()
    for n in names:
        if n in atomic:
            bits.add(n)
        elif n in alias:
            bits |= alias[n]
        else:
            raise Fail(f"{be.type_name}: `QoSPolicyMask::{n}` is not a declared policy or alias")
    be.bits = bits


def claims_in(files: dict[str, str], scopes: list[str]) -> dict[str, list[str]]:
    """bit -> the files claiming to honour it, within these scope dirs."""
    out: dict[str, list[str]] = {}
    for rel, text in files.items():
        if not any(rel == s or rel.startswith(s + "/") for s in scopes):
            continue
        for bit in CLAIM.findall(text):
            out.setdefault(bit, []).append(rel)
    return out


def c_spelling(variant: str) -> str:
    """`TransientLocal` -> `TRANSIENT_LOCAL`, so a C claim site is readable too."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", variant).upper()


def reads_field(text: str, field: str, variant: str | None) -> bool:
    if DISCOVERY_ONLY.search(text):
        return False
    if not re.search(r"(?:\.|->)\s*" + re.escape(field) + r"\b", text):
        return False
    if variant is None:
        return True
    # The C spelling is a SUFFIX of the ABI enumerator
    # (`NROS_RMW_DURABILITY_TRANSIENT_LOCAL`), so it gets no leading word
    # boundary — `_` is a word character, and requiring one here made every
    # C-backend claim read as unevidenced.
    return bool(
        re.search(r"\b" + re.escape(variant) + r"\b", text)
        or re.search(re.escape(c_spelling(variant)) + r"\b", text)
    )


# --------------------------------------------------------------------------
# The rules
# --------------------------------------------------------------------------


def check(files: dict[str, str]) -> tuple[list[str], list[Backend]]:
    traits = files.get(str(TRAITS.relative_to(ROOT)))
    if traits is None:
        raise Fail(f"{TRAITS.relative_to(ROOT)} not found")
    atomic, alias = parse_bits(traits)
    fields = parse_policy_fields(traits)
    backends = find_backends(files, atomic, alias)
    errs: list[str] = []

    # R1 — every declared bit is one a caller can actually request. A bit
    # `required_policies` never sets is a claim nothing can exercise.
    for bit in sorted(set(atomic) - set(fields)):
        errs.append(
            f"QoSPolicyMask::{bit} is declared but `required_policies` never sets it — "
            "no profile can request it, so no backend can honour it"
        )
    for bit in sorted(set(fields) - set(atomic)):
        errs.append(f"`required_policies` sets `{bit}`, which is not a declared policy bit")

    if not backends:
        errs.append("no `impl Session for` found under packages/**/src — the scan is broken")

    claimed_anywhere: dict[str, set[str]] = {}
    for be in backends:
        if be.bits is None:
            continue  # trait default: NONE, nothing to justify
        if be.exempt:
            if not be.all_bits and be.bits != set(atomic):
                errs.append(
                    f"{be.type_name} ({be.file}) declares nros-qos-exempt but advertises a "
                    "SUBSET of the policies. An exemption says the session carries no traffic, "
                    "so it can only be all or nothing — state a mask and drop the exemption."
                )
            continue
        if be.all_bits:
            errs.append(
                f"{be.type_name} ({be.file}) advertises every policy via "
                "`QoSPolicyMask(u32::MAX)` with no nros-qos-exempt reason. A blanket claim is "
                "the failure this gate exists to catch; say why the session carries no traffic."
            )
            continue

        claims = claims_in(files, be.scopes())
        for bit in sorted(be.bits):
            field, variant = fields.get(bit, (None, None))
            sites = claims.get(bit, [])
            if not sites:
                errs.append(
                    f"{be.type_name} ({be.file}) advertises {bit} with no "
                    f"`nros-qos-honours: {bit}` claim under {', '.join(be.scopes())}"
                )
                continue
            if field is None:
                continue  # already reported by R1
            good = [s for s in sites if reads_field(files[s], field, variant)]
            if not good:
                want = f"`qos.{field}`" + (f" and `{variant}`" if variant else "")
                errs.append(
                    f"{be.type_name} ({be.file}) advertises {bit}, and the claim(s) at "
                    f"{', '.join(sites)} read no {want}. A claim is sited on code that APPLIES "
                    "the policy or REFUSES a value it cannot serve; a file marked "
                    "nros-qos-discovery-only contributes nothing."
                )
        for bit, sites in claims.items():
            claimed_anywhere.setdefault(bit, set()).update(sites)

    # R5 — a claim for a bit nothing advertises. Stale exemptions read as
    # tracked debt while being inert (the issue-0743 class); a stale CLAIM
    # reads as an implemented policy while being unreachable.
    # An EXEMPT session's blanket mask is not a claim that anything is
    # honoured, so it must not launder a stale claim into looking live.
    advertised: set[str] = set()
    for be in backends:
        if be.bits and not be.exempt:
            advertised |= be.bits
    for rel, text in files.items():
        for bit in set(CLAIM.findall(text)):
            if bit not in atomic:
                errs.append(f"{rel}: `nros-qos-honours: {bit}` names no declared policy bit")
            elif bit not in advertised:
                errs.append(
                    f"{rel}: claims to honour {bit}, which no backend advertises. Either the "
                    "mask lost the bit and the claim is stale, or the mask should have it."
                )
    return errs, backends


# --------------------------------------------------------------------------
# Negative controls — synthetic for the parser, LIVE for the rule
# --------------------------------------------------------------------------

SYNTH_TRAITS = """
impl QoSPolicyMask {
    pub const RELIABILITY: Self = Self(1 << 0);
    pub const DEPTH: Self = Self(1 << 1);
    pub const NONE: Self = Self(0);
    pub const CORE: Self = Self(Self::RELIABILITY.0 | Self::DEPTH.0);
}

impl QoSProfile {
    pub fn required_policies(&self) -> QoSPolicyMask {
        let mut mask = QoSPolicyMask(0);
        if self.reliability != QoSReliabilityPolicy::SystemDefault {
            mask |= QoSPolicyMask::RELIABILITY;
        }
        if self.depth != DEPTH_SYSTEM_DEFAULT {
            mask |= QoSPolicyMask::DEPTH;
        }
        mask
    }
}
"""


def _parser_self_test(verbose: bool) -> int:
    atomic, alias = parse_bits(SYNTH_TRAITS)
    if set(atomic) != {"RELIABILITY", "DEPTH"}:
        raise Fail(f"selftest: bit vocabulary misread: {sorted(atomic)}")
    if alias.get("CORE") != {"RELIABILITY", "DEPTH"} or alias.get("NONE") != set():
        raise Fail(f"selftest: aliases misread: {alias}")

    fields = parse_policy_fields(SYNTH_TRAITS)
    if fields != {"RELIABILITY": ("reliability", None), "DEPTH": ("depth", None)}:
        raise Fail(f"selftest: bit->field mapping misread: {fields}")

    # The LIVE mapping must cover every live bit, variants included. This is
    # the assertion that would have caught a policy gaining a bit and no arm.
    live = parse_policy_fields(TRAITS.read_text(encoding="utf8"))
    if live.get("DURABILITY_TRANSIENT_LOCAL") != ("durability", "TransientLocal"):
        raise Fail(f"selftest: variant arms misread: {live.get('DURABILITY_TRANSIENT_LOCAL')}")
    if live.get("LIVELINESS_MANUAL_BY_NODE") != ("liveliness_kind", "ManualByNode"):
        raise Fail(f"selftest: variant arms misread: {live.get('LIVELINESS_MANUAL_BY_NODE')}")
    if live.get("DEADLINE") != ("deadline_ms", None):
        raise Fail(f"selftest: `if` arms misread: {live.get('DEADLINE')}")

    if c_spelling("ManualByTopic") != "MANUAL_BY_TOPIC":
        raise Fail("selftest: C spelling of a variant is wrong")
    if verbose:
        print("  parser controls passed")
    return 6


def _mutation_self_test(files: dict[str, str], verbose: bool) -> int:
    """Break the REAL tree four ways and require a red each time.

    A synthetic backend proves nothing about whether this gate would notice a
    regression in the backend we ship, because the synthetic one is written to
    match the matcher. These are the shipped sources with one thing removed.
    """
    ran = 0

    def red(label: str, mutated: dict[str, str], needle: str) -> None:
        nonlocal ran
        ran += 1
        errs, _ = check(mutated)
        if not any(needle in e for e in errs):
            raise Fail(
                f"selftest mutation {label!r} produced no error mentioning {needle!r}.\n"
                f"  Got: {errs or '(clean)'}"
            )

    zenoh_qos = "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/qos.rs"
    zenoh_session = "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs"
    keyexpr = "packages/rmw/zenoh/nros-rmw-zenoh/src/keyexpr.rs"
    for required in (zenoh_qos, zenoh_session, keyexpr):
        if required not in files:
            raise Fail(
                f"selftest: {required} is missing, so the live mutations cannot run. "
                "A control that silently examines nothing is the vacuous-test class."
            )

    # 1. The backend stops READING the depth: the claim survives, the honouring
    #    does not. This is the regression the whole item is about.
    m = dict(files)
    m[zenoh_qos] = re.sub(r"(?:\.|->)\s*depth\b", ".NOT_DEPTH", m[zenoh_qos])
    red("zenoh stops reading qos.depth", m, "advertises DEPTH")

    # 2. The claim is deleted while the code stays. The bit loses its site.
    m = dict(files)
    for f in (zenoh_qos, zenoh_session):
        m[f] = m[f].replace("nros-qos-honours: HISTORY", "(claim deleted)")
    red("zenoh drops its HISTORY claim", m, "advertises HISTORY with no")

    # 3. A claim sited in the DISCOVERY serialiser buys nothing — the exact
    #    shape that made four policies look honoured before W9.
    m = dict(files)
    m[zenoh_qos] = m[zenoh_qos].replace("nros-qos-honours: RELIABILITY", "(moved)")
    m[keyexpr] = m[keyexpr].replace(
        "impl QosKeyExpr for QoSProfile {",
        "// nros-qos-honours: RELIABILITY\nimpl QosKeyExpr for QoSProfile {",
    )
    red("RELIABILITY claimed in the discovery keyexpr", m, "read no `qos.reliability`")

    # 4. Re-advertising the withdrawn liveliness bit, with no site anywhere.
    m = dict(files)
    m[zenoh_session] = m[zenoh_session].replace(
        "| QoSPolicyMask::LIVELINESS_LEASE",
        "| QoSPolicyMask::LIVELINESS_MANUAL_BY_NODE\n            | QoSPolicyMask::LIVELINESS_LEASE",
        1,
    )
    red("MANUAL_BY_NODE re-advertised", m, "advertises LIVELINESS_MANUAL_BY_NODE")

    # 5. A stale claim for a bit nobody advertises.
    m = dict(files)
    m[zenoh_qos] = m[zenoh_qos] + "\n// nros-qos-honours: LIVELINESS_MANUAL_BY_NODE\n"
    red("stale claim", m, "which no backend advertises")

    # 6. An exemption cannot be a subset claim.
    m = dict(files)
    m["packages/rmw/metadata/src/lib.rs"] = m["packages/rmw/metadata/src/lib.rs"].replace(
        "nros_rmw::QoSPolicyMask(u32::MAX)", "nros_rmw::QoSPolicyMask::CORE"
    )
    red("exempt session claiming a subset", m, "declares nros-qos-exempt but advertises a SUBSET")

    if verbose:
        print(f"  {ran} live-tree mutation controls passed")
    return ran


def self_test(files: dict[str, str], verbose: bool = False) -> None:
    n = _parser_self_test(verbose)
    n += _mutation_self_test(files, verbose)
    if verbose:
        print(f"  {n} controls total")


# --------------------------------------------------------------------------


def audit(files: dict[str, str]) -> int:
    traits = files[str(TRAITS.relative_to(ROOT))]
    atomic, alias = parse_bits(traits)
    fields = parse_policy_fields(traits)
    backends = find_backends(files, atomic, alias)
    print(f"{len(atomic)} policy bits, {len(backends)} Session impl(s)\n")
    for be in sorted(backends, key=lambda b: b.file):
        if be.bits is None:
            print(f"{be.type_name:24} {be.crate}\n    (trait default — advertises nothing)")
            continue
        if be.exempt:
            print(f"{be.type_name:24} {be.crate}\n    exempt: {be.exempt}")
            continue
        claims = claims_in(files, be.scopes())
        print(f"{be.type_name:24} {be.crate}")
        for bit in sorted(atomic):
            field, variant = fields.get(bit, (None, None))
            sites = [s for s in claims.get(bit, []) if reads_field(files[s], field, variant)]
            mark = "yes" if bit in be.bits else " no"
            where = ", ".join(sorted({os.path.basename(s) for s in sites})) if sites else "-"
            print(f"    {mark}  {bit:36} {where}")
        print()
    return 0


def main() -> int:
    verbose = any(a.startswith("--verbose-self") for a in sys.argv[1:])
    try:
        files = load_tree()
        if "--audit" in sys.argv:
            return audit(files)
        # On the NORMAL path — `check-gate-selftests` requires it, and a
        # control nobody runs decays into a comment.
        self_test(files, verbose=verbose)
        errs, backends = check(files)
    except Fail as exc:
        print(f"check-qos-mask-derivation: {exc}", file=sys.stderr)
        return 1

    if errs:
        print(f"check-qos-mask-derivation: {len(errs)} problem(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print(
            "\n  A backend advertises a QoS policy by SITING a `nros-qos-honours: <BIT>`\n"
            "  claim on the code that applies it or refuses a value it cannot serve.\n"
            "  `check-qos-mask-derivation.py --audit` prints the whole picture.",
            file=sys.stderr,
        )
        return 1

    n_claiming = sum(1 for b in backends if b.bits and not b.exempt)
    print(
        f"check-qos-mask-derivation: OK ({len(backends)} Session impl(s), "
        f"{n_claiming} advertising a derived mask)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
