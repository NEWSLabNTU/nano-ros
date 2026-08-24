#!/usr/bin/env python3
"""Correlate a nano-ros API surface against the ROS 2 client library it mirrors.

Phase 379. Consumes the records `extract_cxx.py` / `extract_rust.py` produce.

# What "correlate" means here

The campaign's claim is that nano-ros is the SAME API as rclc / rclcpp / rclrs,
diverging only where an RTOS forces it. That claim is only checkable if every
item on either side has been ACCOUNTED FOR -- matched, or classified with a
reason. So this produces four buckets and no fifth:

  same       -- names correspond and the arguments agree
  systematic -- names correspond, the arguments differ, and a SIGNATURE RULE
                explains it: one platform decision applied everywhere (no
                allocator, compile-time QoS, callback bound at creation). The
                rule is stated once in `signature_rules.py`; the row inherits
                its constraint instead of needing a ledger entry of its own.
  differs    -- names correspond, the arguments differ, and NO rule explains it
                (the campaign's work list; each needs a decision, not a shrug)
  ours-only  -- we ship it and ROS 2 does not (an RTOS extension, or a name we
                invented where ROS 2 already had one)
  theirs-only-- ROS 2 ships it and we do not (a gap, or a deliberate decline)

`--check` fails on anything whose bucket is not `same` and has no row in the
ledger. A ledger row is a sentence somebody wrote; the point of the gate is that
no divergence can enter the tree without one.

# Why matching is by NORMALIZED NAME and not by an authored map

An authored map for ~700 items is a document nobody finishes and nobody
re-reads. Names already correspond by construction -- the project's stated goal
is that they do -- so the tool assumes correspondence and makes DISAGREEMENT the
thing a human has to write about. That inverts the labour onto exactly the rows
the campaign cares about.

Normalisation is per-language because each library spells the same idea its own
way:

  C++    `nros::Node::create_publisher` <-> `rclcpp::Node::create_publisher`
         -- drop the namespace, keep `Type::method`.
  C      `nros_publisher_init`          <-> `rclc_publisher_init`
         -- drop the library prefix, keep the rest verbatim.
  Rust   `nros::node::NodeCtx::create_publisher`
                                        <-> `rclrs::NodeState::create_publisher`
         -- drop the module path, and fold rclrs's `XState` naming (0.5+ made
            `Node = Arc<NodeState>`) onto the name a user actually writes.

A rename we CHOSE still needs saying out loud, so the ledger can also assert an
explicit pair; those are matched before normalisation is tried.
"""

import difflib
import re

import signature_rules


# rclrs 0.5 split every handle into `X` (an `Arc<XState>` alias) and `XState`
# (the inherent impl). A user writes `Node`; the methods live on `NodeState`.
# Folding the suffix is what makes `rclrs::NodeState::create_publisher` line up
# with `nros::NodeCtx::create_publisher` instead of reading as two gaps.
_RCLRS_STATE = re.compile(r"^(.*?)State$")

# rclcpp splits every entity into a type-erased base and a typed subclass:
# `Publisher<T>` IS-A `PublisherBase`, and half the methods a user calls
# (`get_topic_name`, `assert_liveliness`, `wait_for_service`, `cancel`) are
# declared on the base. Our `nros::Publisher` is one class, so without folding
# the suffix those methods report as an `ours-only` row and a `theirs-only` row
# that never mention each other -- inventing a divergence out of an inheritance
# split. Exactly the rclrs `XState` case one library over.
_RCLCPP_BASE = re.compile(r"^(.*?)Base$")

# Not every `*Base` is that split. These are real, separate rclcpp types whose
# names happen to end in Base, and folding them would merge two distinct APIs.
_BASE_KEEP = {"NodeBase", "MemoryStrategyBase", "AllocatorMemoryStrategyBase"}

# Our Rust node handle is `NodeCtx` for the reason RFC-0022 gives (no `Arc<Node>`;
# a short-lived borrow instead). It is the same entity as rclrs's `Node`, so the
# correlator must see through the name even though the name is deliberate.
TYPE_SYNONYMS = {
    "rust": {"NodeCtx": "Node", "NodeState": "Node"},
    "c++": {},
    "c": {},
}

# Longest first: `rclc_` must be stripped before `rcl_`, or every rclc symbol
# normalises to a stray leading `c_`.
LIB_PREFIXES = {
    "c": (("nros_", "NROS_"), ("rclc_", "RCLC_", "rcl_", "RCL_")),
}


def _strip_prefix(name, prefixes):
    for p in prefixes:
        if name.startswith(p):
            return name[len(p) :]
    return name


def _last_two(qual):
    """`a::b::Type::method` -> `Type::method`; `a::b::free_fn` -> `free_fn`."""
    parts = [p for p in qual.split("::") if p]
    return "::".join(parts[-2:]) if len(parts) >= 2 else (parts[-1] if parts else "")


def normalize(lang, side, qual, kind):
    """Map a qualified item name onto the language-neutral key used for matching."""
    if lang == "c":
        ours, theirs = LIB_PREFIXES["c"]
        return _strip_prefix(qual, ours if side == "ours" else theirs)

    parts = [p for p in qual.split("::") if p]
    if not parts:
        return qual
    # Drop the crate/library root; keep the tail that a user actually types.
    tail = parts[-1]
    owner = parts[-2] if len(parts) >= 2 else ""

    if lang == "c++":
        m = _RCLCPP_BASE.match(owner)
        if m and m.group(1) and owner not in _BASE_KEEP:
            owner = m.group(1)
        # The TYPE itself must fold too, not only a method's owner: `flatten`
        # builds each member key from its record's type key, so folding only
        # `owner` leaves `rclcpp::PublisherBase`'s methods keyed under
        # `PublisherBase::` and changes nothing.
        m = _RCLCPP_BASE.match(tail)
        if m and m.group(1) and tail not in _BASE_KEEP and kind in ("type", "enum", "alias"):
            tail = m.group(1)

    if lang == "rust":
        m = _RCLRS_STATE.match(owner)
        if m and m.group(1):
            owner = m.group(1)
        owner = TYPE_SYNONYMS["rust"].get(owner, owner)
        tail_syn = TYPE_SYNONYMS["rust"].get(tail)
        if tail_syn and kind in ("type", "enum", "alias"):
            tail = tail_syn

    if kind in ("type", "enum", "alias", "const", "macro"):
        # A type's identity is its own name; its module path is not part of the
        # API the way a method's owning type is.
        return tail
    return ("%s::%s" % (owner, tail)) if owner and owner[:1].isupper() else tail


def flatten(records, lang, side):
    """Records -> {key: item}, one entry per callable or type.

    A class contributes its own key AND one per public method, because a method
    is where the arguments live and arguments are the question.
    """
    out = {}
    for rec in records:
        kind = rec["kind"]
        key = normalize(lang, side, rec["qual"], kind)
        if kind in ("type", "enum"):
            out.setdefault(
                key,
                {
                    "key": key,
                    "kind": kind,
                    "qual": rec["qual"],
                    "values": rec.get("values"),
                },
            )
            for m in rec.get("members", []):
                if m.get("field"):
                    continue
                mkey = "%s::%s" % (key, m["name"])
                # Overloads collapse onto one key; the arity set is what the
                # report compares, so an overload difference still shows up.
                slot = out.setdefault(
                    mkey,
                    {
                        "key": mkey,
                        "kind": "method",
                        "qual": "%s::%s" % (rec["qual"], m["name"]),
                        "overloads": [],
                    },
                )
                slot["overloads"].append(
                    {
                        "params": m.get("params", []),
                        "ret": m.get("ret", ""),
                        "template": m.get("template", []),
                    }
                )
        elif kind == "function":
            slot = out.setdefault(
                key,
                {"key": key, "kind": "function", "qual": rec["qual"], "overloads": []},
            )
            slot["overloads"].append(
                {
                    "params": rec.get("params", []),
                    "ret": rec.get("ret", ""),
                    "template": rec.get("template", []),
                }
            )
        else:
            out.setdefault(key, {"key": key, "kind": kind, "qual": rec["qual"]})
    return out


# Type spellings that mean the same thing on both sides. These are NOT
# divergences to report -- reporting them would bury the real ones under
# hundreds of rows saying `std::string` differs from `const char *`, which
# RFC-0018 already decided once and for all.
_TYPE_NOISE = [
    (re.compile(r"\bconst\s+"), ""),
    (re.compile(r"\s*&\s*"), "&"),
    (re.compile(r"\s+"), " "),
    (re.compile(r"^rclcpp::"), ""),
    (re.compile(r"^nros::"), ""),
    (re.compile(r"^std::"), ""),
]


def canon_type(t):
    s = t or ""
    for pat, rep in _TYPE_NOISE:
        s = pat.sub(rep, s)
    return s.strip()


def arity_set(item):
    """Every parameter count this name accepts, DEFAULT ARGUMENTS INCLUDED.

    Arity, not full types, is the primary comparison: a type difference is
    usually RFC-0018's `std::string` -> `const char*` rule applied again, while
    an ARITY difference means the two APIs ask the user for different things.
    Full types are still reported alongside so a reader can judge.

    A defaulted parameter widens the range rather than fixing it. Counting only
    declared parameters reported `nros::Executor::spin(int32_t poll_ms = 10)` as
    diverging from `rclcpp::Executor::spin()` -- when `exec.spin()` compiles in
    both, which is the entire point of issue 0338's fix. A checker that flags a
    convergence someone deliberately made is worse than no checker.
    """
    out = set()
    for o in item.get("overloads", []):
        params = o["params"]
        required = sum(1 for p in params if not p.get("default"))
        for n in range(required, len(params) + 1):
            out.add(n)
    return out or {0}


def compare(ours, theirs, lang):
    """Bucket every key present on either side.

    `signature_rules` is consulted only after a plain arity comparison fails, so
    a rule can never turn an agreement into an explanation.
    """
    rows = []
    for key in sorted(set(ours) | set(theirs)):
        o = ours.get(key)
        t = theirs.get(key)
        if o and not t:
            rows.append({"key": key, "bucket": "ours-only", "ours": o, "theirs": None})
        elif t and not o:
            rows.append({"key": key, "bucket": "theirs-only", "ours": None, "theirs": t})
        else:
            oa, ta = arity_set(o), arity_set(t)
            if o["kind"] in ("method", "function") or t["kind"] in ("method", "function"):
                if oa & ta:
                    bucket = "same"
                    detail = None
                else:
                    rules = signature_rules.explain(key, o, t)
                    bucket = "systematic" if rules else "differs"
                    detail = {
                        "ours_arity": sorted(oa),
                        "theirs_arity": sorted(ta),
                        "rules": rules,
                    }
            else:
                bucket = "same"
                detail = None
            rows.append(
                {"key": key, "bucket": bucket, "ours": o, "theirs": t, "detail": detail}
            )
    return rows


def render_params(item):
    if not item or not item.get("overloads"):
        return ""
    shown = []
    for o in item["overloads"][:3]:
        shown.append("(" + ", ".join(canon_type(p["type"]) for p in o["params"]) + ")")
    return " | ".join(sorted(set(shown)))


# A rename is a naming difference the bucket report CANNOT show: it splits into
# an `ours-only` row and a `theirs-only` row that never mention each other. Since
# a rename with no platform reason is precisely what this campaign exists to
# find, the pairs are worth surfacing -- but by SIMILARITY, which is a guess.
#
# So these are printed as SUGGESTIONS and never as findings, and they never
# satisfy `--check`. A human confirms the pair and writes the ledger row; the
# tool's job is to stop the pair being invisible.
def suggest_renames(rows, cutoff=0.72):
    """[(ours_key, theirs_key, ratio)] for unmatched names that look alike."""
    ours_only = [r["key"] for r in rows if r["bucket"] == "ours-only"]
    theirs_only = [r["key"] for r in rows if r["bucket"] == "theirs-only"]
    out = []
    for key in ours_only:
        match = difflib.get_close_matches(key, theirs_only, n=1, cutoff=cutoff)
        if not match:
            continue
        ratio = difflib.SequenceMatcher(None, key, match[0]).ratio()
        out.append((key, match[0], ratio))
    out.sort(key=lambda x: -x[2])
    return out
