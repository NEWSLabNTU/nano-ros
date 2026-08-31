#!/usr/bin/env python3
"""phase-403 W9 (issue 0965) - the per-kind CALLBACK SLOT COST has one
definition, and it matches the registration sites that actually claim a slot.

`nros ws entity-inventory` derives `NROS_EXECUTOR_MAX_CBS` by summing
`EntityKind::callback_slots()` over an image's declared entities. That function
is a MIRROR: the CLI is a host binary and `nros-node` is `no_std` and built for
the target, so it cannot read the executor's own accounting and has to restate
it. A mirror is acceptable only while something holds it to the definition -
which is the entire difference between this and a comment, and the reason
`check-infra-queryable-counts` exists one lane over.

The definition is `Executor::next_entry_slot()`. A registration that calls it
claims one entry in the table `MAX_CBS` sizes; one that does not, does not.

WHAT THIS GATE IS ACTUALLY DEFENDING. `create_publisher` never reaches
`next_entry_slot` - on the C++ path it writes an `RmwPublisher` into
caller-owned storage, and on the C path there is no `nros_executor_add_publisher`
to increment `handle_count`. So a publisher costs ZERO slots, and the island's
33 declared entities want 19. The mr-canhubk344 bring-up log recorded "33
handles" and set `MAX_CBS=36` from it. If a later change makes a publisher claim
a slot and this mirror is not moved with it, every image deriving its `MAX_CBS`
under-sizes by its publisher count - which is the silent-wrong-number failure
this whole campaign exists to remove.

Python rather than shell, for the reason `check-infra-queryable-counts` gives:
`check-gate-selftests`'s call detector requires parentheses, which a bash
function call never has.
"""

import os
import re
import shutil
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

MODEL = "packages/cli/nros-cli-core/src/entity_inventory.rs"
SPIN = "packages/core/nros-node/src/executor/spin.rs"
ACTION = "packages/core/nros-node/src/executor/action.rs"
C_API = "packages/api/nros-c/src/executor.rs"

# fn-name fragment -> entity kind, IN ORDER. `service_client` must be tried
# before `service`, and both action kinds before either, or a client is
# classified as a server and the gate agrees with a mirror it never checked.
CLASSIFY = [
    ("action_server", "action_server"),
    ("action_client", "action_client"),
    ("service_client", "service_client"),
    ("subscription", "subscription"),
    ("timer", "timer"),
    ("service", "service_server"),
    ("guard", "guard_condition"),
    ("publisher", "publisher"),
]

# Every kind the model knows. Restated here so a kind ADDED to the model with no
# registration site behind it is caught too - the direction that over-sizes,
# which is cheaper but still a number nothing backs.
KINDS = [
    "publisher",
    "subscription",
    "timer",
    "service_server",
    "service_client",
    "action_server",
    "action_client",
    "guard_condition",
]


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf8") as fh:
        return fh.read()


def declared_costs(text):
    """`EntityKind::callback_slots()` -> {kind: slots}.

    Parses the match arms rather than the whole file: the function is the one
    definition, and reading it structurally is what makes an edit to it visible
    here instead of only in a review.
    """
    m = re.search(
        r"pub fn callback_slots\(self\) -> usize \{(.*?)\n    \}", text, re.S
    )
    if not m:
        return None
    body = m.group(1)
    costs = {}
    # Two arm shapes: `EntityKind::X => N,` and a `|`-joined group sharing one
    # result. Both are normal Rust and both appear today.
    for arm in re.finditer(r"((?:\s*(?:\|\s*)?EntityKind::\w+)+)\s*=>\s*(\d+)", body):
        cost = int(arm.group(2))
        for name in re.findall(r"EntityKind::(\w+)", arm.group(1)):
            costs[camel_to_snake(name)] = cost
    return costs


def camel_to_snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def claiming_fns(text):
    """Every `fn` in `text` whose body reaches `next_entry_slot()`.

    Attributes each call to the nearest preceding `fn`, which is how the module
    is written (one claim per registration entry point, at the top of the body,
    before the fallible work).
    """
    out = []
    fns = [(m.start(), m.group(1)) for m in re.finditer(r"\n\s*(?:pub(?:\(crate\))? )?fn (\w+)", text)]
    for m in re.finditer(r"next_entry_slot\(\)", text):
        owner = None
        for pos, name in fns:
            if pos < m.start():
                owner = name
            else:
                break
        if owner:
            out.append(owner)
    return sorted(set(out))


def classify(fn):
    for frag, kind in CLASSIFY:
        if frag in fn:
            return kind
    return None


def check(root):
    """Return a list of problem strings (empty == pass)."""
    problems = []
    try:
        model = read(root, MODEL)
    except OSError as e:
        return [f"missing {MODEL}: {e}"]
    costs = declared_costs(model)
    if costs is None:
        return [
            f"`EntityKind::callback_slots` not found in {MODEL} - the per-kind "
            f"cost must have exactly one definition, and this gate reads it. "
            f"Fix the pattern; do not delete the check."
        ]
    missing = [k for k in KINDS if k not in costs]
    if missing:
        problems.append(
            f"{MODEL}: callback_slots() states no cost for {', '.join(missing)}. "
            f"Every kind needs one - an unstated kind is counted as zero by "
            f"nothing and as one by nobody."
        )

    sites = []
    for rel in (SPIN, ACTION):
        try:
            sites.extend(claiming_fns(read(root, rel)))
        except OSError as e:
            problems.append(f"cannot read {rel}: {e}")
    if not sites:
        return problems + [
            f"no `next_entry_slot()` call sites found in {SPIN} / {ACTION} - the "
            f"registration shape changed and this gate is now blind. Fix the "
            f"pattern; do not delete the check."
        ]

    claiming = set()
    for fn in sites:
        kind = classify(fn)
        if kind is None:
            problems.append(
                f"`{fn}` claims an executor callback slot and this gate cannot "
                f"tell which entity kind it is. Add it to CLASSIFY in this "
                f"script AND give the kind a cost in {MODEL} - a registration "
                f"nobody counts is a MAX_CBS smaller than the image needs."
            )
            continue
        claiming.add(kind)

    for kind in sorted(claiming):
        if costs.get(kind) == 0:
            problems.append(
                f"`{kind}` reaches Executor::next_entry_slot() but "
                f"callback_slots() says it costs 0 slots. Every image deriving "
                f"NROS_EXECUTOR_MAX_CBS is now short by its {kind} count, and "
                f"the failure is a boot-time ExecutorFull rather than a build "
                f"error."
            )
    for kind in KINDS:
        if kind in claiming:
            continue
        if costs.get(kind, 0) != 0:
            problems.append(
                f"`{kind}` costs {costs[kind]} slot(s) in {MODEL} and no "
                f"registration site in {SPIN} / {ACTION} claims one for it. "
                f"That over-sizes every derived MAX_CBS by its count; if the "
                f"kind really is free, say 0."
            )

    # The specific claim the island's measured 19-not-33 rests on, asserted
    # separately from the loop above so its failure names the consequence.
    if costs.get("publisher") != 0:
        problems.append(
            f"{MODEL} says a publisher costs {costs.get('publisher')} slot(s). "
            f"If that became true, say so here AND in "
            f"cmake/NanoRosEntityInventory.cmake and zephyr/Kconfig, both of "
            f"which state that a publisher claims none."
        )
    if any("publisher" in fn for fn in sites):
        problems.append(
            f"a publisher registration now reaches next_entry_slot() "
            f"({', '.join(fn for fn in sites if 'publisher' in fn)}). "
            f"callback_slots() must charge it, and the three places that state "
            f"'a publisher claims no slot' must be corrected with it."
        )

    # The C API keeps its own `handle_count`, capped at the same MAX_CBS. It is
    # a second accounting of one table, so it is checked separately: a publisher
    # that started incrementing it would under-size a C image while the Rust
    # sites above still read clean.
    try:
        c_api = read(root, C_API)
    except OSError:
        pass  # the C API is an optional member of some trees
    else:
        incs = [m.start() for m in re.finditer(r"handle_count \+= 1", c_api)]
        fns = [
            (m.start(), m.group(1))
            for m in re.finditer(r"\n\s*(?:pub )?(?:unsafe )?extern \"C\" fn (\w+)", c_api)
        ]
        if not incs:
            problems.append(
                f"no `handle_count += 1` sites found in {C_API} - the C API's "
                f"own accounting changed and this gate is now blind. Fix the "
                f"pattern; do not delete the check."
            )
        for pos in incs:
            owner = None
            for fpos, name in fns:
                if fpos < pos:
                    owner = name
                else:
                    break
            if owner and "publisher" in owner:
                problems.append(
                    f"{C_API}: `{owner}` increments handle_count, so a publisher "
                    f"DOES claim a slot on the C path. callback_slots() says 0."
                )
    return problems


# ---------------------------------------------------------------------------
# Self-test. On the NORMAL path, not behind a flag: a negative control nobody
# runs decays into a comment (`check-gate-selftests`).
# ---------------------------------------------------------------------------

MODEL_TMPL = """\
impl EntityKind {{
    pub fn callback_slots(self) -> usize {{
        match self {{
            EntityKind::Publisher => {pub_cost},
            EntityKind::Subscription
            | EntityKind::Timer
            | EntityKind::ServiceServer
            | EntityKind::ServiceClient
            | EntityKind::ActionServer
            | EntityKind::ActionClient
            | EntityKind::GuardCondition => 1,
        }}
    }}
}}
"""

SPIN_TMPL = """\
impl Executor {{
    pub fn register_subscription_buffered_on(&mut self) {{
        let slot = self.next_entry_slot()?;
    }}
    pub fn register_timer(&mut self) {{
        let slot = self.next_entry_slot()?;
    }}
    pub fn register_service_sized(&mut self) {{
        let slot = self.next_entry_slot()?;
    }}
    pub fn register_service_client_raw_sized_inner(&mut self) {{
        let slot = self.next_entry_slot()?;
    }}
    pub fn register_guard_condition(&mut self) {{
        let slot = self.next_entry_slot()?;
    }}
{extra}
}}
"""

ACTION_TMPL = """\
impl Executor {
    pub fn register_action_server_sized(&mut self) {
        let slot = self.next_entry_slot()?;
    }
    pub fn register_action_client_core(&mut self) {
        let slot = self.next_entry_slot()?;
    }
}
"""

C_TMPL = """\
pub unsafe extern "C" fn nros_executor_add_subscription() {{
    executor.handle_count += 1;
}}
{extra}
"""


def _write(root, pub_cost=0, spin_extra="", c_extra=""):
    for rel, body in (
        (MODEL, MODEL_TMPL.format(pub_cost=pub_cost)),
        (SPIN, SPIN_TMPL.format(extra=spin_extra)),
        (ACTION, ACTION_TMPL),
        (C_API, C_TMPL.format(extra=c_extra)),
    ):
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf8") as fh:
            fh.write(body)


def self_test():
    pub_site = """
    pub fn register_publisher_thing(&mut self) {
        let slot = self.next_entry_slot()?;
    }
"""
    unknown_site = """
    pub fn register_wormhole(&mut self) {
        let slot = self.next_entry_slot()?;
    }
"""
    c_pub = """
pub unsafe extern "C" fn nros_executor_add_publisher() {
    executor.handle_count += 1;
}
"""
    cases = [
        ((0, "", ""), 0, "a publisher costs 0 and claims nothing"),
        ((1, "", ""), 1, "the mirror started charging a publisher nothing charges"),
        ((0, pub_site, ""), 1, "a publisher registration started claiming a slot"),
        ((0, unknown_site, ""), 1, "an unclassifiable registration claims a slot"),
        ((0, "", c_pub), 1, "the C API started counting a publisher as a handle"),
    ]
    failures = 0
    tmp = tempfile.mkdtemp()
    try:
        for args, want, label in cases:
            root = os.path.join(tmp, "t")
            shutil.rmtree(root, ignore_errors=True)
            _write(root, *args)
            got = 1 if check(root) else 0
            if got != want:
                sys.stderr.write(f"  self-test FAIL: {label} - got {got}, want {want}\n")
                failures += 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        sys.stderr.write(f"check-entity-slot-costs self-test: FAILED ({failures})\n")
        sys.exit(1)
    print("check-entity-slot-costs self-test: OK")


def main():
    self_test()
    if "--self-test" in sys.argv:
        return
    problems = check(ROOT)
    if problems:
        sys.stderr.write(
            "check-entity-slot-costs: %d problem(s) - issue 0965:\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write(f"  - {p}\n")
        sys.exit(1)
    costs = declared_costs(read(ROOT, MODEL))
    sites = claiming_fns(read(ROOT, SPIN)) + claiming_fns(read(ROOT, ACTION))
    claiming = sorted({classify(f) for f in sites})
    print(f"  ok    {len(sites)} registration site(s) claim a callback slot")
    print(f"  ok    claiming kinds: {', '.join(claiming)}")
    print(f"  ok    free kinds: {', '.join(k for k in KINDS if costs.get(k) == 0)}")
    print("entity-slot-costs: callback_slots() agrees with next_entry_slot().")


if __name__ == "__main__":
    main()
