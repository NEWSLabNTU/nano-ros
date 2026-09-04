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
    scripts/api-parity.py --include-internal  # do not filter the ROS 2 side to public API
    scripts/api-parity.py --topic pubsub   # one stage, all three languages
    scripts/api-parity.py --by-topic      # what is left, per stage
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

# Only PUBLIC ROS 2 items are compared

nano-ros aligns to the API a ROS 2 user writes, not to rclcpp's callback type
erasure, rcl's wait-set plumbing, or the generated accessors of
`rcl_interfaces`. `public_surface.py` drops those, keyed on the file each
declaration came from rather than on its name, and the report says how many
each tier removed. `--include-internal` compares everything.

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
import re
import subprocess
import sys
import fnmatch
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "api_parity"))

import correlate  # noqa: E402
import extract_cxx  # noqa: E402
import extract_rust  # noqa: E402
import public_surface  # noqa: E402
import signature_rules  # noqa: E402
import topics  # noqa: E402

SURFACE_DIR = os.path.join(ROOT, "docs", "reference", "api-surface")
LEDGER_DIR = os.path.join(ROOT, "docs", "reference", "api-parity-ledger")

VERDICTS = ("divergence", "extension", "declined", "gap", "rename")

BUCKETS = ("systematic", "arity-only", "differs", "ours-only", "theirs-only")

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


# issue 0818 — the C++ surface is the UNION of three translation units, not
# whatever `nros.hpp` happens to reach.
#
# `nros.hpp` is a curated convenience header. Using it as the definition of "our
# C++ API" silently made every header it omits non-API, and the tool reported
# that silence as agreement:
#
#   * `component_node.hpp` is not included by it at all, so `ComponentNode`,
#     `NodeHandle`, the `bind_*` family and the `create_*_raw` family produced
#     ZERO rows while holding ~half the C++ `create_timer` call sites.
#   * `std_compat.hpp` IS included, but behind `#ifdef NROS_CPP_STD`, which this
#     extractor never defined — so its eleven free functions were invisible.
#
# Two wrong ledger rows came out of that in one day: W5 group A renamed
# `make_publisher` -> `create_publisher` against a row recording no collision
# when `nros::create_publisher` already existed in std_compat, and
# `cpp:create_timer` claimed the fix was "ADDING a free function" that had
# existed all along.
#
# The std flavour is extracted SEPARATELY rather than by defining the macro for
# everything, because a `no_std` consumer genuinely does not get those symbols —
# folding them into the base surface would trade one wrong answer for another.
# Items reachable only with the flag are tagged `std_only` so a row can say so.
CPP_TRANSLATION_UNITS = (
    ("base", '#include "nros/nros.hpp"\n', ()),
    ("component", '#include "nros/component_node.hpp"\n', ()),
    ("std", '#include "nros/nros.hpp"\n', ("-DNROS_CPP_STD=1",)),
)


def ours_cpp(tmpdir):
    # The de-dup key is the WHOLE RECORD, not its `qual`. `extract` emits one
    # record per DECLARATION -- overloads are separate records that `correlate.
    # flatten` groups afterwards, and a type's record carries its member list --
    # so collapsing on the name alone drops exactly what the extra TUs were
    # added to see. Measured: `nros::spin` lost its `(uint32_t, int32_t)`
    # overload and was reported `differs` against rclcpp when it is
    # `systematic`; `nros::init` lost three of four; `Timer::attach_std_closure`,
    # `GuardCondition::attach_std_closure` and `Seq::to_vector` vanished because
    # their owning type was already seen in the base TU with a shorter member
    # list. That is issue 0818's own failure -- a silently narrowed C++ surface
    # reported as agreement -- one level down.
    seen = set()
    order = []
    for label, source, extra in CPP_TRANSLATION_UNITS:
        items = extract_cxx.extract(
            source,
            "c++",
            extract_cxx.nros_cpp_include_args() + list(extra),
            {"nros"},
            tmpdir,
        )
        for item in items:
            key = json.dumps(item, sort_keys=True)
            if key in seen:
                continue
            seen.add(key)
            if label == "std":
                # Only reachable when the consumer opted into the std flavour.
                item["std_only"] = True
            order.append(item)
    return order


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
        features=extract_rust.NROS_FEATURES,
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
    """Every ledger shard, merged, minus their documentation.

    One file per language. The split is for CONCURRENCY, not taste: classifying
    ~1300 rows is work for several people at once, and a single file makes every
    one of them rebase against the others. A shard is only ever touched by
    whoever owns that lane.

    JSON has no comments, so each shard explains itself in `_doc`. Keys
    beginning with `_` are documentation and are never entries.

    A shard is named after a TOPIC -- `node.json`, `pubsub.json` -- and holds
    that topic's rows in ALL THREE languages. The campaign closes a feature at a
    time across every language, so the topic is what an agent owns and what
    "complete" is asserted about. Sharding by language instead would let C++
    pubsub land while C pubsub sits unexamined, and the drop-in claim is made
    per language: a feature that works in one is not a feature.

    A row filed in the wrong shard is a real error, not a harmless one -- the
    shard is the topic's inventory, and a `qos` row hiding in `node.json` makes
    both stages' counts wrong. `--self-test` rejects it, using the same
    `topics.topic_of` the report groups by, so a shard cannot disagree with the
    taxonomy.
    """
    merged = {}
    if not os.path.isdir(LEDGER_DIR):
        return merged
    for name in sorted(os.listdir(LEDGER_DIR)):
        if not name.endswith(".json"):
            continue
        shard_topic = name[: -len(".json")]
        with open(os.path.join(LEDGER_DIR, name)) as fh:
            text = fh.read()
        # A DUPLICATE KEY is silently survivable and therefore dangerous: JSON
        # parsers keep the last occurrence, so a second row for the same symbol
        # discards the first author's verdict and reasoning with no error
        # anywhere. It happens because two sessions classify the same symbol
        # independently and git merges both -- they land in different regions of
        # the file, so there is no textual conflict to notice. `other.json`
        # carried `cpp:declared_depth` twice on 2026-09-04 for exactly that
        # reason; both said `extension`, which is luck, not a guarantee.
        pairs = json.loads(text, object_pairs_hook=lambda p: p)
        seen = set()
        for key, _ in pairs:
            if key in seen:
                raise SystemExit(
                    f"api-parity: {name} defines {key!r} more than once. JSON keeps the "
                    f"LAST, so the other row's verdict is being discarded silently. "
                    f"Merge them into one row -- do not delete either reason."
                )
            seen.add(key)
        raw = dict(pairs)
        for key, value in raw.items():
            if key.startswith("_"):
                continue
            value = dict(value)
            value["_shard"] = shard_topic
            # The lane is what correctness turns on; the FILENAME is what the
            # person fixing it has to open. A message naming `cpp.json` when the
            # row sits in `cpp-node.json` sends them to a file that need not
            # exist.
            value["_file"] = name
            merged[key] = value
    return merged


def ledger_key(lang, key):
    return "%s:%s" % (lang, key)


def lookup(ledger, lang, key, bucket, buckets_by_key):
    """(entry, inherited) for a row -- a member may inherit its TYPE's verdict.

    `rclcpp::Node` has 49 public methods we do not have. Writing 49 sentences
    that each say "we have no Node" is not a ledger, it is a copy-paste
    exercise, and the fiftieth reader stops reading. So a row on `Node` covers
    `Node::*`.

    The inheritance is conditional, and the condition is the point: it applies
    only when the TYPE is in the SAME bucket as the member. If we have `Node`
    but not `Node::declare_parameter`, the type is `same` and the method is
    `theirs-only` -- a real gap in a type we ship, which is a different
    statement from "we do not have this type" and must be argued on its own.
    An inherited verdict prints with a trailing `*`.
    """
    own = ledger.get(ledger_key(lang, key))
    if own is not None:
        return own, False

    if "::" in key:
        owner = key.rsplit("::", 1)[0]
        if buckets_by_key.get(owner) == bucket:
            inherited = ledger.get(ledger_key(lang, owner))
            if inherited is not None:
                return inherited, True

    # A glob row covers a family. The C surface needs this and the C++/Rust
    # surfaces do not: C names are flat, so `publisher_init` and
    # `publisher_fini` share no owning type for a verdict to descend from, and
    # the entity prefix is the only structure the API has.
    #
    # A glob must DECLARE the bucket it covers, and only matches rows in it.
    # Without that, one `c:action_*` row would silently absorb a gap, an
    # extension and an unexplained signature change alike -- three different
    # claims under one sentence, which is the failure a ledger exists to
    # prevent. Most specific glob wins, so a narrower row can override.
    best = None
    for lkey, entry in ledger.items():
        klang, _, pattern = lkey.partition(":")
        if klang != lang or "*" not in pattern:
            continue
        if entry.get("bucket") != bucket:
            continue
        if not fnmatch.fnmatchcase(key, pattern):
            continue
        if best is None or len(pattern) > len(best[0]):
            best = (pattern, entry)
    if best is not None:
        return best[1], True
    return None, False


def validate_ledger(entries):
    """Structural complaints about ledger rows -- what is checkable with no build.

    Separated from `load_ledger` so the self-test can feed it a bad row. A
    validator that can only be run against the good ledger on disk proves the
    good ledger is good and nothing about the check.

    Deliberately does NOT check that a row sits in the right topic shard. A
    topic is decided by the DECLARING HEADER (see `topics.topic_of`), and a
    ledger row carries no header -- deciding it from the name alone here would
    make the gate disagree with the report for exactly the rows the header was
    introduced to get right. `--check` does that check, where the header is
    known.
    """
    problems = []
    for key, value in sorted(entries.items()):
        if value.get("verdict") not in VERDICTS:
            problems.append("ledger %s: unknown verdict %r" % (key, value.get("verdict")))
        if not value.get("why", "").strip():
            problems.append("ledger %s: empty reason" % key)
        pattern = key.partition(":")[2]
        if "*" in pattern and value.get("bucket") not in BUCKETS:
            problems.append(
                "ledger %s: a glob row must declare the bucket it covers (one of %s)"
                % (key, ", ".join(BUCKETS))
            )
        lang = key.split(":", 1)[0]
        if lang not in LANGS:
            problems.append("ledger %s: unknown language %r" % (key, lang))
        if "_shard" in value and value["_shard"] not in topics.NAMES:
            problems.append(
                "ledger %s: %s is not a topic; shards are named for one of %s"
                % (key, value.get("_file", "?"), ", ".join(topics.NAMES))
            )
    return problems


# --------------------------------------------------------------------------
# report
# --------------------------------------------------------------------------


def run_lang(lang, tmpdir, include_internal=False):
    ours_records = {"c": ours_c, "cpp": ours_cpp, "rust": ours_rust}[lang](tmpdir)
    payload = load_theirs(lang)
    theirs_records = payload["records"]
    removed = {}
    if not include_internal:
        # Filtered at REPORT time, not at --refresh time: the recorded surface
        # stays complete, so tightening or loosening the public-API rule is a
        # code change rather than a re-derivation on a host with ROS installed.
        theirs_records, removed = public_surface.filter_records(theirs_records)
    clang = {"c": "c", "cpp": "c++", "rust": "rust"}[lang]
    ours = correlate.flatten(ours_records, clang, "ours")
    theirs = correlate.flatten(theirs_records, clang, "theirs")
    return correlate.compare(ours, theirs, clang), payload.get("provenance", {}), removed


def row_topic(row):
    """The stage a report row belongs to.

    Prefers THEIRS's header, because a `theirs-only` row is a statement about
    the ROS 2 surface and that is the surface being carved into stages. Falls
    back to ours, then to the name alone.
    """
    header = ""
    for side in ("theirs", "ours"):
        item = row.get(side)
        if item and item.get("header"):
            header = item["header"]
            break
    return topics.topic_of(row["key"], header)


def by_topic(langs, tmpdir):
    """Per stage, per language, how many DECISIONS are still unledgered.

    A decision, not a row: a member whose type carries the verdict is already
    answered, so counting rows would report the same work several times and make
    a finished stage look unfinished.
    """
    ledger = load_ledger()
    table = {}
    for lang in langs:
        rows, _prov, _removed = run_lang(lang, tmpdir)
        buckets = {r["key"]: r["bucket"] for r in rows}
        for r in rows:
            if r["bucket"] == "same":
                continue
            entry, _inh = lookup(ledger, lang, r["key"], r["bucket"], buckets)
            if entry is not None or r["bucket"] == "systematic":
                continue
            if "::" in r["key"]:
                owner = r["key"].rsplit("::", 1)[0]
                if buckets.get(owner) == r["bucket"]:
                    # Answered by whatever answers its type.
                    continue
            topic = row_topic(r)
            table.setdefault(topic, {}).setdefault(lang, 0)
            table[topic][lang] += 1

    print("\ndecisions still open, by stage (in the order STAGE_ORDER gives):\n")
    print("    %-10s %8s %8s %8s %8s" % ("stage", *langs, "total"))
    grand = 0
    for topic in topics.STAGE_ORDER:
        counts = table.get(topic, {})
        total = sum(counts.values())
        grand += total
        if not total:
            print("    %-10s %8s %8s %8s %8s   done" % (topic, *(
                counts.get(l, 0) for l in langs), total))
            continue
        print("    %-10s %8d %8d %8d %8d" % (
            topic, *(counts.get(l, 0) for l in langs), total))
    print("    %-10s %8s %8s %8s %8d" % ("", "", "", "", grand))
    return 0


def report(langs, show, check, suggest, include_internal, grep=None, topic=None):
    ledger = load_ledger()
    unledgered = []
    misfiled = []
    with tempfile.TemporaryDirectory() as tmpdir:
        for lang in langs:
            rows, prov, removed = run_lang(lang, tmpdir, include_internal)
            counts = {}
            by_key = {}
            for r in rows:
                counts[r["bucket"]] = counts.get(r["bucket"], 0) + 1
                by_key[r["key"]] = r["bucket"]

            print("\n=== %s vs %s ===" % (lang, prov.get("package", "?")))
            if prov:
                bits = [f"{k}={v}" for k, v in sorted(prov.items()) if v]
                print("    " + "  ".join(bits))
            print(
                "    same %d   arity-only %d   systematic %d   differs %d   "
                "ours-only %d   theirs-only %d"
                % (
                    counts.get("same", 0),
                    counts.get("arity-only", 0),
                    counts.get("systematic", 0),
                    counts.get("differs", 0),
                    counts.get("ours-only", 0),
                    counts.get("theirs-only", 0),
                )
            )
            if removed:
                # Never silent: a filter that shrinks a number without saying
                # what it took reads exactly like progress.
                print(
                    "    not public API, excluded: "
                    + "  ".join("%s %d" % (t, n) for t, n in sorted(removed.items()))
                )
            elif include_internal:
                print("    --include-internal: comparing the whole ROS 2 surface")

            for r in rows:
                bucket = r["bucket"]
                if grep is not None and not grep.search(r["key"]):
                    continue
                if topic is not None and row_topic(r) != topic:
                    continue
                if bucket == "same" and "same" not in show:
                    continue
                if bucket != "same" and bucket not in show and show != {"all"}:
                    if show and "all" not in show:
                        continue
                entry, inherited = lookup(ledger, lang, r["key"], bucket, by_key)
                if entry is not None and not inherited:
                    want = row_topic(r)
                    if entry.get("_shard") not in (None, want):
                        misfiled.append((ledger_key(lang, r["key"]),
                                         entry["_shard"], want))
                verdict = (entry["verdict"] + "*") if (entry and inherited) else (
                    entry["verdict"] if entry else ""
                )
                if bucket == "systematic":
                    # The rule IS the explanation. Requiring a ledger row too
                    # would restate one sentence once per site, which is how the
                    # sentence stops being read.
                    verdict = "rule:" + ",".join(r["detail"]["rules"])
                elif bucket != "same" and entry is None:
                    unledgered.append((lang, bucket, r["key"]))
                mark = {"same": " ", "arity-only": "?", "systematic": "=",
                        "differs": "!", "ours-only": "+", "theirs-only": "-"}[bucket]
                line = "  %s %-52s %-12s %s" % (mark, r["key"], verdict or "UNLEDGERED", bucket)
                print(line)
                if bucket in ("differs", "systematic", "arity-only") and r.get("detail"):
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
        if misfiled:
            print(
                "\n%d ledger row(s) sit in the wrong topic shard "
                "(all listed; the move is mechanical):" % len(misfiled),
                file=sys.stderr,
            )
            for key, was, want in misfiled:
                print("  %s  is in %s.json, belongs in %s.json" % (key, was, want),
                      file=sys.stderr)
            return 1
        if unledgered:
            print(
                "\n%d item(s) differ with no ledger entry. Add a row to "
                "%s/<lang>.json\nwith one of: %s"
                % (len(unledgered), os.path.relpath(LEDGER_DIR, ROOT), ", ".join(VERDICTS)),
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
        "rust State type folds",
        correlate.normalize("rust", "theirs", "rclrs::PublisherState", "type"),
        "Publisher",
    )
    state_rows = correlate.compare(
        correlate.flatten(
            [{"kind": "type", "qual": "nros::Publisher", "name": "Publisher",
              "members": [{"name": "publish", "params": [{"type": "&M"}], "ret": "", "template": []}]}],
            "rust", "ours"),
        correlate.flatten(
            [{"kind": "type", "qual": "rclrs::PublisherState", "name": "PublisherState",
              "members": [{"name": "publish", "params": [{"type": "&M"}], "ret": "", "template": []}]}],
            "rust", "theirs"),
        "rust",
    )
    check(
        "an rclrs State split is not a divergence",
        {r["key"]: r["bucket"] for r in state_rows}.get("Publisher::publish"),
        "same",
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

    # A library prefix is not a type difference; a meaning is.
    handles = correlate.shares_only_arity(
        {"overloads": [{"params": [{"type": "struct nros_client_t *"}]}]},
        {"overloads": [{"params": [{"type": "rcl_client_t *"}]}]},
    )
    check("prefixed handles are the same type", handles, False)
    # A defaulted tail must be trimmed before positions are compared, or
    # `spin(int32_t = 10)` against `spin()` reads as arity-only.
    check(
        "a defaulted tail does not make a call arity-only",
        correlate.shares_only_arity(
            {"overloads": [{"params": [{"type": "int32_t", "default": True}]}]},
            {"overloads": [{"params": []}]},
        ),
        False,
    )
    check(
        "a shared arity with no shared position is not `same`",
        correlate.shares_only_arity(
            {"overloads": [{"params": [{"type": "const char *"}, {"type": "uint8_t"}]}]},
            {"overloads": [{"params": [{"type": "int"}, {"type": "char **"}]}]},
        ),
        True,
    )

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
    failures.extend(validate_ledger(load_ledger()))

    # And the validator itself must reject each shape, or it only proves the
    # ledger on disk is the ledger on disk.
    bad = {
        "cpp:A": {"verdict": "typo", "why": "x", "_shard": "cpp"},
        "cpp:B": {"verdict": "gap", "why": "  ", "_shard": "cpp"},
        "cpp:E": {"verdict": "gap", "why": "x", "_shard": "nonsense",
                  "_file": "nonsense.json"},
        "go:D": {"verdict": "gap", "why": "x", "_shard": "go"},
    }
    caught = validate_ledger(bad)
    for needle in ("unknown verdict", "empty reason", "is not a topic",
                   "unknown language"):
        if not any(needle in c for c in caught):
            failures.append("validate_ledger missed %r" % needle)

    # A type-level row covers its members only when both sit in the same
    # bucket. The negative case is the one that matters: a gap INSIDE a type we
    # ship is a different claim from not shipping the type.
    led = {"cpp:Node": {"verdict": "gap", "why": "x"}}
    got, inh = lookup(led, "cpp", "Node::create_wall_timer", "theirs-only",
                      {"Node": "theirs-only"})
    if not (got and inh):
        failures.append("a member did not inherit its type's verdict")
    got, inh = lookup(led, "cpp", "Node::create_wall_timer", "theirs-only",
                      {"Node": "same"})
    if got is not None:
        failures.append("a member inherited a verdict across differing buckets")
    got, inh = lookup(led, "cpp", "Node", "theirs-only", {"Node": "theirs-only"})
    if got is None or inh:
        failures.append("a type's own row was reported as inherited")

    # A glob covers a family, but only inside the bucket it declares.
    globbed = {
        "c:action_*": {"verdict": "gap", "why": "x", "bucket": "theirs-only"},
        "c:action_server_init": {"verdict": "divergence", "why": "y"},
    }
    got, inh = lookup(globbed, "c", "action_publish_feedback", "theirs-only", {})
    if not (got and inh and got["verdict"] == "gap"):
        failures.append("a glob row did not cover its family")
    got, _ = lookup(globbed, "c", "action_publish_feedback", "differs", {})
    if got is not None:
        failures.append("a glob row covered a bucket it did not declare")
    got, inh = lookup(globbed, "c", "action_server_init", "theirs-only", {})
    if not got or inh or got["verdict"] != "divergence":
        failures.append("an exact row lost to a glob")
    if not any("must declare the bucket" in c for c in validate_ledger(
            {"c:foo_*": {"verdict": "gap", "why": "x"}})):
        failures.append("a bucketless glob row was accepted")

    failures.extend(public_surface.self_test())
    failures.extend(signature_rules.self_test())
    failures.extend(topics.self_test())

    for f in failures:
        print("FAIL " + f, file=sys.stderr)
    print("self-test: %d checks failed" % len(failures))
    return 1 if failures else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--lang", action="append", choices=LANGS, help="default: all three")
    ap.add_argument("--show", action="append", default=None,
                    help="buckets: same, arity-only, systematic, differs, ours-only, theirs-only, all")
    ap.add_argument("--check", action="store_true", help="exit non-zero on an unledgered difference")
    ap.add_argument("--suggest-renames", action="store_true",
                    help="pair unmatched names by similarity (suggestions, never findings)")
    ap.add_argument("--include-internal", action="store_true",
                    help="do not filter the ROS 2 side down to public API")
    ap.add_argument("--grep", metavar="REGEX",
                    help="list only rows whose key matches; the summary counts stay whole-lane")
    ap.add_argument("--topic", choices=topics.NAMES,
                    help="one stage's rows, in every language")
    ap.add_argument("--by-topic", action="store_true",
                    help="how many decisions each stage still needs, per language")
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

    show = set(args.show or ["differs", "arity-only", "systematic",
                             "ours-only", "theirs-only"])
    if args.by_topic:
        with tempfile.TemporaryDirectory() as tmpdir:
            return by_topic(langs, tmpdir)

    grep = re.compile(args.grep) if args.grep else None
    return report(langs, show, args.check, args.suggest_renames,
                  args.include_internal, grep, args.topic)


if __name__ == "__main__":
    sys.exit(main())
