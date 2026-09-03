#!/usr/bin/env python3
"""Generate the three user-API comparison pages (C, C++, Rust).

MECHANICAL. Two code inputs, one authored input, nothing inferred:

  * OUR surface        parsed from the headers and crates, by the same
                       extractors `api-parity.py` uses -- clang JSON AST for
                       C/C++, rustdoc JSON for Rust. Never a hand list.
  * ROS 2's surface    `docs/reference/api-surface/{rclc,rclcpp,rclrs}.json`,
                       recorded from an installed ROS 2 by `--refresh`.
  * WHY, and WHAT      `docs/reference/api-parity-ledger/*.json`. This is the
    ANSWERS IT         only file a human writes. `why` carries the reason a
                       row diverges; `provides` names our items that answer an
                       upstream one when the mapping is not 1:1.

The correlation itself -- which of the seven states a row is in -- is computed,
not authored. A state is a function of the bucket the correlator assigns and
the verdict the ledger records, and both of those already have gates.

Adding a reason or a re-mapping arrow means editing the ledger and re-running
this. It must never mean editing a page.

Usage:
    python3 scripts/gen-api-comparison.py                 # -> tmp/api-comparison/
    python3 scripts/gen-api-comparison.py --out DIR
    python3 scripts/gen-api-comparison.py --urls urls.json   # cross-page nav
    python3 scripts/gen-api-comparison.py --self-test
"""

import argparse
import collections
import datetime
import html
import importlib.util
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAGES = os.path.join(ROOT, "scripts/api_parity/pages")
sys.path.insert(0, os.path.join(ROOT, "scripts"))
sys.path.insert(0, os.path.join(ROOT, "scripts/api_parity"))


def load_api_parity():
    """`api-parity.py` has a dash in its name, so it needs an explicit loader."""
    spec = importlib.util.spec_from_file_location(
        "api_parity_main", os.path.join(ROOT, "scripts/api-parity.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------- states
# Ordered as the legend reads them. The glyph is not decoration: state must be
# legible without colour, and seven hues alone would not be.
STATES = [
    ("same", "same", "●"),
    ("reshaped", "re-shaped", "●"),
    ("renamed", "renamed", "●"),
    ("remapped", "re-mapped", "◆"),
    ("rejected", "rejected", "✕"),
    ("missing", "not implemented", "○"),
    ("ours", "ours only", "✥"),
]

VERDICT_STATE = {
    "declined": "rejected",
    "gap": "missing",
    "extension": "ours",
    "divergence": "reshaped",
}


def state(row):
    """The seven-way state, derived -- never authored.

    Order matters. `provides` wins over the verdict because an m-to-n mapping
    is the more specific fact: a row can be BOTH a divergence and answered by
    three of our items, and the arrow is what a reader cannot reconstruct.
    """
    if row["bucket"] == "same":
        return "same"
    if row.get("provides"):
        return "remapped"
    if row.get("verdict") == "rename" or row["bucket"] == "systematic":
        return "renamed"
    return VERDICT_STATE.get(row.get("verdict"), "reshaped")


# ---------------------------------------------------------------- signatures
def sig(item):
    """One line, from the parsed record. Rendered from params/ret, not prose."""
    if not item:
        return ""
    qual = item.get("qual") or item.get("key") or ""
    overloads = item.get("overloads")
    if not overloads:
        kind = item.get("kind") or ""
        if kind in ("type", "alias", "enum", "struct", "const", "trait"):
            return ("%s %s" % (kind, qual)).strip()
        return qual
    first = overloads[0]
    params = ", ".join(
        ((p.get("type") or "") + (" " + p["name"] if p.get("name") else "")).strip()
        for p in first.get("params", [])
    )
    extra = ""
    if len(overloads) > 1:
        n = len(overloads) - 1
        extra = "   /* +%d overload%s */" % (n, "" if n == 1 else "s")
    return ("%s %s(%s)%s" % (first.get("ret") or "", qual, params, extra)).strip()


# ---------------------------------------------------------------- grouping
TOPIC_TITLE = {
    "init": "Context & initialisation",
    "node": "Node",
    "pubsub": "Publish / subscribe",
    "service": "Services",
    "action": "Actions",
    "param": "Parameters",
    "timer": "Timers & clocks",
    "exec": "Executor & waiting",
    "qos": "Quality of service",
    "lifecycle": "Lifecycle",
    "log": "Logging",
    "graph": "Graph introspection",
    "serde": "Serialisation",
    "types": "Shared types",
    "other": "Everything else",
}

# C++ sections are the ledger's own stages. Header FILENAMES would mix ours
# with rclcpp's, giving one concept two vocabularies (`subscription.hpp` beside
# `subscription_base.hpp`) and a section list nobody can scan.
def cpp_section(row, topic):
    return TOPIC_TITLE.get(topic, topic)


# Rust audience is the crate that DEFINES the item. Every Rust item is
# `nros::`-qualified because the facade re-exports the whole tree, so grouping
# on the qualified path puts 1086 of 1778 rows in one bucket and says nothing.
FACADE = "Facade an application writes against"
RUNTIME = "Node, executor & component runtime"
PARAMS = "Parameters"
SEAM = "Backend-author seam (RMW trait)"
WIRE = "Serialisation & wire"
CORE = "Core types"

RUST_CRATE_AUD = [
    ("packages/api/nros/", FACADE),
    ("packages/core/nros-node/", RUNTIME),
    ("packages/core/nros-params/", PARAMS),
    ("packages/core/nros-rmw/", SEAM),
    ("packages/platform/", SEAM),
    ("packages/core/nros-serdes/", WIRE),
    ("packages/core/nros-core/", CORE),
]

# A `theirs-only` row has no item of ours to locate, so it is placed by rclrs's
# own module -- the row is a statement about THEIR surface.
RCLRS_AUD = {
    "node": FACADE, "publisher": FACADE, "subscription": FACADE,
    "client": FACADE, "service": FACADE, "qos": FACADE, "action": FACADE,
    "timer": FACADE, "logging": FACADE,
    "parameter": PARAMS,
    "context": RUNTIME, "executor": RUNTIME, "worker": RUNTIME, "wait_set": RUNTIME,
    "rcl_bindings": SEAM,
    "dynamic_message": WIRE, "serialized_message": WIRE, "vendor": WIRE,
}


def rust_section(row):
    ours = row.get("ours")
    if ours:
        header = ours.get("header") or ""
        for prefix, name in RUST_CRATE_AUD:
            if header.startswith(prefix):
                return name
    theirs = row.get("theirs")
    if theirs:
        seg = (theirs.get("header") or "").replace("rclrs/src/", "").split("/")[0]
        return RCLRS_AUD.get(seg.replace(".rs", ""), CORE)
    return CORE


def owner(key):
    return key.split("::")[0] if "::" in key else key


# ---------------------------------------------------------------- extraction
def collect(langs):
    """Run the correlator and join each row to its ledger entry."""
    ap = load_api_parity()
    ledger = ap.load_ledger()
    out = {}
    with tempfile.TemporaryDirectory() as tmp:
        for lang in langs:
            rows, prov, _removed = ap.run_lang(lang, tmp)
            buckets = {r["key"]: r["bucket"] for r in rows}
            recs = []
            for r in rows:
                entry, inherited = ap.lookup(ledger, lang, r["key"], r["bucket"], buckets)
                entry = entry or {}
                topic = ap.row_topic(r)
                ours, theirs = r.get("ours"), r.get("theirs")
                rec = {
                    "key": r["key"],
                    "bucket": r["bucket"],
                    "verdict": entry.get("verdict") or "",
                    "why": entry.get("why") or "",
                    "provides": entry.get("provides") or [],
                    "ours": ours,
                    "theirs": theirs,
                }
                rec["s"] = state(rec)
                recs.append({
                    "k": r["key"],
                    "s": rec["s"],
                    "b": r["bucket"],
                    "v": rec["verdict"],
                    "og": sig(ours),
                    "tg": sig(theirs),
                    "oq": (ours or {}).get("qual") or "",
                    "tq": (theirs or {}).get("qual") or "",
                    "kind": (ours or theirs or {}).get("kind") or "",
                    "w": rec["why"],
                    "p": rec["provides"],
                    "i": 1 if inherited else 0,
                    "no": max(len((ours or {}).get("overloads") or []),
                              len((theirs or {}).get("overloads") or [])),
                    "g": (TOPIC_TITLE.get(topic, topic) if lang == "c"
                          else owner(r["key"])),
                    "sec": ("" if lang == "c"
                            else cpp_section(rec, topic) if lang == "cpp"
                            else rust_section(rec)),
                })
            out[lang] = {"rows": recs, "prov": prov}
    return out


# ---------------------------------------------------------------- rendering
TITLES = {
    "c": ("nano-ros C API vs rclc",
          "Every C entry point in nano-ros set against rclc and rcl, item for item."),
    "cpp": ("nano-ros C++ API vs rclcpp",
            "The nano-ros C++ surface against rclcpp, grouped by the type that owns each member."),
    "rust": ("nano-ros Rust API vs rclrs",
             "The nano-ros Rust surface against rclrs, grouped by who writes against it."),
}
NAVNAME = {"c": "C · rclc", "cpp": "C++ · rclcpp", "rust": "Rust · rclrs"}
LAYOUT = {"c": "flat", "cpp": "sectioned", "rust": "sectioned"}


def provenance_line(lang, prov):
    """Say which ROS 2 the page was compared against, from the recorded surface."""
    if lang == "c":
        ref = (prov.get("rclc_ref") or "")[:7]
        return "rclc + rcl · ROS 2 %s%s" % (
            prov.get("distro", "?"), (" · rclc " + ref) if ref else "")
    if lang == "cpp":
        return "rclcpp · ROS 2 %s" % prov.get("distro", "?")
    ref = (prov.get("ref") or "")[:7]
    return "rclrs %s%s" % (prov.get("version", "?"), (" · " + ref) if ref else "")


def nav(cur, urls):
    out = []
    for lang in ("c", "cpp", "rust"):
        if lang == cur:
            out.append('<span aria-current="page">%s</span>' % NAVNAME[lang])
        elif urls.get(lang):
            out.append('<a href="%s">%s</a>' % (html.escape(urls[lang]), NAVNAME[lang]))
        else:
            out.append("<span>%s</span>" % NAVNAME[lang])
    return "".join(out)


def render(lang, data, urls, stamp):
    shell = open(os.path.join(PAGES, "page.html")).read()
    css = open(os.path.join(PAGES, "page.css")).read()
    js = open(os.path.join(PAGES, "page.js")).read()
    rows = data["rows"]
    title, sub = TITLES[lang]
    counts = collections.Counter(r["s"] for r in rows)
    prov = provenance_line(lang, data["prov"])
    payload = {"lang": lang, "layout": LAYOUT[lang], "rows": rows,
               "counts": dict(counts), "prov": prov, "stamp": stamp}
    # `</` inside the embedded JSON would close the <script> early.
    blob = json.dumps(payload, ensure_ascii=False).replace("</", "<\\/")
    return (shell
            .replace("/*CSS*/", css)
            .replace("/*JS*/", js)
            .replace("__TITLE__", html.escape(title))
            .replace("__SUB__", html.escape(sub))
            .replace("__PROV__", html.escape(prov))
            .replace("__STAMP__", html.escape(stamp))
            .replace("__LANG__", lang)
            .replace("__TOTAL__", str(len(rows)))
            .replace("__NAV__", nav(lang, urls))
            .replace('"__DATA__"', blob)), counts


# ---------------------------------------------------------------- self-test
def self_test():
    """The derived parts must stay derived, and the embed must stay safe."""
    fails = []

    def check(cond, msg):
        if not cond:
            fails.append(msg)

    # state() is a pure function of bucket + verdict + provides
    check(state({"bucket": "same", "verdict": "", "provides": []}) == "same",
          "an exact correlation is not `same`")
    check(state({"bucket": "theirs-only", "verdict": "gap", "provides": []}) == "missing",
          "a gap is not `not implemented`")
    # the ordering that matters: an m-to-n arrow outranks the verdict, because a
    # row can be both a divergence and answered by three of our items
    check(state({"bucket": "differs", "verdict": "divergence",
                 "provides": ["nros::A"]}) == "remapped",
          "`provides` did not outrank the verdict")
    check(state({"bucket": "systematic", "verdict": "", "provides": []}) == "renamed",
          "a systematic transform is not `renamed`")
    check(state({"bucket": "ours-only", "verdict": "extension", "provides": []}) == "ours",
          "an extension is not `ours only`")
    # a gap must never be silently arrowed -- that would make the page assert
    # the opposite of its own chip
    check(state({"bucket": "theirs-only", "verdict": "gap", "provides": []}) != "remapped",
          "an unarrowed gap rendered as re-mapped")

    # signatures come from the parsed record, never from a name
    s = sig({"qual": "nros_take", "kind": "function", "overloads": [
        {"params": [{"name": "sub", "type": "nros_subscription_t *"}], "ret": "nros_ret_t"}]})
    check(s == "nros_ret_t nros_take(nros_subscription_t * sub)", "signature render: %r" % s)
    check(sig(None) == "", "a missing item did not render empty")
    check("+1 overload */" in sig({"qual": "f", "overloads": [
        {"params": [], "ret": "int"}, {"params": [], "ret": "int"}]}),
        "overload count not reported")

    # every state has a glyph, so the page is legible without colour
    check(len({g for _, _, g in STATES}) >= 4, "states are not glyph-distinguishable")
    check(len(STATES) == 7, "state count changed without updating the legend")

    # the embed escape must survive a ledger reason containing markup
    blob = json.dumps({"w": "see </script> and <b>"}, ensure_ascii=False).replace("</", "<\\/")
    check("</script>" not in blob, "embedded JSON can close the script tag")

    # templates must still carry every placeholder the renderer fills
    shell = open(os.path.join(PAGES, "page.html")).read()
    for token in ("/*CSS*/", "/*JS*/", "__TITLE__", "__SUB__", "__PROV__",
                  "__STAMP__", "__TOTAL__", "__NAV__", '"__DATA__"'):
        check(token in shell, "page.html lost the %s placeholder" % token)

    for f in fails:
        print("  FAIL: " + f)
    print("gen-api-comparison self-test: %d check(s) failed" % len(fails))
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "tmp/api-comparison"))
    ap.add_argument("--urls", help="JSON {lang: url} for the cross-page nav")
    ap.add_argument("--langs", default="c,cpp,rust")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    urls = json.load(open(args.urls)) if args.urls else {}
    stamp = os.environ.get("NROS_DOC_DATE") or datetime.date.today().isoformat()
    langs = [l.strip() for l in args.langs.split(",") if l.strip()]
    os.makedirs(args.out, exist_ok=True)

    data = collect(langs)
    for lang in langs:
        page, counts = render(lang, data[lang], urls, stamp)
        path = os.path.join(args.out, "api-%s.html" % lang)
        open(path, "w").write(page)
        print("%-5s %5d rows  %7.1f KB  %s" % (
            lang, len(data[lang]["rows"]), len(page) / 1024, path))
        print("      " + "  ".join(
            "%s=%d" % (k, counts.get(k, 0)) for k, _, _ in STATES))
    return 0


if __name__ == "__main__":
    sys.exit(main())
