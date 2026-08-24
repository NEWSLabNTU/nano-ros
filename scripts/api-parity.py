#!/usr/bin/env python3
"""Phase 379 — how the nano-ros user API differs from the ROS 2 client library it mirrors.

nano-ros claims to be rclc / rclcpp / rclrs in shape, so a ROS 2 developer can
read and write it, and so a ported source file compiles with a build-glue change
rather than a rewrite. RFC-0036 catalogs the divergences that claim permits.

That catalog is PROSE, and prose about an API goes stale silently -- RFC-0036
itself shipped calling the Rust error `RclrsError` when the type had been
`NanoRosError` for months. This tool is the same catalog with a build behind it:
it extracts both surfaces from their actual sources and reports every item that
does not correspond.

    scripts/api-parity.py                 # report all three languages
    scripts/api-parity.py --lang cpp      # one language
    scripts/api-parity.py --show same     # include the matching rows too
    scripts/api-parity.py --suggest-renames   # pair up look-alike unmatched names
    scripts/api-parity.py --check         # fail on anything unledgered
    scripts/api-parity.py --refresh       # re-derive the ROS 2 side from source
    scripts/api-parity.py --self-test

# Where the ROS 2 side comes from

Re-derived by `--refresh` from a real installation and CACHED under
`docs/reference/api-surface/`, for the reason `scripts/rmw-api-parity.py` caches
its contract: the comparison must be runnable on a host with no ROS, no rclc
checkout and no rclrs workspace, or it runs on one host and rots everywhere else.

  rclcpp  `/opt/ros/<distro>/include` -- installed headers, parsed by clang.
  rclc    a git checkout of `ros2/rclc` (`--rclc <path>`); it is not part of a
          desktop ROS install, being a micro-ROS package. Compared TOGETHER WITH
          `rcl`, because rclc is a convenience layer and not a whole API: its
          own examples call `rcl_publish`, `rcl_take` and `rcl_*_fini` directly
          (23 `rclc_executor_init` against 6 `rcl_publish` in `rclc_examples`).
          Comparing against rclc alone would score our publish and take entry
          points as inventions when they are the ROS 2 C API doing its job.
  rclrs   a `ros2_rust` workspace checkout (`--rclrs <path>`); not packaged at all.

# What a verdict means

`--check` demands that every non-matching row carry a ledger entry, one of:

  divergence  we changed it, and a platform constraint is why. This is the only
              sanctioned reason to differ (see RFC-0036), so the `why` must name
              the constraint -- `no_std`, no exceptions, no allocator, no
              runtime env, single-threaded transport -- not a preference.
  extension   we add it, and an RTOS scenario needs it. ROS 2 has no equivalent.
  declined    ROS 2 has it, we deliberately do not, with the reason.
  gap         ROS 2 has it, we should too, and nobody has done it. A gap is a
              legitimate ledger entry -- the point is that it is WRITTEN DOWN,
              not that it is absent.
  rename      the two names differ and ours is the one that should change. This
              is the campaign's work list: a rename with no platform reason is
              a defect, because the drop-in claim is what it costs.

The gate is not "no differences". It is "no UNEXPLAINED differences".
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "api_parity"))

import correlate  # noqa: E402
import extract_cxx  # noqa: E402
import extract_rust  # noqa: E402

SURFACE_DIR = os.path.join(ROOT, "docs", "reference", "api-surface")
LEDGER = os.path.join(ROOT, "docs", "reference", "api-parity-ledger.json")

VERDICTS = ("divergence", "extension", "declined", "gap", "rename")

LANGS = ("c", "cpp", "rust")

# `rclcpp.hpp` is the header a user includes; the action and lifecycle surfaces
# live in sibling packages that a ROS 2 user also includes by name.
RCLCPP_SOURCE = (
    "#include <rclcpp/rclcpp.hpp>\n"
    "#include <rclcpp_action/rclcpp_action.hpp>\n"
    "#include <rclcpp_lifecycle/lifecycle_node.hpp>\n"
)
RCLCPP_NAMESPACES = {"rclcpp", "rclcpp_action", "rclcpp_lifecycle"}

RCLC_SOURCE = (
    "#include <rcl/rcl.h>\n"
    "#include <rcl_action/rcl_action.h>\n"
    "#include <rcl/graph.h>\n"
    "#include <rcl/logging.h>\n"
    "#include <rclc/rclc.h>\n"
    "#include <rclc/executor.h>\n"
    "#include <rclc/action_server.h>\n"
    "#include <rclc/action_client.h>\n"
    "#include <rclc_lifecycle/rclc_lifecycle.h>\n"
    "#include <rclc_parameter/rclc_parameter.h>\n"
)


# --------------------------------------------------------------------------
# our side -- always extracted live, never cached
#
# Caching OUR surface would defeat the tool: the whole point is to notice when
# an edit to our headers moves us away from ROS 2, and a cache would report the
# surface as it was when somebody last remembered to refresh it.
# --------------------------------------------------------------------------


def ours_cpp(tmpdir):
    return extract_cxx.extract(
        '#include "nros/nros.hpp"\n',
        "c++",
        extract_cxx.nros_cpp_include_args(),
        {"nros"},
        tmpdir,
    )


def ours_c(tmpdir):
    return extract_cxx.extract(
        '#include "nros/nros.h"\n',
        "c",
        extract_cxx.nros_c_include_args(),
        {""},
        tmpdir,
        prefixes={"nros_", "NROS_"},
    )


def ours_rust(tmpdir):
    docdir, _ = extract_rust.rustdoc_json(
        os.path.join(ROOT, "packages", "api", "nros"),
        with_deps=True,
        target_dir=os.path.join(tmpdir, "rustdoc-ours"),
    )
    docs = extract_rust.Docs(docdir)
    return extract_rust.surface(docs, docs.crate("nros"), "nros")


# --------------------------------------------------------------------------
# their side -- cached, re-derivable
# --------------------------------------------------------------------------


def theirs_path(lang):
    return os.path.join(SURFACE_DIR, {"c": "rclc", "cpp": "rclcpp", "rust": "rclrs"}[lang] + ".json")


def load_theirs(lang):
    path = theirs_path(lang)
    if not os.path.exists(path):
        raise SystemExit(
            "no recorded surface at %s.\nRun `scripts/api-parity.py --refresh --lang %s` "
            "on a host that has the source (see this script's docstring)." % (path, lang)
        )
    with open(path) as fh:
        return json.load(fh)


def derive_rclcpp(prefix, tmpdir):
    return extract_cxx.extract(
        RCLCPP_SOURCE,
        "c++",
        extract_cxx.ros_include_args(prefix),
        RCLCPP_NAMESPACES,
        tmpdir,
    )


def derive_rclc(prefix, rclc_root, tmpdir):
    inc = extract_cxx.ros_include_args(prefix)
    for pkg in ("rclc", "rclc_lifecycle", "rclc_parameter"):
        inc.append("-I" + os.path.join(rclc_root, pkg, "include"))
    return extract_cxx.extract(
        RCLC_SOURCE, "c", inc, {""}, tmpdir, prefixes={"rclc_", "RCLC_", "rcl_", "RCL_"}
    )


def derive_rclrs(rclrs_root, tmpdir):
    docdir, _ = extract_rust.rustdoc_json(
        rclrs_root, with_deps=False, target_dir=os.path.join(tmpdir, "rustdoc-rclrs")
    )
    docs = extract_rust.Docs(docdir)
    return extract_rust.surface(docs, docs.crate("rclrs"), "rclrs")


def provenance(lang, prefix, rclc_root, rclrs_root):
    """Record WHAT a cached surface came from, so a stale one can be spotted.

    Deliberately NOT the local path it was derived from. These files are
    committed, and a checkout directory on the machine that ran `--refresh`
    identifies nothing to anyone else -- it is noise at best and a leaked home
    directory at worst. A distro name, a crate version and a git ref are what
    another host can actually compare against.
    """
    if lang == "cpp":
        return {"package": "rclcpp", "distro": os.path.basename(prefix.rstrip("/"))}
    if lang == "c":
        return {
            "package": "rclc+rcl",
            "distro": os.path.basename(prefix.rstrip("/")),
            "rclc_ref": _git_describe(rclc_root),
        }
    return {
        "package": "rclrs",
        "version": _cargo_version(rclrs_root),
        "ref": _git_describe(rclrs_root),
    }


def _git_describe(path):
    try:
        out = subprocess.run(
            ["git", "-C", path, "rev-parse", "HEAD"], capture_output=True, text=True
        )
        return out.stdout.strip() or None
    except OSError:
        return None


def _cargo_version(path):
    manifest = os.path.join(path, "Cargo.toml")
    if not os.path.exists(manifest):
        return None
    for line in open(manifest):
        if line.startswith("version"):
            return line.split("=", 1)[1].strip().strip('"')
    return None


def refresh(langs, prefix, rclc_root, rclrs_root):
    os.makedirs(SURFACE_DIR, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmpdir:
        for lang in langs:
            if lang == "cpp":
                recs = derive_rclcpp(prefix, tmpdir)
            elif lang == "c":
                if not rclc_root:
                    raise SystemExit("--rclc <path to a ros2/rclc checkout> is required for --lang c")
                recs = derive_rclc(prefix, rclc_root, tmpdir)
            else:
                if not rclrs_root:
                    raise SystemExit("--rclrs <path to the rclrs crate> is required for --lang rust")
                recs = derive_rclrs(rclrs_root, tmpdir)
            payload = {
                "provenance": provenance(lang, prefix, rclc_root, rclrs_root),
                "records": recs,
            }
            with open(theirs_path(lang), "w") as fh:
                json.dump(payload, fh, indent=1, sort_keys=True)
                fh.write("\n")
            print("recorded %d records -> %s" % (len(recs), os.path.relpath(theirs_path(lang), ROOT)))


# --------------------------------------------------------------------------
# ledger
# --------------------------------------------------------------------------


def load_ledger():
    """The ledger, minus its own documentation.

    JSON has no comments, so the file explains itself in `_doc`. Keys beginning
    with `_` are documentation and are never entries.
    """
    if not os.path.exists(LEDGER):
        return {}
    with open(LEDGER) as fh:
        raw = json.load(fh)
    return {k: v for k, v in raw.items() if not k.startswith("_")}


def ledger_key(lang, key):
    return "%s:%s" % (lang, key)


# --------------------------------------------------------------------------
# report
# --------------------------------------------------------------------------


def run_lang(lang, tmpdir):
    ours_records = {"c": ours_c, "cpp": ours_cpp, "rust": ours_rust}[lang](tmpdir)
    payload = load_theirs(lang)
    theirs_records = payload["records"]
    clang = {"c": "c", "cpp": "c++", "rust": "rust"}[lang]
    ours = correlate.flatten(ours_records, clang, "ours")
    theirs = correlate.flatten(theirs_records, clang, "theirs")
    return correlate.compare(ours, theirs, clang), payload.get("provenance", {})


def report(langs, show, check, suggest):
    ledger = load_ledger()
    unledgered = []
    with tempfile.TemporaryDirectory() as tmpdir:
        for lang in langs:
            rows, prov = run_lang(lang, tmpdir)
            counts = {}
            for r in rows:
                counts[r["bucket"]] = counts.get(r["bucket"], 0) + 1

            print("\n=== %s vs %s ===" % (lang, prov.get("package", "?")))
            if prov:
                bits = [f"{k}={v}" for k, v in sorted(prov.items()) if v]
                print("    " + "  ".join(bits))
            print(
                "    same %d   differs %d   ours-only %d   theirs-only %d"
                % (
                    counts.get("same", 0),
                    counts.get("differs", 0),
                    counts.get("ours-only", 0),
                    counts.get("theirs-only", 0),
                )
            )

            for r in rows:
                bucket = r["bucket"]
                if bucket == "same" and "same" not in show:
                    continue
                if bucket != "same" and bucket not in show and show != {"all"}:
                    if show and "all" not in show:
                        continue
                entry = ledger.get(ledger_key(lang, r["key"]))
                verdict = entry["verdict"] if entry else ""
                if bucket != "same" and entry is None:
                    unledgered.append((lang, bucket, r["key"]))
                mark = {"same": " ", "differs": "!", "ours-only": "+", "theirs-only": "-"}[bucket]
                line = "  %s %-52s %-12s %s" % (mark, r["key"], verdict or "UNLEDGERED", bucket)
                print(line)
                if bucket == "differs" and r.get("detail"):
                    print(
                        "      ours   %s\n      theirs %s"
                        % (
                            correlate.render_params(r["ours"]),
                            correlate.render_params(r["theirs"]),
                        )
                    )

            if suggest:
                pairs = correlate.suggest_renames(rows)
                print(
                    "\n  possible renames (%d) -- SIMILARITY, not evidence; confirm each\n"
                    "  before writing a ledger row, and expect false pairs:" % len(pairs)
                )
                for ours_key, theirs_key, ratio in pairs:
                    print("    %.2f  %-42s ->  %s" % (ratio, ours_key, theirs_key))

    if check:
        if unledgered:
            print(
                "\n%d item(s) differ with no ledger entry. Add a row to %s\n"
                "with one of: %s"
                % (len(unledgered), os.path.relpath(LEDGER, ROOT), ", ".join(VERDICTS)),
                file=sys.stderr,
            )
            for lang, bucket, key in unledgered[:40]:
                print("  %s:%s  (%s)" % (lang, key, bucket), file=sys.stderr)
            if len(unledgered) > 40:
                print("  ... and %d more" % (len(unledgered) - 40), file=sys.stderr)
            return 1
        print("\nevery divergence carries a ledger entry")
    return 0


def self_test():
    """Exercise the correlator on hand-built records, not on the real surfaces.

    The real surfaces need clang, a ROS install and a nightly toolchain; a
    self-test that needs those is a self-test that gets skipped.
    """
    failures = []

    def check(name, got, want):
        if got != want:
            failures.append("%s: got %r want %r" % (name, got, want))

    check("c prefix ours", correlate.normalize("c", "ours", "nros_publisher_init", "function"), "publisher_init")
    check("c prefix theirs", correlate.normalize("c", "theirs", "rclc_publisher_init", "function"), "publisher_init")
    check(
        "cpp method",
        correlate.normalize("c++", "ours", "nros::Node::create_publisher", "function"),
        "Node::create_publisher",
    )
    check(
        "rust State fold",
        correlate.normalize("rust", "theirs", "rclrs::NodeState::create_publisher", "function"),
        "Node::create_publisher",
    )
    check(
        "rust NodeCtx synonym",
        correlate.normalize("rust", "ours", "nros::node::NodeCtx::create_publisher", "function"),
        "Node::create_publisher",
    )
    check("cpp type", correlate.normalize("c++", "ours", "nros::QoS", "type"), "QoS")
    check(
        "rclcpp Base fold",
        correlate.normalize("c++", "theirs", "rclcpp::PublisherBase::get_topic_name", "function"),
        "Publisher::get_topic_name",
    )
    check(
        "a real *Base type is not folded",
        correlate.normalize("c++", "theirs", "rclcpp::node_interfaces::NodeBase::get_name", "function"),
        "NodeBase::get_name",
    )
    # The type key must fold as well -- member keys are built from it, so
    # folding only the method owner changes nothing at all.
    check(
        "rclcpp Base type folds",
        correlate.normalize("c++", "theirs", "rclcpp::PublisherBase", "type"),
        "Publisher",
    )
    base_rows = correlate.compare(
        correlate.flatten(
            [{"kind": "type", "qual": "nros::Publisher", "name": "Publisher",
              "members": [{"name": "get_topic_name", "params": [], "ret": "", "template": []}]}],
            "c++", "ours"),
        correlate.flatten(
            [{"kind": "type", "qual": "rclcpp::PublisherBase", "name": "PublisherBase",
              "members": [{"name": "get_topic_name", "params": [], "ret": "", "template": []}]}],
            "c++", "theirs"),
        "c++",
    )
    check(
        "an inheritance split is not a divergence",
        {r["key"]: r["bucket"] for r in base_rows}.get("Publisher::get_topic_name"),
        "same",
    )
    check("type noise", correlate.canon_type("const std::string &"), "string&")

    ours = correlate.flatten(
        [
            {
                "kind": "type",
                "qual": "nros::Node",
                "name": "Node",
                "members": [
                    {"name": "create_publisher", "params": [{"type": "const char *"}], "ret": "", "template": []},
                    {"name": "only_ours", "params": [], "ret": "", "template": []},
                ],
            }
        ],
        "c++",
        "ours",
    )
    theirs = correlate.flatten(
        [
            {
                "kind": "type",
                "qual": "rclcpp::Node",
                "name": "Node",
                "members": [
                    {
                        "name": "create_publisher",
                        "params": [{"type": "const std::string &"}, {"type": "const rclcpp::QoS &"}],
                        "ret": "",
                        "template": [],
                    },
                    {"name": "only_theirs", "params": [], "ret": "", "template": []},
                ],
            }
        ],
        "c++",
        "theirs",
    )
    rows = {r["key"]: r["bucket"] for r in correlate.compare(ours, theirs, "c++")}
    check("Node same", rows.get("Node"), "same")

    # A defaulted parameter must not read as a divergence -- `spin(int32_t = 10)`
    # against `spin()` is the convergence issue 0338 landed on purpose.
    defaulted = correlate.flatten(
        [
            {
                "kind": "type",
                "qual": "nros::Executor",
                "name": "Executor",
                "members": [
                    {
                        "name": "spin",
                        "params": [{"type": "int32_t", "default": True}],
                        "ret": "",
                        "template": [],
                    }
                ],
            }
        ],
        "c++",
        "ours",
    )
    plain = correlate.flatten(
        [
            {
                "kind": "type",
                "qual": "rclcpp::Executor",
                "name": "Executor",
                "members": [{"name": "spin", "params": [], "ret": "", "template": []}],
            }
        ],
        "c++",
        "theirs",
    )
    drows = {r["key"]: r["bucket"] for r in correlate.compare(defaulted, plain, "c++")}
    check("default arg is not a divergence", drows.get("Executor::spin"), "same")

    rename_rows = correlate.compare(
        correlate.flatten(
            [{"kind": "function", "qual": "nros::make_publisher", "name": "make_publisher",
              "params": [], "ret": "", "template": []}],
            "c++", "ours"),
        correlate.flatten(
            [{"kind": "function", "qual": "rclcpp::create_publisher", "name": "create_publisher",
              "params": [], "ret": "", "template": []}],
            "c++", "theirs"),
        "c++",
    )
    pairs = correlate.suggest_renames(rename_rows)
    check("rename suggested", [(a, b) for a, b, _ in pairs], [("make_publisher", "create_publisher")])
    check(
        "unlike names are not paired",
        correlate.suggest_renames(
            correlate.compare(
                correlate.flatten(
                    [{"kind": "function", "qual": "nros::zzz_board_locator", "name": "zzz_board_locator",
                      "params": [], "ret": "", "template": []}],
                    "c++", "ours"),
                correlate.flatten(
                    [{"kind": "function", "qual": "rclcpp::spin", "name": "spin",
                      "params": [], "ret": "", "template": []}],
                    "c++", "theirs"),
                "c++",
            )
        ),
        [],
    )
    check("arity differs", rows.get("Node::create_publisher"), "differs")
    check("ours-only", rows.get("Node::only_ours"), "ours-only")
    check("theirs-only", rows.get("Node::only_theirs"), "theirs-only")

    # A ledger row must not be able to claim a verdict this tool does not know:
    # a typo'd verdict that silently satisfies the gate is the failure mode a
    # ledger is supposed to prevent.
    for k, v in load_ledger().items():
        if v.get("verdict") not in VERDICTS:
            failures.append("ledger %s: unknown verdict %r" % (k, v.get("verdict")))
        if not v.get("why", "").strip():
            failures.append("ledger %s: empty reason" % k)
        lang = k.split(":", 1)[0]
        if lang not in LANGS:
            failures.append("ledger %s: unknown language %r" % (k, lang))

    for f in failures:
        print("FAIL " + f, file=sys.stderr)
    print("self-test: %d checks failed" % len(failures))
    return 1 if failures else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--lang", action="append", choices=LANGS, help="default: all three")
    ap.add_argument("--show", action="append", default=None,
                    help="buckets to list: same, differs, ours-only, theirs-only, all")
    ap.add_argument("--check", action="store_true", help="exit non-zero on an unledgered difference")
    ap.add_argument("--suggest-renames", action="store_true",
                    help="pair unmatched names by similarity (suggestions, never findings)")
    ap.add_argument("--refresh", action="store_true", help="re-derive the ROS 2 side and record it")
    ap.add_argument("--ros-prefix", default=os.environ.get("ROS_PREFIX", "/opt/ros/humble"))
    ap.add_argument("--rclc", help="path to a ros2/rclc checkout (for --refresh --lang c)")
    ap.add_argument("--rclrs", help="path to the rclrs crate (for --refresh --lang rust)")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    langs = args.lang or list(LANGS)
    if args.refresh:
        refresh(langs, args.ros_prefix, args.rclc, args.rclrs)
        return 0

    show = set(args.show or ["differs", "ours-only", "theirs-only"])
    return report(langs, show, args.check, args.suggest_renames)


if __name__ == "__main__":
    sys.exit(main())
