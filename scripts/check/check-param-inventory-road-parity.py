#!/usr/bin/env python3
"""issue 1436 -- the parameter inventory is a PEER of the entity inventory.

Two inventories are composed from one resolved SystemModel, and they answer
INDEPENDENT questions:

  * `EntityInventory::from_model`   -- what endpoints does this image create?
  * `ParamDeclarations::from_model` -- what parameters do its nodes declare?

A contract may answer either without the other. The cmake road says so in its
own words -- *"a model with no topics can still declare parameters"* -- and the
cargo road nested the parameter half inside the entity half's `Some` arm. The
entity predicate asked "does this describe WIRING?", which is NARROWER than the
question its own doc comment claims it asks ("did anybody author a contract?"),
so a contract declaring only `params:` answered `None`: the nested attach never
ran, every parameter fact was discarded, and the resolve failed naming wiring
the user was never asked for. The repo's canonical parameter fixture,
`packages/cli/nros-cli-core/tests/fixtures/param_declarations/`, is exactly that
shape -- and the tests beside it call `ParamDeclarations::from_model` DIRECTLY,
bypassing both roads, which is why nothing asked for two phases.

Two checks:

  A. `EntityInventory::from_model`'s emptiness predicate must test
     `node_params` alongside the other four terms. This is what makes `None`
     mean "no contract authored" rather than "no wiring described". It is the
     load-bearing invariant.

  B. `cmd::build`'s runtime backstop must stay. Nesting the attach inside the
     `Some` arm is ALLOWED -- it is safe exactly BECAUSE of (A) -- so the
     implication "(no inventory) implies (no parameters declared)" is what
     holds the road up, and a `debug_assert!` naming this issue pins it.
     Narrowing (A) again then fails loudly at the site that would otherwise
     discard the facts a second time.

**There is deliberately NO check of the form "every site composing an
`EntityInventory` also composes `ParamDeclarations`".** That was the first
shape and its rule is FALSE: `contract_join` composes an inventory to join
contract rows onto probe rows and has no business with parameters, and the unit
tests in `entity_inventory.rs` compose dozens more. A gate whose reach is wider
than its rule reports noise, and noise is how a real finding gets scrolled past.

The self-test at the bottom is a NEGATIVE CONTROL on the normal path (issue
1167 -- a guard that exists is not a guard that fires).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

INVENTORY = REPO / "packages/cli/nros-cli-core/src/entity_inventory.rs"
ROAD = REPO / "packages/cli/nros-cli-core/src/cmd/build.rs"

PARAM_CALL = "ParamDeclarations::from_model("

# The four the predicate always had, plus the one issue 1436 added.
PREDICATE_TERMS = (
    "model.structure.topics.is_empty()",
    "model.structure.services.is_empty()",
    "model.structure.actions.is_empty()",
    "model.contracts.node_paths.is_empty()",
    "model.contracts.node_params.is_empty()",
)


def check_predicate(text: str) -> list[str]:
    """(A) the emptiness predicate covers `node_params`."""
    m = re.search(
        r"pub fn from_model\(\s*source:.*?\n(.*?)\n\s*\{?\s*\n?\s*return None;",
        text,
        re.S,
    )
    if not m:
        return [
            f"{INVENTORY.relative_to(REPO)}: could not find "
            "`EntityInventory::from_model`'s emptiness predicate -- this gate "
            "cannot verify issue 1436's invariant, which is a failure, not a pass"
        ]
    body = m.group(1)
    missing = [t for t in PREDICATE_TERMS if t not in body]
    if missing:
        return [
            f"{INVENTORY.relative_to(REPO)}: `EntityInventory::from_model`'s "
            f"predicate does not test {', '.join(missing)}."
            "\n    `None` from this function must mean NOBODY AUTHORED A "
            "CONTRACT, not `no wiring described`. A contract declaring only "
            "`params:` is authored and states a real fact; dropping it from "
            "the predicate makes the cargo road discard every parameter fact "
            "and fail the resolve naming wiring (issue 1436)."
        ]
    return []


def check_backstop(text: str) -> list[str]:
    """(B) the runtime implication (A) licenses is pinned where it is relied on."""
    if "debug_assert!" in text and "issue 1436" in text and PARAM_CALL in text:
        return []
    return [
        f"{ROAD.relative_to(REPO)}: the cargo road composes "
        "`ParamDeclarations` inside the entity inventory's `Some` arm, which "
        "is safe ONLY while `from_model`'s predicate covers `node_params`."
        "\n    That implication must carry a `debug_assert!` naming issue "
        "1436, so narrowing the predicate fails loudly here instead of "
        "silently discarding every parameter fact a second time."
    ]


def self_test() -> None:
    """Both checks must FAIL on the shape they exist to catch."""
    good = INVENTORY.read_text()
    mutated = good.replace(
        "            && model.contracts.node_params.is_empty()\n", "", 1
    )
    assert mutated != good, (
        "self-test could not remove the `node_params` term -- the predicate "
        "moved and this gate's expectations are stale"
    )
    assert check_predicate(mutated), (
        "NEGATIVE CONTROL FAILED: the predicate check passed a body with no "
        "`node_params` term, which is issue 1436 exactly"
    )
    assert not check_predicate(good), (
        "the predicate check fails on the live tree; fix the finding it reports"
    )

    road = ROAD.read_text()
    assert not check_backstop(road), (
        "the backstop check fails on the live tree; fix the finding it reports"
    )
    assert check_backstop(road.replace("debug_assert!", "let _unused = ")), (
        "NEGATIVE CONTROL FAILED: the backstop check passed a road with its "
        "`debug_assert!` removed"
    )


def main() -> int:
    self_test()

    findings = check_predicate(INVENTORY.read_text())
    findings += check_backstop(ROAD.read_text())

    if findings:
        print("check-param-inventory-road-parity: FAIL\n")
        for f in findings:
            print(f"  {f}\n")
        return 1

    print(
        "check-param-inventory-road-parity: OK "
        f"(predicate covers all {len(PREDICATE_TERMS)} terms, including "
        "`node_params`; the cargo road's backstop is in place)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
