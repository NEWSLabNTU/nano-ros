#!/usr/bin/env python3
"""The topic each API item belongs to — the unit a feature is finished in.

Phase 379. The campaign closes the API a FEATURE at a time, across all three
languages together: node in C, C++ and Rust, then pubsub in all three, and so
on. A topic is therefore the thing an agent owns, a ledger shard is named after
one, and "complete" is a statement about a topic rather than about a language.

Splitting by language instead would let C++ pubsub land while C pubsub sits
unexamined, and the drop-in claim is made per language — a feature that works in
one is not a feature.

# The DECLARING HEADER decides first; the name is the fallback

A C API spells everything `lower_snake_t`, so a name pattern broad enough to
catch `nros_ret_t` also catches `rcl_bool_array_t`, `rcl_topic_endpoint_info_t`
and `rcl_jump_threshold_t` -- and files the YAML parameter parser, a graph query
and a clock callback under "types". That is what the first version did, and the
mistake is invisible in the counts.

The header says what the name cannot: `rcl_bool_array_t` is declared in
`rcl_yaml_param_parser/types.h` (param), `rcl_topic_endpoint_info_t` in
`rcl/graph.h` (graph), `rcl_jump_threshold_t` in `rcl/time.h` (timer). Every
record carries the file it was declared in, so the evidence is already there.

Names remain the fallback, for the headers no map should bother with -- rclcpp's
`utilities.hpp` holds `ok`, `shutdown` AND `spin`, which are two topics, and a
header map that pretended otherwise would be worse than the names.

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


# Directory fragments that settle a header before its basename is consulted:
# rclcpp_action's `server.hpp` and `client_goal_handle.hpp` are action, not
# service, and the basename alone says the opposite.
HEADER_DIRS = [
    ("rclcpp_action/", "action"),
    ("rcl_action/", "action"),
    ("rcl_yaml_param_parser/", "param"),
    ("rcl_lifecycle/", "lifecycle"),
    ("rclc_lifecycle/", "lifecycle"),
    ("rclc_parameter/", "param"),
]

# Basename stems, longest-first at lookup so `subscription_options` beats
# `subscription`. A header absent here falls back to the name patterns; that is
# the right answer for a header covering more than one topic.
HEADER_STEMS = {
    "publisher": "pubsub", "subscription": "pubsub", "loaned_message": "pubsub",
    "readonly_loaned_message": "pubsub", "serialized_message": "pubsub",
    "message_info": "pubsub", "create_publisher": "pubsub",
    "create_subscription": "pubsub", "generic_publisher": "pubsub",
    "generic_subscription": "pubsub", "create_generic_publisher": "pubsub",
    "create_generic_subscription": "pubsub",

    "service": "service", "client": "service", "create_service": "service",
    "create_client": "service", "service_info": "service",

    "action_client": "action", "action_server": "action",
    "action_goal_handle": "action", "goal_handle": "action",
    "goal_state_machine": "action",

    "parameter": "param", "parameter_value": "param", "parameter_client": "param",
    "parameter_service": "param", "parameter_map": "param",
    "parameter_event_handler": "param", "rclc_parameter": "param",

    "lifecycle": "lifecycle", "lifecycle_node": "lifecycle",
    "lifecycle_publisher": "lifecycle",
    "rcl_lifecycle": "lifecycle", "rclc_lifecycle": "lifecycle",
    "transition": "lifecycle", "managed_entity": "lifecycle",
    "default_state_machine": "lifecycle",

    "timer": "timer", "time": "timer", "clock": "timer", "rate": "timer",
    "sleep": "timer", "duration": "timer",

    "qos": "qos", "qos_event": "qos", "qos_overriding_options": "qos",
    "event": "qos", "event_callback": "qos",

    "executor": "exec", "executors": "exec", "executor_handle": "exec",
    "executor_options": "exec", "basic_executor": "exec", "worker": "exec",
    "wait": "exec", "wait_set": "exec", "waitable": "exec",
    "wait_result": "exec", "wait_set_runner": "exec",
    "callback_group": "exec", "guard_condition": "exec",

    "log_level": "log", "logger": "log", "logging": "log", "log_params": "log",

    "graph": "graph", "node_graph_interface": "graph",
    "network_flow_endpoint": "graph",

    "init": "init", "init_options": "init", "context": "init",
    "arguments": "init", "domain_id": "init",

    "node": "node", "node_options": "node", "node_impl": "node",

    "error_handling": "types", "allocator": "types", "error": "types",
    "data_types": "types",
    # rclrs's `RclPrimitive` is its executor's dispatch trait, not vocabulary.
    "rcl_primitive": "exec",

    "cdr": "serde",
    "boot_config": "boot", "app_config": "boot", "app_main": "boot",
}


# Our whole C surface is declared in ONE cbindgen output, `nros_generated.h`,
# so the header carries no topic and the name often does not either -- an
# `nros_accepted_callback_t` taking a goal handle is action's, and nothing about
# the string says so. These are authored, each resolved by reading the
# declaration, and they are few because everything else the name or the header
# already settles.
KEY_OVERRIDES = {
    # action callbacks: every one takes a goal uuid or an action server
    "accepted_callback_t": "action",
    "cancel_callback_t": "action",
    "cancel_return_code_t": "action",
    "result_callback_t": "action",
    # a service client's reply callback: (response bytes, len, ctx)
    "response_callback_t": "service",
    # QoS events raised on a subscription
    "count_status_t": "qos",
    "liveliness_changed_status_t": "qos",
    "event_liveliness_changed_cb_t": "qos",
    "event_subscriber_count_cb_t": "qos",
    "deadline_policy_t": "qos",
    # scheduling: RFC-0047's sched context, and the executor's wake state
    "sched_class_t": "exec",
    "sched_context_id_t": "exec",
    "sched_context_t": "exec",
    "sched_priority_t": "exec",
    "wake_state_t": "exec",
    # the typesupport handle a publisher/subscription is created with
    "message_type_t": "pubsub",
    "service_type_t": "service",
    "action_type_t": "action",
    "node_state_t": "node",
    "support_state_t": "init",
    # rclrs takes this straight from the rcl bindings, where nothing names the
    # feature; it is the id a service reply is correlated by.
    "rmw_request_id_t": "service",
    # CDR errors: the serde stage's vocabulary, not the general one.
    "SerError": "serde",
    "DeserError": "serde",
}


def topic_of(key, header=None):
    """The one topic an item belongs to. Total: everything gets exactly one.

    `header` is the file the declaration came from. When it is known and mapped,
    it wins -- see the module docstring for why a C name cannot be trusted here.
    """
    override = KEY_OVERRIDES.get(key)
    if override:
        return override
    if header:
        norm = header.replace("\\", "/")
        for fragment, topic in HEADER_DIRS:
            if fragment in norm:
                return topic
        stem = norm.rsplit("/", 1)[-1]
        for suffix in (".hpp", ".h", ".rs"):
            if stem.endswith(suffix):
                stem = stem[: -len(suffix)]
                break
        mapped = HEADER_STEMS.get(stem)
        if mapped:
            return mapped
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

    # The header outranks the name, which is the whole point of having it.
    check2 = lambda k, h, want: (
        None if topic_of(k, h) == want
        else failures.append("topic_of(%r, %r) = %r, want %r" % (k, h, topic_of(k, h), want))
    )
    check2("bool_array_t",
           "/opt/ros/humble/include/rcl_yaml_param_parser/rcl_yaml_param_parser/types.h",
           "param")
    check2("topic_endpoint_info_t", "/opt/ros/humble/include/rcl/rcl/graph.h", "graph")
    check2("jump_threshold_t", "/opt/ros/humble/include/rcl/rcl/time.h", "timer")
    check2("event_t", "/opt/ros/humble/include/rcl/rcl/event.h", "qos")
    check2("error_state_t", "/opt/ros/humble/include/rcl/rcl/error_handling.h", "types")
    # rclcpp_action's server.hpp is action; its basename alone says service.
    check2("Server::accept", "/opt/ros/humble/include/rclcpp_action/rclcpp_action/server.hpp",
           "action")
    # An unmapped header falls back to the name -- `utilities.hpp` holds `ok`,
    # `shutdown` AND `spin`, so no single mapping would be right.
    check2("spin", "/opt/ros/humble/include/rclcpp/rclcpp/utilities.hpp", "exec")
    check2("shutdown", "/opt/ros/humble/include/rclcpp/rclcpp/utilities.hpp", "init")
    # `on_shutdown` reads as init by name; ours is a lifecycle transition hook,
    # and its file says so.
    check2("on_shutdown", "packages/core/nros-node/src/lifecycle.rs", "lifecycle")

    check("accepted_callback_t", "action")
    check("response_callback_t", "service")
    check("sched_context_t", "exec")
    check("message_type_t", "pubsub")
    check("rmw_request_id_t", "service")
    check("SerError", "serde")
    # An override beats even a header, because the header here is one generated
    # file shared by the entire C surface.
    check2("wake_state_t", "/x/nros/nros_generated.h", "exec")

    # Totality: anything at all lands somewhere.
    if topic_of("zzz_no_pattern_matches_this") != "other":
        failures.append("an unmatched key did not fall through to `other`")

    return failures
