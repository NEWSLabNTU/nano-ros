#!/usr/bin/env python3
"""The topic each API item belongs to — the unit a feature is finished in.

Phase 379. The campaign closes the API a FEATURE at a time, across all three
languages together: node in C, C++ and Rust, then pubsub in all three, and so
on. A topic is therefore the thing an agent owns, a ledger shard is named after
one, and "complete" is a statement about a topic rather than about a language.

Splitting by language instead would let C++ pubsub land while C pubsub sits
unexamined, and the drop-in claim is made per language — a feature that works in
one is not a feature.

# Assignment is FIRST MATCH WINS, and the order encodes one real decision

`Node::create_publisher` could be filed under node or under pubsub. It is
pubsub: the verb exists to produce a publisher, and someone auditing "is our
publisher API complete" needs the way you obtain one. So the entity patterns are
matched BEFORE the node pattern, and `node` collects what is left on the node
itself — its name, its namespace, its graph queries, its logger.

The same rule puts `Node::declare_parameter` under param and
`Node::create_wall_timer` under timer. In every case the question "which feature
is incomplete without this?" decides, not which type declares it.

# `other` is visible on purpose

Every item lands in exactly one topic and `other` catches the residue. It is
reported rather than hidden, because a large `other` means this taxonomy is
wrong for part of the surface -- which is a finding, not a rounding error.
"""

import re

# Ordered. See the module docstring: entity patterns precede `node`.
#
# Patterns are case-SENSITIVE and spell both forms where both occur. Case
# folding looks tidier and files `Timer::cancel` under action, because action
# owns `cancel_` and `CancelGoal`. Precision here is cheaper than a taxonomy
# that is wrong for a few dozen rows in a way nobody notices.
TOPICS = [
    # Graph introspection first, and only its narrow spellings: `count_publishers`
    # contains `publisher` and pubsub would take it, but counting publishers is a
    # graph query -- nothing about the publisher API is incomplete without it.
    (
        "graph",
        re.compile(
            r"count_publishers|count_subscri|get_node_names|names_and_types|graph"
        ),
    ),
    # Actions next: `action_publish_feedback` contains `publish`, and pubsub
    # would take it otherwise.
    (
        "action",
        re.compile(
            r"Action|action_|Goal|goal_|Feedback|feedback_|publish_feedback"
            r"|CancelGoal|cancel_goal|cancel_request|cancel_response"
        ),
    ),
    (
        "pubsub",
        re.compile(
            r"Publisher|publisher|Subscription|subscription|LoanedMessage"
            r"|SerializedMessage|\bpublish|\btake\b|take_data|try_recv"
            r"|borrow_loaned|return_loaned"
        ),
    ),
    (
        "service",
        re.compile(
            r"Service|service|Client|client|send_reply|send_response|take_request"
        ),
    ),
    (
        "param",
        re.compile(r"Parameter|parameter|param_|\bparam\b|_parameters?\b"),
    ),
    ("lifecycle", re.compile(r"Lifecycle|lifecycle|transition|StateMachine")),
    (
        "timer",
        re.compile(
            r"Timer|timer|Rate|\brate\b|Clock|clock|\bTime\b|time_|Duration"
            r"|duration|sleep|wall_timer"
        ),
    ),
    (
        "qos",
        re.compile(
            r"QoS|qos|Reliability|Durability|History|Liveliness|Deadline|Lifespan"
        ),
    ),
    (
        "exec",
        re.compile(
            r"Executor|executor|Executable|CallbackGroup|callback_group|\bspin"
            r"|GuardCondition"
            r"|guard_condition|Waitable|WaitSet|wait_set|\bwait\b"
        ),
    ),
    ("log", re.compile(r"Logger|logger|Logging|logging|log_|\blog\b|Severity")),
    (
        "graph",
        re.compile(
            r"graph|count_publishers|count_subscri|get_node_names|Endpoint"
            r"|names_and_types|\bEvent\b"
        ),
    ),
    # The three below are checked AFTER every feature topic, so a feature keeps
    # its own callback typedef (`subscription_callback_t` is pubsub's) and only
    # what belongs to no feature lands here. They exist because the first run
    # put 316 rows in `other`, which this module calls a finding rather than a
    # rounding error -- and it was: `other` was three nameable things.
    (
        "serde",
        re.compile(r"^cdr_|\bCdr|serialize|deserialize|Serializer|Deserializer"),
    ),
    (
        "types",
        re.compile(
            r"_callback_t$|_t$|ErrorCode|RetCode|ReturnCode|\bResult\b|Expected"
            r"|\bFuture\b|Promise|Span|FixedString|FixedSequence|HeapString"
            r"|HeapSequence|Borrowed"
        ),
    ),
    (
        "boot",
        re.compile(
            r"BOOT_|BakedBoot|BootConfig|boot_config|BoardConfig|board_config"
            r"|app_main|app_config|main!"
        ),
    ),
    (
        "init",
        re.compile(
            r"^init$|^shutdown$|^ok$|Context|context_|InitOptions|support_|Support"
            r"|signal_handler|on_shutdown|ros_arguments"
        ),
    ),
    ("node", re.compile(r"Node|node_|\bnamespace\b|get_name|remap")),
]

# `dict.fromkeys` rather than a set: a topic may appear twice in TOPICS (graph
# does, narrow first and broad later) and the order here is the reporting order.
NAMES = list(dict.fromkeys([name for name, _ in TOPICS])) + ["other"]

# The order a stage is taken in. Earlier topics are the ones later ones are
# written in terms of: nothing can be complete before the entry point that
# creates it, and every entity is created on a node.
STAGE_ORDER = [
    "types",
    "init",
    "node",
    "pubsub",
    "service",
    "timer",
    "qos",
    "param",
    "action",
    "exec",
    "lifecycle",
    "log",
    "graph",
    "serde",
    "boot",
    "other",
]


def topic_of(key):
    """The one topic `key` belongs to. Total: every key gets exactly one."""
    for name, pattern in TOPICS:
        if pattern.search(key):
            return name
    return "other"


def self_test():
    failures = []

    def check(key, want):
        got = topic_of(key)
        if got != want:
            failures.append("topic_of(%r) = %r, want %r" % (key, got, want))

    # The ordering decision, stated as tests so changing the order breaks here
    # rather than silently re-filing hundreds of rows.
    check("Node::create_publisher", "pubsub")
    check("Node::create_subscription", "pubsub")
    check("Node::create_service", "service")
    check("Node::create_wall_timer", "timer")
    check("Node::declare_parameter", "param")
    check("Node::get_name", "node")
    check("Node::get_namespace", "node")
    check("Node", "node")

    check("Publisher::publish", "pubsub")
    check("Timer::cancel", "timer")
    check("ActionClient::send_goal", "action")
    check("ActionServer::handle_cancel", "action")
    check("Client::wait_for_service", "service")
    check("Subscription::take", "pubsub")
    check("publisher_init", "pubsub")
    check("subscription_fini", "pubsub")
    check("Service::send_response", "service")
    check("client_init", "service")
    check("ActionServer::accept", "action")
    check("action_publish_feedback", "action")
    check("QoS::reliability", "qos")
    check("Executor::spin", "exec")
    check("AnyExecutable", "exec")
    check("executor_add_subscription", "pubsub")
    check("Clock::now", "timer")
    check("Duration::seconds", "timer")
    check("Logger::get_child", "log")
    check("count_publishers", "graph")
    check("count_subscribers", "graph")
    check("get_topic_names_and_types", "graph")
    # ...but the narrow graph entry must not swallow the publisher API itself.
    check("Publisher::get_subscription_count", "pubsub")
    check("init", "init")
    check("shutdown", "init")
    check("support_init", "init")
    check("lifecycle_change_state", "lifecycle")
    check("cdr_read_f32", "serde")
    check("nros_ret_t", "types")
    check("Expected::value", "types")
    check("BOOT_SET_DOMAIN", "boot")
    check("BakedBootConfig::new", "boot")
    # A feature keeps its own typedef: `types` is checked after every feature.
    check("subscription_callback_t", "pubsub")
    check("timer_callback_t", "timer")

    if set(NAMES) != set(STAGE_ORDER):
        failures.append(
            "STAGE_ORDER and the topic list disagree: %r"
            % (set(NAMES) ^ set(STAGE_ORDER),)
        )

    # Totality: anything at all lands somewhere.
    if topic_of("zzz_no_pattern_matches_this") != "other":
        failures.append("an unmatched key did not fall through to `other`")

    return failures
