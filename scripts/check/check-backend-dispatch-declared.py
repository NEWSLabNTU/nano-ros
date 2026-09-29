#!/usr/bin/env python3
"""Every RMW backend crate DECLARES how it delivers a subscription — issue 1577.

## The fact, and why the build needs it

A sizing descriptor row saying `registration_path = "in_place"` prices a
subscription at NO receive region in the executor arena. That is true only on a
backend that hands the sample to the callback out of its own slot, and one
descriptor can serve builds that link another: a single-package leaf's fixture
rows switch backend by cargo FEATURE over one image, so its cyclonedds row
reads the descriptor its `system.toml` (zenoh) wrote. Honouring the row there
under-sizes the arena, and the image fails at registration with
`BufferTooSmall`.

So `nros-node/build.rs` asks the build, not the descriptor. Each backend crate
enables one feature on `nros-rmw`:

    in-place-dispatch   zenoh, XRCE
    buffered-dispatch   Cyclone, the metadata recorder

`nros-rmw/build.rs` publishes both through `links` metadata, and `nros-node`
honours an `in_place` row only when in-place is CLAIMED and buffered is not.
Silence answers "buffered": Cyclone and uORB under cmake reach cargo as a bare
`rmw-cffi`, with no crate to declare anything, and must price the full region.

## What this gate holds

1. **Every crate under `packages/rmw/` is classified** — `in_place`, `buffered`,
   or not a backend, with a reason. Both directions: a row naming no crate
   fails too. A new backend crate cannot land undeclared, which is the failure
   that would re-open 1577 silently — an undeclared in-place backend only loses
   the saving, but an undeclared buffering one beside an in-place one is the
   under-size.
2. **Each backend enables exactly its feature, unconditionally** — on a
   non-optional `nros-rmw` dependency. A declaration behind an optional dep or
   a crate feature is one a build can switch off while the backend is linked.
3. **Nothing else enables either feature.** A router or umbrella that declares
   in-place would claim it for every backend behind it; that is the whole
   reason the declaration lives on the backend.
4. **The producer's table agrees.** `nros-cli-core`'s `backend_dispatch()` is
   where the descriptor decides a row CAN be `in_place` (by the entry's `rmw`
   name). Each name it classifies must belong to a backend crate declaring the
   same dispatch here, and each named backend must be in its table.
5. **The three ends of the carrier exist** — the two features on `nros-rmw`,
   the two `links` keys its build script emits, and the two `DEP_NROS_RMW_*`
   variables `nros-node/build.rs` reads. A renamed key on one side leaves the
   reader seeing nothing, which fails SAFE and therefore silently.

Buildless: `git ls-files` + reads. Self-test on the normal path.

Run:  python3 scripts/check/check-backend-dispatch-declared.py [--self-test]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]

IN_PLACE = "in_place"
BUFFERED = "buffered"
FEATURE = {IN_PLACE: "in-place-dispatch", BUFFERED: "buffered-dispatch"}

# crate name -> (dispatch | None, rmw names the producer's table uses, reason)
#
# `rmw` names tie a row to `backend_dispatch()` in the CLI (assertion 4). A
# backend with none (the metadata recorder) is not selectable by an entry, so
# the producer never prices a row for it.
CLASSIFICATION: dict[str, tuple[str | None, tuple[str, ...], str]] = {
    "nros-rmw-zenoh": (
        IN_PLACE,
        ("zenoh",),
        "`Subscription::supports_process_in_place` is unconditionally `true`",
    ),
    "nros-rmw-xrce-cffi": (
        IN_PLACE,
        ("xrce",),
        "the C vtable's `subscription_supports_in_place` writes `true` and "
        "`process_raw_in_place` is non-NULL",
    ),
    "nros-rmw-cyclonedds": (
        BUFFERED,
        ("cyclonedds", "cyclone"),
        "the C vtable leaves both in-place slots NULL",
    ),
    "nros-rmw-metadata": (
        BUFFERED,
        (),
        "the probe recorder takes the trait default `supports_process_in_place() == false`",
    ),
    "nros-rmw-cffi": (
        None,
        (),
        "the ROUTER — every image reaches its backend through it",
    ),
    "nros-bridge": (None, (), "composes backends; each declares for itself"),
    "nros-transport-callbacks": (
        None,
        (),
        "custom-transport callback factories, no subscriptions",
    ),
    "nros-rmw-xrce-cffi-staticlib": (
        None,
        (),
        "staticlib wrapper over `nros-rmw-xrce-cffi`",
    ),
    "nros-rmw-zenoh-staticlib": (None, (), "staticlib wrapper over `nros-rmw-zenoh`"),
    "cyclonedds-sys": (
        None,
        (),
        "C library build; `nros-rmw-cyclonedds` is the backend",
    ),
    "nros-rmw-cyclonedds-sys": (
        None,
        (),
        "C shim build; `nros-rmw-cyclonedds` is the backend",
    ),
    "zpico-sys": (None, (), "zenoh-pico C build; `nros-rmw-zenoh` is the backend"),
    "nros-zpico-build": (None, (), "build-script helper"),
    "zpico-alloc": (None, (), "allocator glue"),
    "zpico-link-ivc": (None, (), "a zenoh-pico link, not a backend"),
    "zpico-serial": (None, (), "a zenoh-pico link, not a backend"),
    "zpico-platform-custom": (None, (), "platform shim for zenoh-pico"),
}

PRODUCER = "packages/cli/nros-cli-core/src/sizing_descriptor.rs"
RMW_MANIFEST = "packages/core/nros-rmw/Cargo.toml"
RMW_BUILD = "packages/core/nros-rmw/build.rs"
NODE_BUILD = "packages/core/nros-node/build.rs"


# --- readers -----------------------------------------------------------------


def dep_tables(doc: dict) -> list[dict]:
    """Normal dependency tables — the default one and every `[target.*]` one.

    Dev- and build-dependencies are deliberately excluded: a feature enabled
    there does not unify into the image.
    """
    out = [doc.get("dependencies", {})]
    for t in doc.get("target", {}).values():
        out.append(t.get("dependencies", {}))
    return out


def declared_dispatch(doc: dict) -> tuple[set[str], list[str]]:
    """({dispatch declared unconditionally}, [problems]) for one manifest."""
    declared: set[str] = set()
    problems: list[str] = []
    for table in dep_tables(doc):
        for key, spec in table.items():
            if not isinstance(spec, dict):
                continue
            if spec.get("package", key) != "nros-rmw":
                continue
            for d, feat in FEATURE.items():
                if feat in spec.get("features", []):
                    if spec.get("optional"):
                        problems.append(
                            f"enables `nros-rmw/{feat}` on an OPTIONAL dependency — a build "
                            "can drop the declaration while the backend is linked"
                        )
                    else:
                        declared.add(d)
    for name, items in doc.get("features", {}).items():
        for item in items:
            for feat in FEATURE.values():
                if item.endswith(f"/{feat}"):
                    problems.append(
                        f"enables `{item}` through crate feature `{name}` — the declaration "
                        "must hold whenever the backend is linked, so it belongs on the "
                        "dependency itself"
                    )
    return declared, problems


def producer_table(src: str) -> dict[str, str] | None:
    """`backend_dispatch()`'s arms as {rmw name: dispatch}, or None if unfound."""
    m = re.search(r"fn backend_dispatch\([^)]*\)[^{]*\{(.*?)\n\}", src, re.S)
    if not m:
        return None
    out: dict[str, str] = {}
    for arm in re.finditer(
        r"((?:\"[^\"]+\"\s*\|?\s*)+)=>\s*Some\(BackendDispatch::(\w+)\)", m.group(1)
    ):
        kind = {"InPlace": IN_PLACE, "Buffered": BUFFERED}.get(arm.group(2))
        if kind is None:
            return None
        for name in re.findall(r'"([^"]+)"', arm.group(1)):
            out[name] = kind
    return out or None


# --- the rule ----------------------------------------------------------------


def check(
    crates: dict[str, dict],
    enablers: dict[str, str],
    producer: dict[str, str] | None,
    rmw_manifest: dict,
    rmw_build: str,
    node_build: str,
    classification=CLASSIFICATION,
) -> list[str]:
    """Every violation, as text. `crates` is {name: manifest} for packages/rmw;
    `enablers` is {crate name: manifest path} for EVERY tracked manifest that
    mentions either feature, repo-wide."""
    errors: list[str] = []

    # 1 — classification, both directions.
    for name in sorted(set(crates) - set(classification)):
        errors.append(
            f"`{name}` is under packages/rmw/ and UNCLASSIFIED — add it to CLASSIFICATION "
            "as in_place, buffered, or not a backend (with the reason)"
        )
    for name in sorted(set(classification) - set(crates)):
        errors.append(
            f"CLASSIFICATION names `{name}`, which is no crate under packages/rmw/"
        )

    # 2 — each classified crate declares exactly its dispatch.
    for name in sorted(set(crates) & set(classification)):
        want = classification[name][0]
        got, problems = declared_dispatch(crates[name])
        errors.extend(f"`{name}` {p}" for p in problems)
        if want is None and got:
            errors.append(
                f"`{name}` is not a backend but declares {sorted(got)} — a non-backend "
                "declaring dispatch claims it for every backend behind it"
            )
        elif want is not None and got != {want}:
            errors.append(
                f"`{name}` is classified {want} but declares {sorted(got) or 'nothing'} — "
                f"its `nros-rmw` dependency must enable `{FEATURE[want]}` "
                f"({classification[name][2]})"
            )

    # 3 — nothing outside the backends enables either feature.
    backends = {n for n, (d, _, _) in classification.items() if d is not None}
    for name, path in sorted(enablers.items()):
        if name not in backends and name != "nros-rmw":
            errors.append(
                f"`{name}` ({path}) enables a dispatch feature and is not a classified "
                "backend — only the crate that IS the backend may declare it"
            )

    # 4 — the producer's table agrees with the declarations.
    if producer is None:
        errors.append(
            f"could not read `backend_dispatch()`'s arms in {PRODUCER} — the producer's "
            "in-place table has moved or changed shape; update this reader"
        )
    else:
        by_rmw = {r: (n, d) for n, (d, rs, _) in classification.items() for r in rs}
        for rmw, kind in sorted(producer.items()):
            if rmw not in by_rmw:
                errors.append(
                    f"the producer classifies rmw `{rmw}` as {kind}, and no backend crate "
                    "here claims that name"
                )
            elif by_rmw[rmw][1] != kind:
                errors.append(
                    f"the producer prices rmw `{rmw}` as {kind}, but `{by_rmw[rmw][0]}` "
                    f"declares {by_rmw[rmw][1]} — the descriptor and the build disagree"
                )
        for rmw, (name, _) in sorted(by_rmw.items()):
            if rmw not in producer:
                errors.append(
                    f"`{name}` claims rmw `{rmw}`, which the producer's `backend_dispatch()` "
                    "does not classify"
                )

    # 5 — the three ends of the carrier.
    feats = rmw_manifest.get("features", {})
    if rmw_manifest.get("package", {}).get("links") != "nros_rmw":
        errors.append(f'{RMW_MANIFEST} must declare `links = "nros_rmw"` (the carrier)')
    for d, feat in FEATURE.items():
        key = feat.replace("-", "_")
        if feat not in feats:
            errors.append(f"{RMW_MANIFEST} has no `{feat}` feature")
        if (
            f"CARGO_FEATURE_{key.upper()}" not in rmw_build
            or f'"{key}"' not in rmw_build
        ):
            errors.append(f"{RMW_BUILD} does not publish `{key}` from its feature")
        if f"DEP_NROS_RMW_{key.upper()}" not in node_build:
            errors.append(f"{NODE_BUILD} does not read `DEP_NROS_RMW_{key.upper()}`")
    return errors


# --- the tree ----------------------------------------------------------------


def git_ls(*pathspec: str) -> list[str]:
    return subprocess.run(
        ["git", "-C", str(REPO), "ls-files", *pathspec],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()


def load(rel: str) -> dict:
    with open(REPO / rel, "rb") as fh:
        return tomllib.load(fh)


def tree_inputs():
    crates: dict[str, dict] = {}
    for rel in git_ls("packages/rmw/**Cargo.toml", "packages/rmw/*/Cargo.toml"):
        doc = load(rel)
        name = doc.get("package", {}).get("name")
        if name:
            crates[name] = doc
    enablers: dict[str, str] = {}
    pat = re.compile(r"\b(in-place-dispatch|buffered-dispatch)\b")
    for rel in git_ls("*Cargo.toml"):
        text = (REPO / rel).read_text(encoding="utf-8", errors="replace")
        # A mention in a comment is prose, not a declaration.
        code = "\n".join(line.split("#", 1)[0] for line in text.splitlines())
        if pat.search(code):
            name = tomllib.loads(text).get("package", {}).get("name", rel)
            enablers[name] = rel
    producer = producer_table((REPO / PRODUCER).read_text(encoding="utf-8"))
    return (
        crates,
        enablers,
        producer,
        load(RMW_MANIFEST),
        (REPO / RMW_BUILD).read_text(encoding="utf-8"),
        (REPO / NODE_BUILD).read_text(encoding="utf-8"),
    )


# --- self-test ---------------------------------------------------------------


def self_test(quiet: bool = False) -> int:
    """Plant each violation and demand a red, on SYNTHETIC inputs.

    A control driven by the tree it checks passes the day the tree changes,
    for the wrong reason.
    """

    def m(name, feats=None, optional=False, extra=None):
        dep = {"path": "x", "features": feats or []}
        if optional:
            dep["optional"] = True
        doc = {"package": {"name": name}, "dependencies": {"nros-rmw": dep}}
        if extra:
            doc.update(extra)
        return doc

    cls = {
        "be-in": (IN_PLACE, ("zen",), "r"),
        "be-buf": (BUFFERED, ("cyc",), "r"),
        "router": (None, (), "r"),
    }
    good_crates = {
        "be-in": m("be-in", ["in-place-dispatch"]),
        "be-buf": m("be-buf", ["buffered-dispatch"]),
        "router": m("router"),
    }
    good_enablers = {"be-in": "a/Cargo.toml", "be-buf": "b/Cargo.toml"}
    good_producer = {"zen": IN_PLACE, "cyc": BUFFERED}
    rmw_manifest = {
        "package": {"name": "nros-rmw", "links": "nros_rmw"},
        "features": {"in-place-dispatch": [], "buffered-dispatch": []},
    }
    rmw_build = 'CARGO_FEATURE_IN_PLACE_DISPATCH "in_place_dispatch" CARGO_FEATURE_BUFFERED_DISPATCH "buffered_dispatch"'
    node_build = "DEP_NROS_RMW_IN_PLACE_DISPATCH DEP_NROS_RMW_BUFFERED_DISPATCH"

    def run(**over):
        args = dict(
            crates=good_crates,
            enablers=good_enablers,
            producer=good_producer,
            rmw_manifest=rmw_manifest,
            rmw_build=rmw_build,
            node_build=node_build,
            classification=cls,
        )
        args.update(over)
        return check(**args)

    assert run() == [], f"the clean control went red: {run()}"

    cases = {
        "an unclassified crate": dict(crates={**good_crates, "new-be": m("new-be")}),
        "a row naming no crate": dict(
            crates={k: v for k, v in good_crates.items() if k != "router"}
        ),
        "a backend that declares nothing": dict(
            crates={**good_crates, "be-buf": m("be-buf")}
        ),
        "a backend declaring the wrong dispatch": dict(
            crates={**good_crates, "be-buf": m("be-buf", ["in-place-dispatch"])}
        ),
        "a backend declaring both": dict(
            crates={
                **good_crates,
                "be-in": m("be-in", ["in-place-dispatch", "buffered-dispatch"]),
            }
        ),
        "a declaration on an optional dep": dict(
            crates={
                **good_crates,
                "be-in": m("be-in", ["in-place-dispatch"], optional=True),
            }
        ),
        "a declaration through a crate feature": dict(
            crates={
                **good_crates,
                "be-in": m(
                    "be-in",
                    ["in-place-dispatch"],
                    extra={"features": {"x": ["nros-rmw/in-place-dispatch"]}},
                ),
            }
        ),
        "a router that declares": dict(
            crates={**good_crates, "router": m("router", ["in-place-dispatch"])}
        ),
        "an enabler outside the backends": dict(
            enablers={**good_enablers, "nros-c": "c/Cargo.toml"}
        ),
        "a producer arm with no backend": dict(
            producer={**good_producer, "dds": BUFFERED}
        ),
        "a producer disagreeing with the build": dict(
            producer={"zen": BUFFERED, "cyc": BUFFERED}
        ),
        "a backend name the producer omits": dict(producer={"zen": IN_PLACE}),
        "an unreadable producer": dict(producer=None),
        "a missing links key": dict(
            rmw_manifest={**rmw_manifest, "package": {"name": "nros-rmw"}}
        ),
        "a missing nros-rmw feature": dict(
            rmw_manifest={**rmw_manifest, "features": {"buffered-dispatch": []}}
        ),
        "a build script that does not publish": dict(
            rmw_build=rmw_build.replace('"in_place_dispatch"', "")
        ),
        "a reader of a renamed key": dict(node_build="DEP_NROS_RMW_BUFFERED_DISPATCH"),
    }
    for label, over in cases.items():
        assert run(**over), f"self-test: {label} was NOT caught"

    # The producer reader, on the real shape.
    src = (
        "fn backend_dispatch(rmw: &str) -> Option<BackendDispatch> {\n"
        "    match rmw {\n"
        '        "cyclonedds" | "cyclone" => Some(BackendDispatch::Buffered),\n'
        '        "zenoh" | "xrce" => Some(BackendDispatch::InPlace),\n'
        "        _ => None,\n"
        "    }\n"
        "}\n"
    )
    assert producer_table(src) == {
        "cyclonedds": BUFFERED,
        "cyclone": BUFFERED,
        "zenoh": IN_PLACE,
        "xrce": IN_PLACE,
    }, producer_table(src)
    assert producer_table(src.replace("Buffered", "Loaned")) is None

    if not quiet:
        print(
            f"[OK] self-test: clean control green, {len(cases)} planted violations red"
        )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    self_test(quiet=True)
    errors = check(*tree_inputs())
    if errors:
        print("[FAIL] RMW backend dispatch declarations (issue 1577):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        print(
            "\n  `nros-node` prices a descriptor's `in_place` subscription at no receive\n"
            "  region only when the linked backends CLAIM in-place dispatch and none\n"
            "  declares buffered. See `nros-rmw`'s Cargo.toml features.",
            file=sys.stderr,
        )
        return 1
    backends = sum(1 for d, _, _ in CLASSIFICATION.values() if d)
    print(
        f"[OK] {len(CLASSIFICATION)} packages/rmw crates classified, {backends} backends "
        "declare their dispatch, the producer's table agrees"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
