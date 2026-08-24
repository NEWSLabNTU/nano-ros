#!/usr/bin/env python3
"""Signature changes nano-ros makes SYSTEMATICALLY, and the constraint behind each.

Phase 379. Some divergences are not per-item decisions -- they are one decision
applied everywhere. `rcl` threads an `rcl_allocator_t *` through six entry
points; nano-ros has one global allocator, so it appears in none of them. That is
one sentence, not six ledger rows, and writing it six times is how the sentence
stops being read.

So a divergence that a rule explains is bucketed `systematic` and carries the
rule's constraint. Only what NO rule explains stays `differs` and needs a ledger
row of its own. The rules are stated once, here, with the constraint each answers.

# The rules are derived, not invented

Every rule below was read off the first report's 32 C-lane divergences, not
guessed in advance, and each names the entry points it covers so the claim can
be checked. A rule that stops matching anything is a rule to delete.

# Matching drops the FEWEST parameters that explain the difference

A rule says "this parameter class is absent on our side". Applying every rule at
once would over-explain: `rcl_client_init` takes both an options struct and a
node, our `nros_client_init` takes the node and not the options, and dropping
both leaves theirs one parameter SHORTER than ours -- a difference invented by
the explanation. So rules are applied one parameter at a time, in priority
order, stopping the moment the arities overlap. The report names the rules that
were actually needed.

A rule never satisfies `--check` on its own beyond marking the row `systematic`:
the constraint text is the argument, and if it does not hold, the row is a
defect the rule is hiding. Deleting a rule is the way to re-open every row it
covers at once.
"""

import re

# Ordered: the most clearly-forced classes first, so an explanation reaches for
# them before it reaches for anything arguable. `handle-owns-node` is last
# because a node parameter is usually legitimate on both sides.
THEIRS_DROPS = [
    {
        "id": "no-allocator",
        "type": re.compile(r"\brcl_allocator_t\b"),
        "constraint": (
            "one global allocator. `nros_platform_alloc` is the only allocation route "
            "in the tree (gated by check-no-direct-kernel-alloc), so a per-call "
            "allocator argument has nothing to vary -- it could only be passed the "
            "same value or a wrong one."
        ),
        "covers": "clock_init, executor_init, support_init, timer_init, make_node_a_lifecycle_node",
    },
    {
        "id": "compile-time-options",
        "type": re.compile(r"(_options_t\b|\brmw_qos_profile_t\b)"),
        "constraint": (
            "QoS and entity options are selected at COMPILE time (RFC-0036 'QoS "
            "subset ... selected at compile time'; RFC-0045 bakes boot config), so "
            "there is no runtime options struct to accept. Accepting one would "
            "promise a negotiation the backends do not perform."
        ),
        "covers": "publisher_init, subscription_init, service_init, client_init, action_client_init, action_server_init, guard_condition_init, node_init",
    },
    {
        "id": "no-argv",
        "type": re.compile(r"\bchar \*\*|\bint\b(?!\d)"),
        "applies_to": re.compile(r"^support_init$"),
        "constraint": (
            "an embedded image has no argc/argv. RFC-0045 resolves boot config from "
            "baked values and the board, so the entry point takes what a device can "
            "actually supply."
        ),
        "covers": "support_init",
    },
    {
        "id": "executor-owns-no-entity-storage",
        "type": re.compile(r"(\bvoid \*|_callback_t\b|\bsize_t\b)"),
        "applies_to": re.compile(r"^executor_add_"),
        "constraint": (
            "the callback and the message buffer are bound to the ENTITY at creation "
            "(RFC-0041, unified callback receive model), not handed to the executor "
            "when the entity is added. rclc's executor owns per-entity storage and "
            "must be told its size; ours does not, so it has nothing to be told."
        ),
        "covers": "executor_add_subscription, executor_add_service, executor_add_client, executor_add_guard_condition, executor_add_action_server, executor_add_action_client",
    },
    {
        "id": "handle-owns-node",
        "type": re.compile(r"\brcl(c)?_node_t\b"),
        "applies_to": re.compile(r"_fini$"),
        "constraint": (
            "our entity handles retain the node they were created on, so teardown "
            "does not ask the caller to still have it. Costs one pointer per entity "
            "and removes a lifetime the caller would otherwise have to enforce with "
            "no allocator and no ownership types to help."
        ),
        "covers": "publisher_fini, subscription_fini, service_fini, client_fini, action_server_fini, action_client_fini",
    },
]

# Parameters OUR side adds. Same shape, opposite direction.
OURS_DROPS = [
    {
        "id": "callback-bound-at-creation",
        "type": re.compile(r"(_callback_t\b|\bvoid \*ctx|\bvoid \*)"),
        "applies_to": re.compile(r"_init$"),
        "constraint": (
            "the mirror of executor-owns-no-entity-storage: what rclc passes to "
            "`rclc_executor_add_*`, we take at `*_init`. Same two values, one place "
            "instead of two, so an entity cannot exist in a state where it is "
            "subscribed and has no callback."
        ),
        "covers": "subscription_init, service_init, action_server_init, timer_init",
    },
    {
        "id": "status-return-out-param",
        "type": re.compile(r"&\s*$|\*\s*out\b"),
        "constraint": (
            "no exceptions (-fno-exceptions on Zephyr/FreeRTOS/bare-metal, RFC-0018) "
            "and no allocator, so a constructor cannot fail and a factory cannot "
            "return an owning pointer. The entity is an out-parameter and the return "
            "is the status."
        ),
        "covers": "the C++ create_* family",
    },
]


def _param_types(overload):
    return [p.get("type", "") for p in overload.get("params", [])]


def _try_drop(types, rules, key, arities_ok):
    """Drop matching parameters one at a time until `arities_ok(len(types))`.

    Returns (remaining_types, [rule ids used]) or (None, []) if no sequence of
    drops reaches an overlap. Minimal by construction: it stops at the first
    length that works, so a rule is never credited for a parameter that did not
    need explaining.
    """
    used = []
    remaining = list(types)
    if arities_ok(len(remaining)):
        return remaining, used
    for rule in rules:
        scope = rule.get("applies_to")
        if scope and not scope.search(key):
            continue
        i = 0
        while i < len(remaining):
            if rule["type"].search(remaining[i]):
                del remaining[i]
                if rule["id"] not in used:
                    used.append(rule["id"])
                if arities_ok(len(remaining)):
                    return remaining, used
                continue
            i += 1
    return None, used


def explain(key, ours_item, theirs_item):
    """[rule ids] if systematic rules reconcile the arities, else [].

    Both directions are tried: a parameter class we DROP (allocator, options)
    and one we ADD (a callback taken at creation, a status out-parameter).
    """
    ours_arities = {len(o.get("params", [])) for o in (ours_item or {}).get("overloads", [])}
    if not ours_arities:
        return []

    for overload in (theirs_item or {}).get("overloads", []):
        types = _param_types(overload)
        kept, used = _try_drop(types, THEIRS_DROPS, key, lambda n: n in ours_arities)
        if kept is not None:
            return used

    # The other direction: shrink OURS toward one of their arities.
    theirs_arities = {
        len(o.get("params", [])) for o in (theirs_item or {}).get("overloads", [])
    }
    if not theirs_arities:
        return []
    for overload in (ours_item or {}).get("overloads", []):
        types = _param_types(overload)
        kept, used = _try_drop(types, OURS_DROPS, key, lambda n: n in theirs_arities)
        if kept is not None:
            return used
    return []


def constraint(rule_id):
    for rule in THEIRS_DROPS + OURS_DROPS:
        if rule["id"] == rule_id:
            return rule["constraint"]
    return ""


def self_test():
    failures = []

    def item(*param_lists):
        return {
            "overloads": [
                {"params": [{"type": t} for t in params]} for params in param_lists
            ]
        }

    # `rcl_client_fini(client, node)` against `nros_client_fini(client)`.
    got = explain("client_fini", item(["struct nros_client_t *"]),
                  item(["rcl_client_t *", "rcl_node_t *"]))
    if got != ["handle-owns-node"]:
        failures.append("client_fini: got %r" % (got,))

    # `rcl_publisher_init` drops the options struct and KEEPS the node -- the
    # over-explanation guard: dropping both would leave theirs shorter.
    got = explain(
        "publisher_init",
        item(["struct nros_publisher_t *", "struct nros_node_t *",
              "struct nros_message_type_t *", "char *"]),
        item(["rcl_publisher_t *", "rcl_node_t *", "rosidl_message_type_support_t *",
              "char *", "rcl_publisher_options_t *"]),
    )
    if got != ["compile-time-options"]:
        failures.append("publisher_init: got %r" % (got,))

    # A genuine difference no rule covers must stay unexplained.
    got = explain("mystery_call", item(["int"]), item(["int", "int", "int", "int"]))
    if got:
        failures.append("an unexplained divergence was explained: %r" % (got,))

    # `applies_to` must actually scope: the node rule is for teardown, so it
    # must not silently explain an init that legitimately takes a node.
    got = explain(
        "node_only_init",
        item(["struct nros_x_t *"]),
        item(["rcl_x_t *", "rcl_node_t *"]),
    )
    if got:
        failures.append("handle-owns-node escaped its _fini scope: %r" % (got,))

    for rule in THEIRS_DROPS + OURS_DROPS:
        if not rule.get("constraint", "").strip():
            failures.append("rule %s has no constraint" % rule["id"])
        if not rule.get("covers", "").strip():
            failures.append("rule %s names no entry points" % rule["id"])

    return failures
