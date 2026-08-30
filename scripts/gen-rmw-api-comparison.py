#!/usr/bin/env python3
"""Generate the nano-ros RMW vs ROS 2 rmw comparison, as HTML.

THREE sources, no fourth:

  1. ROS 2's side   `docs/reference/rmw-implementation-signatures.txt`
                    (name / return / params / header, extracted from the Humble
                    headers by `rmw-api-inventory.py --signatures`)
  2. nano-ros' side `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`
                    for per-backend SLOTS, and the sibling ABI headers for
                    GLOBAL functions — the two are different things and the
                    table says which
  3. the reasons    `docs/reference/rmw-api-map.toml`, the authored map that
                    `just check rmw-api-parity` also reads. One file, two
                    consumers: two copies of a map is how a document ends up
                    describing a tree that moved.

Every row is one upstream symbol: what ROS 2 declares, what we provide, and the
reason when they differ — including when only the ARGUMENTS differ, which is the
case a name-only comparison silently passes.

  python3 scripts/gen-rmw-api-comparison.py           # rewrite
  python3 scripts/gen-rmw-api-comparison.py --check   # fail if it drifted
"""
import argparse
import html
import importlib.util as _util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOC = os.path.join(ROOT, "book", "src", "reference", "rmw-api-comparison.md")
ABI_DIR = os.path.join(ROOT, "packages", "core", "nros-rmw-abi", "include", "nros")


def _load(name, path):
    spec = _util.spec_from_file_location(name, os.path.join(ROOT, "scripts", path))
    mod = _util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _split_params(raw):
    """`['const rmw_publisher_t *publisher', 'size_t len']` — NAMES KEPT.

    The upstream extract drops parameter names deliberately (a renamed argument
    is not an ABI difference). Ours are kept because they are the only place the
    reader learns what the argument MEANS — `size_t` twice in a row is not a
    signature anyone can act on. Comparison still runs on TYPES; this is for the
    eye, not the diff.
    """
    raw = " ".join(raw.split())
    if raw in ("", "void"):
        return []
    out, depth, cur = [], 0, ""
    for ch in raw:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def vtable_slots_named():
    """{slot: [param text with names]} straight from the vtable header."""
    src = open(os.path.join(ABI_DIR, "rmw_vtable.h"), encoding="utf-8").read()
    body = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    body = re.sub(r"(?m)//.*$", " ", body)
    body = body[body.index("typedef struct nros_rmw_vtable_t {"):]
    body = body[: body.index("} nros_rmw_vtable_t;")]
    out = {}
    for m in re.finditer(r"\(\s*\*\s*([a-z_0-9]+)\s*\)\s*\(", body):
        depth, params = 1, ""
        for ch in body[m.end():]:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    break
            params += ch
        out.setdefault(m.group(1), _split_params(params))
    return out


def global_signatures():
    """{name: (ret, [params])} for the plain exported ABI functions.

    These are NOT vtable slots: they are defined once for the image rather than
    per backend, which is a real distinction for a reader deciding where a
    behaviour can vary. `rmw_qos_profile_check_compatible` is the example — its
    answer must not differ by backend, and its useful call sites may run before
    any backend registers.
    """
    out = {}
    for fn in sorted(os.listdir(ABI_DIR)):
        if not fn.endswith(".h"):
            continue
        src = open(os.path.join(ABI_DIR, fn), encoding="utf-8").read()
        src = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
        src = re.sub(r"(?m)//.*$", " ", src)
        # A declaration, not a slot: `ret name(params);` at file scope. Slots
        # are `ret (*name)(params)` and are excluded by the absence of `(*`.
        for m in re.finditer(
            r"(?m)^\s*([A-Za-z_][A-Za-z0-9_ ]*?[\w*])\s+((?:nros_rmw|rmw)_[a-z_0-9]+)\s*\(([^;{]*?)\)\s*;",
            src,
            re.S,
        ):
            ret, name, params = m.group(1).strip(), m.group(2), m.group(3)
            params = " ".join(params.split())
            out.setdefault(name, (ret, _split_params(params)))
    return out


def sig(ret, params):
    inner = ", ".join(params) if params else "void"
    return f"{ret} ({inner})"


CSS = """<style>
/* Scoped to this page. Colours follow mdBook's THEME CLASSES (`.coal`,
   `.navy`, `.ayu` are the dark ones) rather than `prefers-color-scheme`,
   because the book's theme is a reader choice, not an OS one. */
.rmwcmp{--ret:#8250df;--fn:#0550ae;--ty:#116329;--pu:#8b8b9a;
--del:#cf222e;--delbg:#ffebe9;--add:#0a7d33;--addbg:#e6ffec;--renbg:#fff8c5;
--line:var(--table-border-color,#ddd);--chip:var(--table-header-bg,#f2f2f7)}
.coal .rmwcmp,.navy .rmwcmp,.ayu .rmwcmp{--ret:#d2a8ff;--fn:#79c0ff;--ty:#7ee787;
--pu:#8b8b9a;--del:#ff7b72;--delbg:#3d1c1f;--add:#56d364;--addbg:#12301c;--renbg:#3a3018}
.rmwcmp table{table-layout:fixed;width:100%;border-collapse:collapse;margin:0}
.rmwcmp th{font-size:11px;text-transform:uppercase;letter-spacing:.06em;
text-align:left;padding:.5rem .7rem;background:var(--chip);border:none}
.rmwcmp td{padding:.65rem .7rem;border:none;border-top:1px solid var(--line);
vertical-align:top}
.rmwcmp td.c{width:29%}.rmwcmp td.why{width:42%;font-size:12.5px;opacity:.86}
.rmwcmp pre{margin:0;padding:0;background:none;border:none;
font:12.5px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;
white-space:pre-wrap;overflow-wrap:anywhere}
.rmwcmp .ret{color:var(--ret)}.rmwcmp .fn{color:var(--fn);font-weight:600}
.rmwcmp .ty{color:var(--ty)}.rmwcmp .pu{color:var(--pu)}
.rmwcmp .del{background:var(--delbg);color:var(--del);border-radius:3px;padding:0 .15em}
.rmwcmp .add{background:var(--addbg);color:var(--add);border-radius:3px;padding:0 .15em}
.rmwcmp .ren{background:var(--renbg);border-radius:3px;padding:0 .15em}
.rmwcmp .none{color:var(--del);font-weight:600;font-size:13px}
.rmwcmp .elsewhere{color:var(--fn);font-weight:600;font-size:13px}
.rmwcmp tr.inert td.c:nth-child(2){opacity:.5}
.rmwcmp .wrap{border:1px solid var(--line);border-radius:8px;overflow:hidden;margin:1rem 0}
</style>"""


def fmt_sig(ret, params, name, is_slot, dropped=(), added=(), renamed=False, types=None):
    """One signature, format A: return / name / one argument per line.

    Collapsed to a single line at zero or one argument, where the vertical form
    buys nothing.

    `is_slot` decides `(*name)` versus `name`, which is the whole notation for
    "per-backend vtable slot" versus "global function defined once" — the
    C syntax already says it, so nothing else has to.
    """
    e = html.escape
    nm = f"<span class='fn{" ren" if renamed else ""}'>{e(name)}</span>"
    head = f"<span class=pu>(*</span>{nm}<span class=pu>)</span>" if is_slot else nm

    def ty(i, p):
        # Diff on the TYPE (names are not ABI); render whatever `params` holds,
        # which on our side includes the argument name.
        key = types[i] if types else p
        cls = "ty del" if key in dropped else ("ty add" if key in added else "ty")
        return f"<span class='{cls}'>{e(p)}</span>"

    r = f"<span class=ret>{e(ret)}</span>"
    if not params:
        return f"{r}\n{head}<span class=pu>(</span><span class=ty>void</span><span class=pu>)</span>"
    if len(params) == 1:
        return f"{r}\n{head}<span class=pu>(</span>{ty(0, params[0])}<span class=pu>)</span>"
    body = "<span class=pu>,</span>\n".join("  " + ty(i, p) for i, p in enumerate(params))
    return f"{r}\n{head}<span class=pu>(</span>\n{body}\n<span class=pu>)</span>"


def arg_rules():
    """The systematic reasons an argument list differs, from the map."""
    import tomllib

    with open(os.path.join(ROOT, "docs", "reference", "rmw-api-map.toml"), "rb") as fh:
        return tomllib.load(fh).get("arg_rule", [])


def explain_args(up_params, our_params, rules):
    """([(title, why)], [unexplained]) for the arguments upstream has and we do not.

    Only upstream-side parameters are attributed: an argument we ADD is our
    shape and is described by the slot's own row, while one we DROP is a
    decision that needs a stated cause. An unexplained drop fails `--check`,
    because "the lists differ" is not a reason.
    """
    seen, unexplained = [], []
    for p in up_params:
        if p in our_params:
            continue
        hit = next((r for r in rules if r["match"] in p), None)
        if hit is None:
            unexplained.append(p)
        elif (hit["title"], hit["why"]) not in seen:
            seen.append((hit["title"], hit["why"]))
    return seen, unexplained


def build():
    parity = _load("_parity", "rmw-api-parity.py")
    shape = _load("_shape", "rmw-abi-shape.py")

    contract = parity.read_contract()
    upstream = shape.upstream_signatures()
    slots, rets = shape.vtable_slots()
    globs = global_signatures()
    slots_named = vtable_slots_named()
    kinds = parity._slot_kinds()
    rules = arg_rules()

    rows, unexplained = [], {}
    tally = {"same": 0, "redesigned": 0, "absent": 0}

    for sym in contract:
        where, detail = parity.MAP.get(sym, ("gap", ""))
        up_ret, up_params = upstream.get(sym, ("?", []))
        mechanical = sym[4:] if sym.startswith("rmw_") else sym

        our_html = ""
        status = "absent"
        causes = []
        inert = False
        note = detail if where in ("layer", "declined") else ""

        if where in ("vtable", "global"):
            if where == "vtable":
                name = re.split(r"[ ,(]", detail.strip())[0]
                inert = kinds.get(name) == "inert"
                our_ret, our_params = rets.get(name, "?"), slots.get(name, [])
                our_display = slots_named.get(name, our_params)
                is_slot = True
            else:
                name = sym
                our_ret, our_named = globs.get(sym, ("?", []))
                # Compare on types, show the names.
                our_params = [
                    " ".join(re.sub(r"\b[a-z_][a-z_0-9]*\s*(\[\s*\])?$", "", q).split())
                    for q in our_named
                ]
                our_display = our_named
                is_slot = False

            identical = our_params == up_params and our_ret == up_ret and name in (sym, mechanical)
            status = "same" if identical else "redesigned"
            renamed = name not in (sym, mechanical)
            dropped = [p for p in up_params if p not in our_params]
            added = [p for p in our_params if p not in up_params]
            if not identical:
                causes, missing = explain_args(up_params, our_params, rules)
                if missing:
                    unexplained[sym] = missing
            our_html = fmt_sig(
                our_ret, our_display, name, is_slot,
                dropped=(), added=added if not identical else (), renamed=renamed,
                types=our_params,
            )
            up_html = fmt_sig(
                up_ret, up_params, sym, False,
                dropped=dropped if not identical else (),
            )
        else:
            up_html = fmt_sig(up_ret, up_params, sym, False)

        tally[status] += 1
        rows.append({
            "sym": sym, "where": where, "up": up_html, "ours": our_html,
            "status": status, "causes": causes, "note": note, "inert": inert,
            "renamed_to": (
                re.split(r"[ ,(]", detail.strip())[0]
                if where == "vtable" and re.split(r"[ ,(]", detail.strip())[0] not in (sym, mechanical)
                else ""
            ),
        })

    return contract, rows, tally, sum(1 for r in rows if r["inert"]), unexplained


def render(contract, rows, tally, n_inert, _un):
    e = html.escape
    o = []
    w = o.append

    w("<!-- GENERATED by scripts/gen-rmw-api-comparison.py — do not edit by hand.")
    w("     Regenerate: python3 scripts/gen-rmw-api-comparison.py")
    w("     Gated by:   just check rmw-api-comparison -->")
    w("")
    w("# RMW API — every upstream symbol, side by side")
    w("")
    w("What ROS 2's `rmw` asks an implementation for, what nano-ros provides, and")
    w("the reason wherever the two differ. **Derived from three sources** — the ROS 2")
    w("signature extract, the nano-ros ABI headers, and the authored reason map")
    w("`docs/reference/rmw-api-map.toml` that `just check rmw-api-parity` also reads —")
    w("so a slot changing shape moves this page in the same commit or fails the gate.")
    w("")
    w("For the prose rationale behind the big divergences, see")
    w("[RMW API: Differences from upstream `rmw.h`](../design/rmw-vs-upstream.md).")
    w("")
    w("## How to read a row")
    w("")
    w("**The signature says which surface it is on.** nano-ros answers an upstream")
    w("symbol in one of two ways, and C syntax already distinguishes them:")
    w("")
    w("```c")
    w("rmw_ret_t (*count_publishers)(...)   // vtable slot — a function pointer,")
    w("                                     // may differ per backend")
    w("rmw_ret_t rmw_compare_gids_equal(...) // global — a plain exported function,")
    w("                                     // defined once for the image")
    w("```")
    w("")
    w("Slot names drop the `rmw_` prefix because that is genuinely the name in")
    w("`nros_rmw_vtable_t`; showing `rmw_count_publishers` on the right would flatter")
    w("the comparison.")
    w("")
    w("**Marks show the difference.** Red — upstream takes it, we do not. Green — we")
    w("take it, upstream does not. Yellow — the name differs from the mechanical one")
    w("(upstream minus `rmw_`). A row with **no marks is identical on both sides** and")
    w("carries no reason, because there is nothing to explain.")
    w("")
    w("**Argument names appear on the right only.** The ROS 2 side is read from a")
    w("signature extract that drops parameter names on purpose — a renamed argument is")
    w("not an ABI difference, and reporting one trains people to skim. Ours are kept")
    w("because they are the only place a reader learns what an argument *means*:")
    w("`size_t` twice in a row is not a signature anyone can act on. The comparison")
    w("itself runs on **types**, so a name never colours a row.")
    w("")
    w("**An empty right-hand cell** reads *rejected* when the symbol is deliberately")
    w("absent, or *answered elsewhere* when the capability ships outside the RMW seam —")
    w("in the executor, in codegen, or inside a backend. Both carry the reason.")
    w("")
    w("**A dimmed right-hand cell** is an *inert* slot: declared in the vtable, written")
    w("and read by nothing. A reserved shape, not a working capability (issue 0800).")
    w("")
    w("## What is being compared")
    w("")
    w("Not `rmw.h` against `rmw_vtable.h` — that comparison is wrong twice. Upstream")
    w("declares 177 `RMW_PUBLIC` functions, but most are utilities `rmw` itself")
    w("*defines*; an implementation links those, so comparing against 177 manufactures")
    w("~90 phantom gaps. And our vtable is only the backend seam: plenty of what")
    w("upstream calls rmw lives one layer up in the executor, one layer down in a")
    w("backend, or in codegen.")
    w("")
    w(f"So the contract is **empirical** — the {len(contract)} `rmw_*` symbols that")
    w("`librmw_fastrtps_cpp.so` and `librmw_zenoh_cpp.so` both define. Two independent")
    w("implementations with identical symbol sets is a better definition of \"what an")
    w("rmw must provide\" than any reading of the headers.")
    w("")
    w("| | count |")
    w("| --- | --- |")
    w(f"| identical on both sides | {tally['same']} |")
    w(f"| re-designed | {tally['redesigned']} |")
    w(f"| rejected or answered elsewhere | {tally['absent']} |")
    w(f"| …of the above, answered by an *inert* slot | {n_inert} |")
    w(f"| **contract total** | **{len(contract)}** |")
    w("")
    w("## Every contract symbol")
    w("")
    w(CSS)
    w('<div class=rmwcmp><div class=wrap><table>')
    w("<tr><th>ROS 2</th><th>nano-ros</th><th>reason</th></tr>")
    for r in rows:
        cls = " class=inert" if r["inert"] else ""
        w(f"<tr{cls}>")
        w(f"<td class=c><pre>{r['up']}</pre></td>")
        if r["ours"]:
            w(f"<td class=c><pre>{r['ours']}</pre></td>")
        elif r["where"] == "declined":
            w("<td class=c><span class=none>rejected</span></td>")
        else:
            w("<td class=c><span class=elsewhere>answered elsewhere</span></td>")
        bits = []
        if r["renamed_to"]:
            bits.append(f"<b>renamed</b> — the slot is <code>{e(r['renamed_to'])}</code>.")
        if r["inert"]:
            bits.append("<b>inert</b> — declared, written and read by nothing.")
        for title, why in r["causes"]:
            bits.append(f"<b>{e(title)}</b> — {e(why)}")
        if r["note"]:
            bits.append(e(r["note"]))
        w(f"<td class=why>{'<br><br>'.join(bits)}</td>")
        w("</tr>")
    w("</table></div></div>")
    w("")
    w("## Reproduce")
    w("")
    w("| command | asks |")
    w("| --- | --- |")
    w("| `just check rmw-api-parity` | is every contract symbol classified |")
    w("| `just check rmw-abi-shape` | does the vtable mirror it — name, args, return |")
    w("| `just check rmw-slot-producers` | which slots anything actually writes or reads |")
    w("| `python3 scripts/rmw-api-parity.py --contract` | re-derive the contract from an installed impl |")
    w("")
    return "\n".join(o) + "\n"


def self_test():
    """Negative control, on the NORMAL path.

    Exercises the shipping helpers rather than a copy of them: the signature
    formatter and the argument attribution are where a defect would silently
    produce a WRONG document — a row that reads "identical" because the diff
    never ran is worse than no document.
    """
    # A dropped argument must be attributed, and an unknown one must NOT be.
    rules = [{"match": "rcutils_allocator_t", "title": "t", "why": "w"}]
    causes, missing = explain_args(
        ["rcutils_allocator_t *", "struct wat_t *"], [], rules
    )
    if [t for t, _ in causes] != ["t"]:
        print("selftest: known parameter was not attributed", file=sys.stderr)
        return False
    if missing != ["struct wat_t *"]:
        print("selftest: unknown parameter was not reported", file=sys.stderr)
        return False

    # An argument present on both sides is neither dropped nor added.
    causes, missing = explain_args(["const char *"], ["const char *"], rules)
    if causes or missing:
        print("selftest: a matching parameter was treated as a difference", file=sys.stderr)
        return False

    # The slot form must be distinguishable from the global form — that is the
    # whole notation for "per-backend" vs "defined once".
    slot = fmt_sig("rmw_ret_t", [], "publish", True)
    glob = fmt_sig("rmw_ret_t", [], "rmw_publish", False)
    if "(*" not in slot or "(*" in glob:
        print("selftest: slot and global render the same", file=sys.stderr)
        return False

    # Zero and one argument collapse; two or more do not.
    if "\n  " in fmt_sig("rmw_ret_t", ["int a"], "x", True):
        print("selftest: a single argument was not collapsed", file=sys.stderr)
        return False
    if "\n  " not in fmt_sig("rmw_ret_t", ["int a", "int b"], "x", True):
        print("selftest: two arguments were not broken onto lines", file=sys.stderr)
        return False

    # A parameter NAME must never colour a row: the diff runs on `types`.
    marked = fmt_sig(
        "rmw_ret_t", ["const char *topic"], "x", True,
        added=["const char *"], types=["const char *"],
    )
    if "ty add" not in marked:
        print("selftest: diff did not use the parallel type list", file=sys.stderr)
        return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    if not self_test():
        print("gen-rmw-api-comparison: selftest failed — output is not trustworthy.", file=sys.stderr)
        return 1

    contract, rows, tally, n_inert, unexplained = build()
    if unexplained:
        print(
            "ERROR: %d slot(s) drop an upstream argument no rule explains:" % len(unexplained),
            file=sys.stderr,
        )
        for sym, params in sorted(unexplained.items()):
            print("  %-52s %s" % (sym, ", ".join(params)), file=sys.stderr)
        print(
            "\n  Add an `[[arg_rule]]` to docs/reference/rmw-api-map.toml naming the\n"
            "  parameter TYPE and what we do instead. \"The lists differ\" is not a reason.",
            file=sys.stderr,
        )
        return 1
    new = render(contract, rows, tally, n_inert, unexplained)

    if args.check:
        old = open(DOC).read() if os.path.exists(DOC) else ""
        if new != old:
            print(
                "ERROR: book/src/reference/rmw-api-comparison.md is stale — regenerate with\n"
                "  python3 scripts/gen-rmw-api-comparison.py\n"
                "and commit. It derives from the headers and the map the parity gate\n"
                "reads, so drift means one of those moved and the document did not.",
                file=sys.stderr,
            )
            subprocess.run(["git", "--no-pager", "diff", "--stat", "--", DOC], cwd=ROOT)
            return 1
        print(f"rmw-api-comparison OK ({len(contract)} symbols).")
        return 0

    with open(DOC, "w") as fh:
        fh.write(new)
    print(
        f"wrote book/src/reference/rmw-api-comparison.md — {len(contract)} symbols "
        f"({tally['same']} identical, {tally['redesigned']} re-designed, {tally['absent']} absent)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
