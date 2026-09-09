// nros-cpp: Node class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file node.hpp
 * @ingroup grp_node
 * @brief `nros::Node` and global session helpers.
 */

#ifndef NROS_CPP_NODE_HPP
#define NROS_CPP_NODE_HPP

#include <cstdint>
#include <cstddef>
#include <type_traits> // Phase 189.M3.3.e — SFINAE on the callback-style create_service
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdlib> // getenv — Phase 123.B.3 env-aware init
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio> // fopen — Phase 212.L.5 init_with_launch path-exists check
#endif
#endif

// Phase 118.D: ffi.h MUST come before qos.hpp so qos.hpp's
// `#ifndef NROS_CPP_FFI_H` guard sees the canonical types and skips
// its local redefinitions.
#include "nros_cpp_ffi.h"

#include "nros/result.hpp"
#include "nros/nros_cpp_config_generated.h"
#include "nros/qos.hpp"
// Phase 189.M3.1 — rclcpp-style named-options structs
// (`SubscriptionOptions` / `PublisherOptions`) used by the 4-arg
// `create_subscription` / `create_publisher` overloads below.
#include "nros/options.hpp"
// Phase 84.G8: heavy entity headers (publisher / subscription / service /
// client / action_server / action_client) are no longer pulled in here.
// Each entity header provides the out-of-line definition of its
// corresponding `Node::create_X<T>()` template and includes `node.hpp`
// itself. Consumers that #include `nros/nros.hpp` (the umbrella) still
// get every entity + every create method via that path; consumers that
// only want lightweight Node access can include this header directly
// and pay for only the light entities (timer, guard_condition,
// executor) below.
#include "nros/timer.hpp"
// Issue 0789 — `Node::get_clock()` / `Node::now()`. `clock.hpp` pulls in
// `time.hpp` and `duration.hpp`, so a node that stamps a header needs no
// further include.
#include "nros/clock.hpp"
// phase-427 W4 — `ComponentNode` is merged into `Node`, so the two things its
// header pulled in for the merged members come here with them:
//   * `log.hpp`  — `NROS_ERROR`, the OVERRIDABLE sink `report_component_failure`
//     routes through so a freestanding image can be given a boot diagnostic
//     (issue 1015).
//   * `declared_qos.hpp` — `nros::declared_depth` + `DECLARED_DEPTH_UNDECLARED`,
//     read by `Node::check_declared_depth` (phase-403 step 2).
// Neither includes a nano-ros header of its own, so neither can close a cycle
// back onto `node.hpp`. `component.hpp` — which DOES include this file — is why
// the member-pointer `create_subscription_in` / `create_subscription_in_group`
// family is DECLARED here and DEFINED there.
#include "nros/declared_qos.hpp"
#include "nros/log.hpp"
#include "nros/guard_condition.hpp"
#include "nros/executor.hpp"
// phase-417 stage 2b (RFC-0089) — `nros::TopicEndpointInfo` and the visitor
// typedef used by the graph forwarders below.
#include "nros/graph.hpp"
// Phase 273 (RFC-0047) — callback-group token (value type, no heap).
#include "nros/callback_group.hpp"
// phase-427 W1 declared the parameter facade here; phase-426 W4 took away the
// store it read. The hosted block no longer holds a `ParameterServer` — the
// facade forwards to the EXECUTOR's store — so this header needs
// `parameter.hpp` for nothing of its own. It is kept because `nros::Parameter`
// / `nros::ParameterServer` remain a public surface a consumer reaches through
// the node header (`examples/native/cpp/parameters` uses the class directly),
// and `parameter.hpp` depends on `result.hpp` and `<nros/parameter.h>` only,
// so there is no cycle.
#include "nros/parameter.hpp"

// `<chrono>` is the one capability whose predicate is not the uniform one —
// it needs `<ratio>` as a prerequisite too. The three measured corrections that
// established that live with the predicate, at the one detection site.
#include "nros/std_detect.hpp"

// ---------------------------------------------------------------------------
// phase-427 W1 — the ONE capability predicate the hosted node shape uses
// ---------------------------------------------------------------------------
//
// `rclcpp::Node`'s hosted signatures are spelled in `std::shared_ptr`,
// `std::string`, `std::vector` and `std::function`, so where those are absent
// the SIGNATURES are absent with them. That is a gate on METHODS, which the
// capability-layout rule permits; what it never gates is a MEMBER — every
// hosted-only member now lives out of line behind the unconditional
// `void* hosted_` below.
//
// The four probes are defined by the headers included above (`timer.hpp` for
// `<memory>` and `<functional>`, `options.hpp` for `<string>` and `<vector>`),
// so this conjunction is decidable here. It is the SAME conjunction `nros.hpp`
// used to wrap the whole shim class in; naming it once is what lets the class
// live in one place.
#if defined(NROS_CPP_HAS_SHARED_PTR) && defined(NROS_CPP_HAS_STD_STRING) &&                        \
    defined(NROS_CPP_HAS_STD_VECTOR) && defined(NROS_CPP_HAS_STD_FUNCTION)
#define NROS_CPP_NODE_HOSTED 1
#endif
// No `#include` here on purpose. Each of the four probes above is DEFINED only
// by the branch that already included the header it stands for (`timer.hpp` for
// `<memory>`/`<functional>`, `options.hpp` for `<string>`/`<vector>`), so an
// include in this block would be redundant — and it would sit outside an
// `NROS_CPP_STD` region, which `check-cpp-freestanding-includes` refuses for
// exactly the reason issue 0332 records: a hosted STL include a freestanding
// board cannot satisfy, reachable because a probe answered wrong.

// `NROS_RCLCPP_MAX_PARAMS` IS GONE (phase-426 W4). It sized an inline
// `nros::ParameterServer` on the merged node's hosted block — a SECOND
// parameter store, node-local, which `ros2 param get` could not see and a
// sibling node did not share. The member is deleted and the facade forwards to
// the executor's store (`nros/node_parameters.hpp`), so there is no per-node
// arena left for a number to bound.
//
// The image's one parameter arena is now `NROS_MAX_PARAMETERS` on
// `nros-params` (default 32, a build knob), and it is shared across every node
// on the executor rather than multiplied by the node count. Do not reintroduce
// a per-facade capacity: a second number is how a second store starts.

#ifdef NROS_RMW_CYCLONEDDS
extern "C" int32_t nros_rmw_cyclonedds_register(void);
#endif
#if defined(NROS_RMW_XRCE) || defined(NROS_RMW_XRCE_CFFI)
extern "C" int32_t nros_rmw_xrce_register(void);
#endif
#ifdef NROS_RMW_ZENOH_CFFI
extern "C" int32_t nros_rmw_zenoh_register(void);
#endif
#ifdef NROS_RMW_UORB
extern "C" int32_t nros_rmw_uorb_register(void);
#endif

// Issue #229 pin (cross-space, C++-FFI half): ErrorCode must stay
// value-identical to the NROS_CPP_RET_* codes (nros_cpp_ffi.h, included
// above) that every shim below feeds into Result().
static_assert(NROS_CPP_RET_NOT_FOUND == -4 && NROS_CPP_RET_ALREADY_EXISTS == -5 &&
                  NROS_CPP_RET_FULL == -6 && NROS_CPP_RET_NOT_INIT == -7 &&
                  NROS_CPP_RET_TRY_AGAIN == -14 && NROS_CPP_RET_REENTRANT == -15 &&
                  NROS_CPP_RET_UNSUPPORTED == -16,
              "nros_cpp_ret_t diverged from the shared numbering (issue #229)");

// Phase 84.G8: forward declarations of the heavy entity class
// templates. Full definitions live in the corresponding `*.hpp`,
// which also provide the out-of-line `Node::create_X<>` template
// bodies — consumers only pay for the entities they #include.
//
// phase-428 (RFC-0089): six of them are DECLARED in the upstream namespace,
// because that is where their definition now lives, and this header names them
// that way. The `nros::` migration alias for each is declared ONCE, beside its
// type in that type's own header -- repeating it here would give the alias two
// declaration sites, and `api-parity` attributes a name to the header it is
// first seen in.
namespace rclcpp {
template <typename M> class Publisher;
template <typename M> class Subscription;
template <typename S> class Service;
template <typename S> class Client;
} // namespace rclcpp

namespace rclcpp_action {
template <typename A> class Server;
template <typename A> class Client;
} // namespace rclcpp_action

namespace nros {

// Phase 122.3.d.b — L1 polling-mode action wrappers.
template <typename A> class PollingActionServer;
template <typename A> class PollingActionClient;
template <typename M> class PollingSubscription;

/// Executor-bound node handle the generated entry hands to a node constructor
/// (RFC-0044 §Design.1, merged onto `Node` by phase-427 W4).
///
/// Carries the opaque executor handle the node is created against — the same
/// pointer `nros::global_handle()` / `Node::executor_handle()` expose. The entry
/// obtains it post-`init` and placement-news the component with it.
///
/// This was `nros::ComponentNode`'s ctor parameter. `ComponentNode` is gone; the
/// handle is not, because construction against an EXPLICIT executor is a real
/// capability and the generated entry is its caller. It is the second of the
/// type's TWO constructors — RFC-0047's "one component, several named nodes"
/// does not exist (a component owns exactly one node; what the subnode packages
/// exercise is several named CALLBACK GROUPS), so there is no third.
struct NodeHandle {
    void* executor;
    constexpr NodeHandle() : executor(nullptr) {}
    explicit constexpr NodeHandle(void* exec) : executor(exec) {}
    constexpr bool valid() const { return executor != nullptr; }
};

/// Issue #227 — pass as `domain_id` to request an EXPLICIT domain 0. Plain
/// `0` is the UNSET sentinel (defers to `ROS_DOMAIN_ID` env on hosted, then
/// the baked `NROS_ENTRY_DOMAIN_ID` macro, then the default — the #206
/// model-A ladder), so a literal domain 0 is otherwise unreachable once the
/// image bakes a nonzero domain. Valid domains cap at 232, so 255 is
/// unambiguous. Mirrors `NROS_DOMAIN_ID_EXPLICIT_ZERO` in the C API
/// (nros_generated.h); hosted env still overrides it under model A.
// `constexpr` at namespace scope is implicitly const, so it already has internal
// linkage per TU — `inline` bought nothing and cost C++14 compatibility, which
// nano-ros otherwise keeps (see `just check cpp`'s freestanding c++14 syntax
// gate). PX4 builds every module with -std=gnu++14 -Werror, so an inline
// variable here made <nros/nros.hpp> uncompilable in a PX4 module (phase-325 W2).
constexpr uint8_t kDomainIdExplicitZero = 255;

namespace detail {

/// Boot-failure diagnostic (RFC-0044 Q2, refined in 242.4; moved off
/// `ComponentNode` onto `Node` by phase-427 W4).
///
/// A failed entity/param creation in a node constructor is unrecoverable on
/// firmware (boot is all-or-nothing) — but the constructor does not abort. It
/// records the `ok()` latch and the generated entry / single-node carrier checks
/// it post-construct, then halts boot **naming the failing node** via this
/// helper. Hosted builds also print to `stderr`; freestanding builds get the
/// overridable sink and nothing else. NOT `[[noreturn]]` — the caller decides
/// how to halt.
inline void report_component_failure(const char* node_name, const char* what, int32_t code) {
    // Route through the OVERRIDABLE sink first, so a freestanding image can
    // see this at all. Issue 1015's bisect ran aground here: on Zephyr both
    // arms below compile to nothing, so a component that failed to register
    // halted SILENTLY -- the board printed only the Zephyr banner, identical
    // to a healthy boot, and "is it broken?" had no answer on the target where
    // it mattered. Issue 0589 fixed exactly this class for the Rust side; the
    // C++ side still had it.
    NROS_ERROR("node \"%s\": FAILED at %s (code=%d)", (node_name != nullptr) ? node_name : "?",
               (what != nullptr) ? what : "?", static_cast<int>(code));
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    ::std::fprintf(stderr, "[nros] FATAL: node \"%s\" failed to construct at %s (code=%d)\n",
                   (node_name != nullptr) ? node_name : "?", (what != nullptr) ? what : "?",
                   static_cast<int>(code));
#endif
}

/// phase-403 step 2 — the code `set_error` records when a subscription's QoS
/// depth disagrees with the depth its system's contract declared.
///
/// Named after the phase rather than borrowed from `nros_cpp_ret_t`: this is
/// not a backend failure, it is the image contradicting its own manifest, and a
/// code that also means "the RMW said no" would send the reader to the wrong
/// half of the tree.
constexpr int32_t DECLARED_DEPTH_MISMATCH = -403;

/// The boot-time diagnostic for that disagreement — the runtime twin of the
/// `static_assert`, and it prints the same three facts: the topic, the depth
/// the declaration states, and the depth the code passed.
inline void report_declared_depth_mismatch(const char* node_name, const char* topic, int declared,
                                           int passed) {
    NROS_ERROR("node \"%s\": topic \"%s\" DECLARED depth %d but the QoS "
               "passed states %d. Depth multiplies the executor arena, so they must agree.",
               (node_name != nullptr) ? node_name : "?", (topic != nullptr) ? topic : "?", declared,
               passed);
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    ::std::fprintf(stderr,
                   "[nros] FATAL: node \"%s\": topic \"%s\" was DECLARED depth %d in "
                   "the contract sidecar (<stem>.contract.yaml) but the QoS passed to "
                   "create_subscription_in states depth %d. Depth multiplies the executor "
                   "arena (cost is (depth+1)*bound per subscription), so the two must agree. "
                   "Fix the contract row or the call site.\n",
                   (node_name != nullptr) ? node_name : "?", (topic != nullptr) ? topic : "?",
                   declared, passed);
#else
    (void)node_name;
    (void)topic;
    (void)declared;
    (void)passed;
#endif
}

/// Type-erased head of `Node`'s hosted block — phase-427 W1.
///
/// `Node::hosted_` is ONE unconditional `void*`, so `sizeof(Node)` cannot
/// follow a capability probe. But the pointee's type is hosted-only, and
/// `~Node()` is compiled in BOTH configurations, so the destructor cannot name
/// it: a `#if`-gated `delete` would give two translation units of one image two
/// different inline destructors, which is the ODR half of the same defect.
///
/// So the block carries its own destroyer. `~Node()` is byte-identical in every
/// configuration — it calls through this function pointer when the pointer is
/// non-null, and a freestanding TU never sets it, because nothing there can
/// allocate the block in the first place.
///
/// `Node::hosted_` stores the address of the BASE subobject (the cast is
/// written out at the allocation site), so the round trip is exact.
struct NodeHostedBase {
    void (*destroy)(void*);
};

#ifdef NROS_CPP_NODE_HOSTED
/// Every member that used to live on the separate hosted `rclcpp::Node`.
///
/// Allocated LAZILY, on the first hosted-shape call, and never on a
/// freestanding target — which is the property that makes one node type
/// affordable on an image with no allocator. A node constructed with
/// `Node("talker")` and driven through the out-ref `create_*` family never
/// touches `operator new`.
struct NodeHosted : NodeHostedBase {
    /// `rclcpp::Node::get_node_options()`. Ten of its accessors are
    /// REFUSE-LOUD (`options.hpp`); the object is stored so the getter can
    /// hand back what the constructor was given.
    ::rclcpp::NodeOptions options;

    /// Co-ownership of arena-registered services / clients / subscription
    /// callback cells / wall-timer cells. The executor arena holds a raw
    /// pointer as its dispatch context and has no unregister path, so the
    /// entity must outlive the node even if the caller drops the `shared_ptr`
    /// we handed back.
    ::std::vector<::std::shared_ptr<void>> owned_entities;

    static void destroy_fn(void* p) {
        delete static_cast<NodeHosted*>(static_cast<NodeHostedBase*>(p));
    }
    NodeHosted() { this->destroy = &NodeHosted::destroy_fn; }
};
#endif // NROS_CPP_NODE_HOSTED

} // namespace detail

/// Initialize an nros session.
///
/// Opens a middleware connection. Must be called before creating nodes.
/// Call `shutdown()` to clean up.
///
/// @param locator  Middleware locator (e.g., "tcp/127.0.0.1:7447"), or nullptr for default.
/// @param domain_id  ROS domain ID (0-232). `0` = unset (env > baked macro >
///                   default decide); `nros::kDomainIdExplicitZero` (255) =
///                   explicitly domain 0 (issue #227).
/// @return Result indicating success or failure.
inline Result init(const char* locator = nullptr, uint8_t domain_id = 0);

/// Initialize the nros session with an explicit session name.
///
/// `session_name` is the *process-wide* identifier used by the
/// XRCE-DDS RMW backend to derive a unique session key. Two
/// processes connecting to the same XRCE Agent MUST use distinct
/// session names — otherwise the agent treats them as the same
/// client and topic publishes don't cross-route. For zenoh / DDS
/// backends the value is informational only.
///
/// Pick a name that's stable for the process and distinct from
/// every other nros process you intend to share an agent with.
/// Typical choice: the process's primary node name (e.g.
/// `"talker"`, `"listener"`).
///
/// @param locator       Middleware locator, or nullptr for default.
/// @param domain_id     ROS domain ID (0-232); `0` = unset,
///                      `nros::kDomainIdExplicitZero` = explicit domain 0.
/// @param session_name  Per-process session identifier. Must not be nullptr.
/// @return Result indicating success or failure.
inline Result init(const char* locator, uint8_t domain_id, const char* session_name);

/// Issue 1050 defect (3) — `init` with an explicit RMW backend name.
///
/// `rmw` is the BAKED rung of RFC-0045's precedence model A: a hosted
/// `$NROS_RMW` still wins over it, and `nullptr` (or `""`) means "this image
/// names no backend", which resolves only when exactly one is registered.
///
/// Use it when the image knows which backend it wants and the registry cannot
/// be trusted to contain only that one. On a hosted target it cannot: a Rust
/// backend compiled into `libnros_cpp.a` registers from its `.init_array` ctor,
/// which runs BEFORE `main` and therefore before the generated
/// `nros_app_register_backends()` — so an archive carrying a backend the image
/// never declared registers first. That is how a PX4 module declaring
/// `BACKENDS uorb` came to open zenoh.
///
/// The plain `init` overloads reach this automatically when the build bakes
/// `NROS_ENTRY_RMW` (`nano_ros_entry(... RMW <name>)`); call it directly only
/// to choose at run time.
inline Result init_with_rmw(const char* rmw, const char* locator = nullptr, uint8_t domain_id = 0,
                            const char* session_name = "node");

/// Phase 212.L.5 Pattern 2 — launch-aware init.
///
/// Resolves runtime knobs (domain id, locator, RMW choice) in this order:
/// 1. `$NROS_RUNTIME_OVERLAY` (JSON sidecar emitted by
///    `nros launch --emit-runtime-overlay`). NOT yet consumed —
///    placeholder for the follow-up wave.
/// 2. Launch XML at `<CARGO_MANIFEST_DIR>/launch/*.xml`. NOT yet parsed —
///    the runtime trusts the launcher to project params/remaps into the
///    child env before exec().
/// 3. Env vars: `ROS_DOMAIN_ID`, `NROS_LOCATOR`, `RMW_IMPLEMENTATION` /
///    `NROS_RMW`. This is the active overlay channel today.
///
/// `argc` / `argv` are reserved for the structured `--ros-args` parse
/// that lands with the runtime-overlay wave. They are accepted and
/// ignored for forward-compat.
///
/// `session_name` falls back to `"nros_cpp"` when null (matches the
/// 2-arg `init` overload).
inline Result init_with_launch_auto(int argc = 0, char** argv = nullptr,
                                    const char* session_name = nullptr);

/// Phase 212.L.5 Pattern 2 — explicit-path variant of
/// [`init_with_launch_auto`].
///
/// Verifies `path` exists (so misspelled paths fail fast) but does NOT
/// yet parse the XML — the env overlay is the active source today. See
/// the auto variant's notes for the follow-up plan.
inline Result init_with_launch(const char* path, int argc = 0, char** argv = nullptr,
                               const char* session_name = nullptr);

/// Shut down the nros session.
///
/// Closes the middleware connection and frees all resources.
inline Result shutdown();

/// Node — the primary interface for creating ROS entities.
///
/// Mirrors `rclcpp::Node`. Entities (publishers, subscriptions, services,
/// etc.) are created through the node. The node holds a reference to the
/// parent executor session.
///
/// Usage:
/// ```cpp
/// nros::Node node;
/// NROS_TRY(nros::Node::create(node, "my_node"));
/// ```
/// The node — `rclcpp::Node`, and `nros::Node`, which are ONE TYPE
/// (phase-427).
///
/// Three shapes collapsed here: the freestanding out-ref node, the hosted
/// `shared_ptr` node that used to be a separate `rclcpp::Node` in `nros.hpp`,
/// and (from RFC-0044) the derivable component base. `rclcpp::Node` is declared
/// as an alias for this class at the bottom of this header, so
/// `std::is_same<rclcpp::Node, nros::Node>::value` is true and there is exactly
/// one set of entities, one arena registration path and one parameter facade.
///
/// WHY THE CLASS IS DEFINED IN `nros::` AND ALIASED INTO `rclcpp::`, rather
/// than the other way round as RFC-0089's end state describes. It is measured,
/// not stylistic: `scripts/api-parity.py` extracts the NATIVE C++ surface with
/// namespace root `{"nros"}` and the PORTED surface with `{"rclcpp", ...}`,
/// and only the native bucket is gated by `just check api-parity`. Moving the
/// class definition into `rclcpp` empties `nros::Node::*` from the native
/// surface, so every one of its ~60 members re-buckets to `theirs-only` with no
/// ledger row and the gate goes red — a namespace question answered by a
/// measurement tool's roots. Every other type in this API is already spelled
/// this way (`rclcpp::Publisher`, `rclcpp::QoS`, `rclcpp::Timer`,
/// `rclcpp::Clock` are all aliases of `nros::` definitions), so this is the
/// consistent shape as well as the affordable one. Flipping it is the same
/// change as phase-428's whole-tree `nros::` -> `rclcpp::` migration, and
/// belongs with it.
///
/// LAYOUT IS PROBE-INDEPENDENT (phase-427 W1, gate
/// `check-cpp-capability-layout`). Every member below is unconditional; the
/// hosted-only state lives behind `hosted_`. A capability probe may gate a
/// METHOD — and many below are gated — but it may never change `sizeof`. Two
/// TUs of one image disagreeing about `<memory>` is a SUPPORTED state here
/// (px4 sets `-DNROS_CPP_STD` on one module; the Zephyr cyclone module adds a
/// `cxx-compat` include dir for some targets only), and the shim's
/// `std::vector<std::shared_ptr<detail::WallTimer>> timers_` already shipped
/// that bug once.
class Node {
  public:
    /// Default constructor — creates an uninitialized node. Pair with
    /// [`init`].
    Node()
        : handle_(), initialized_(false), executor_handle_(nullptr), clock_(NROS_CLOCK_ROS_TIME),
          hosted_(nullptr) {}

    // ==== phase-427 W2 — construction ======================================
    //
    // Upstream constructs a node from a name and THROWS on failure. RFC-0018
    // forbids exceptions here, so the constructor is kept [clause 2] and the
    // failure channel changes [clause 1]: the node records the failure and
    // `ok()` reports it.

    /// `rclcpp::Node("talker")` — upstream's shape, no allocator, every target.
    ///
    /// Opens on the GLOBAL executor `rclcpp::init()` created (issue 0465 — one
    /// session per image). ADOPT-BOUNDED: upstream throws where this leaves
    /// `ok()` false, which is weaker, so it has to be LOUD by other means —
    /// the generated entry checks `ok()` and halts naming the node (RFC-0044
    /// Q2), and a hand-written `main` must check it. The book says so; the
    /// compiler cannot.
    explicit Node(const char* name, const char* ns = nullptr)
        : handle_(), initialized_(false), executor_handle_(nullptr), clock_(NROS_CLOCK_ROS_TIME),
          hosted_(nullptr) {
        (void)this->init(name, ns);
    }

    /// Explicit-code construction — the channel a `-fno-exceptions` target
    /// needs in place of a throwing constructor. Pair with `Node()`.
    ///
    /// Re-initialising an already-initialised node is `AlreadyExists` rather
    /// than a silent re-open.
    Result init(const char* name, const char* ns = nullptr) {
        if (initialized_) return Result(ErrorCode::AlreadyExists);
        if (!Node::global_initialized()) return Result(ErrorCode::NotInitialized);
        executor_handle_ = Node::global_storage();
        return Node::create(*this, name, ns);
    }

    /// Construct on an EXPLICIT executor handle rather than the global one —
    /// what a generated entry writes, and what `nros::ComponentNode`'s
    /// `NodeHandle` constructor used to be (RFC-0089 §"What this means for the
    /// merge": construction is not identity).
    Result init_on(void* executor_handle, const char* name, const char* ns = nullptr) {
        if (initialized_) return Result(ErrorCode::AlreadyExists);
        if (executor_handle == nullptr) return Result(ErrorCode::NotInitialized);
        executor_handle_ = executor_handle;
        return Node::create(*this, name, ns);
    }

    /// Construct against an EXPLICIT executor-bound handle — what a generated
    /// entry writes, and what `nros::ComponentNode(NodeHandle, name)` was
    /// before phase-427 W4 merged it here.
    ///
    /// This is the SECOND of the type's two constructors. On a null handle or a
    /// creation failure it latches the error rather than aborting: the entry
    /// checks `ok()` post-construct and halts naming this node (RFC-0044 Q2).
    explicit Node(NodeHandle handle, const char* name, const char* ns = nullptr)
        : handle_(), initialized_(false), executor_handle_(nullptr), clock_(NROS_CLOCK_ROS_TIME),
          hosted_(nullptr) {
        if (!handle.valid()) {
            this->set_error("ctor (null executor handle)", -1);
            return;
        }
        Result r = this->init_on(handle.executor, name, ns);
        if (!r.ok()) {
            this->set_error("node create", r.raw());
        }
    }

    /// `rclcpp::ok()`'s spelling, asked of one node: did this node come up?
    ///
    /// Replaces upstream's throw. See the constructor for the envelope.
    ///
    /// phase-427 W4 — this now answers for the LATCH as well as for creation.
    /// A node whose `create_*` failed after a successful open is not ok, which
    /// is what the generated entry's post-construct check has always meant.
    ///
    /// Issue #230 — SMP safety. The failure state is tracked as a `has_error_`
    /// flag whose HEALTHY value is the zero-initialized default, so a reader on
    /// a different core than the constructing one (ASI FVP SMP-4) always sees a
    /// correct "ok" even before the constructor's stores propagate — no spurious
    /// "failed at ? (code=0)" boot line. A real failure is published with a
    /// **release** store and read here with an **acquire** load, so the
    /// dangerous direction (silently MISSING a failure) is closed too.
    bool ok() const { return initialized_ && !__atomic_load_n(&has_error_, __ATOMIC_ACQUIRE); }

    /// The site of the first failure (`"create_publisher_in"`, `"node create"`,
    /// …), or `nullptr` when no failure was latched. For the boot diagnostic.
    const char* error_what() const {
        // Acquire-fence on the flag so a standalone call (not preceded by
        // ok()) still sees the released `error_what_` write (issue #230).
        (void)__atomic_load_n(&has_error_, __ATOMIC_ACQUIRE);
        return error_what_;
    }
    /// The raw error code of the first latched failure, or `0`.
    int32_t error_code() const {
        (void)__atomic_load_n(&has_error_, __ATOMIC_ACQUIRE);
        return error_code_;
    }

    // ==== phase-427 W1/W3 — the HOSTED shape ================================
    //
    // Upstream's own signatures, verbatim, on the same type. Everything below
    // is spelled in `std::shared_ptr` / `std::string` / `std::vector` /
    // `std::function`, so where those are absent the SIGNATURES are absent with
    // them — a gate on METHODS, never on a member.
    //
    // ONE NAME, TWO SIGNATURES (RFC-0089 §"Entity creation"). The out-ref
    // family above and the `shared_ptr` family below are OVERLOADS, and the
    // compiler separates them without ambiguity because the hosted forms put
    // `M` only in the return type — so a call that passes an out-param deduces
    // nothing for them and they drop out, while an explicit
    // `create_publisher<M>("chatter", 10)` cannot bind an out-ref parameter to
    // a string literal. On a FREESTANDING target the hosted overload does not
    // exist at all, so that same ported line fails to compile naming the
    // out-ref overload — mechanical, which is what clause 2 asks for.
#ifdef NROS_CPP_NODE_HOSTED
    using SharedPtr = ::std::shared_ptr<Node>;

    /// `std::make_shared<rclcpp::Node>("talker")` — upstream's constructor.
    explicit Node(const ::std::string& name) : Node(name.c_str(), nullptr) {}

    /// Upstream's `(name, options)` constructor.
    Node(const ::std::string& name, const ::rclcpp::NodeOptions& options)
        : Node(name.c_str(), nullptr) {
        this->hosted().options = options;
    }

    /// Upstream's `(name, namespace, options)` constructor.
    Node(const ::std::string& name, const ::std::string& ns,
         const ::rclcpp::NodeOptions& options = ::rclcpp::NodeOptions())
        : Node(name.c_str(), ns.c_str()) {
        this->hosted().options = options;
    }

    /// `rclcpp::Node::get_node_options()`.
    const ::rclcpp::NodeOptions& get_node_options() const { return this->hosted().options; }

    /// `rclcpp::Node::shared_from_this()`.
    ///
    /// ADOPT-BOUNDED, and this is the one place the merge had to weaken a
    /// contract rather than keep it. Upstream gets this from
    /// `std::enable_shared_from_this<Node>`, a BASE CLASS carrying a
    /// `std::weak_ptr` member — 16 bytes of layout that exist only where
    /// `<memory>` does, which is precisely what the capability-layout rule
    /// forbids and what the deleted `timers_` member already shipped once.
    ///
    /// So the base is gone and the verb is a method. The returned pointer
    /// ALIASES `this` with an EMPTY owner: it observes the node and does not
    /// extend its lifetime, where upstream's shares ownership. In this API the
    /// node is constructed by a generated entry (or by a `main`) and outlives
    /// everything it is handed to, so the two behave the same — but a caller
    /// who stores it past the node's scope gets a dangling pointer where
    /// upstream would have kept the node alive. Upstream also THROWS
    /// `bad_weak_ptr` when the node is not already owned by a `shared_ptr`;
    /// this never throws, which is the RFC-0018 direction.
    ::std::shared_ptr<Node> shared_from_this() {
        return ::std::shared_ptr<Node>(::std::shared_ptr<void>(), this);
    }
    /// Const overload of [`shared_from_this`].
    ::std::shared_ptr<const Node> shared_from_this() const {
        return ::std::shared_ptr<const Node>(::std::shared_ptr<void>(), this);
    }

    /// The shim's own `initialized()` — the same answer as [`ok`] and
    /// [`is_valid`]. Kept so a file written against the pre-merge
    /// `rclcpp::Node` still compiles.
    bool initialized() const { return initialized_; }

    /// The shim's `nros_node()` escape hatch. There is one node type now, so
    /// this is the identity; kept for the same reason `initialized()` is.
    Node& nros_node() { return *this; }
    /// Const overload of [`nros_node`].
    const Node& nros_node() const { return *this; }

    // -- entity creation, upstream's signatures (bodies in `nros.hpp`) -------

    /// `create_publisher<M>(topic, qos)` — upstream's shape.
    template <typename M>
    ::std::shared_ptr<::rclcpp::Publisher<M>> create_publisher(const ::std::string& topic,
                                                               const QoS& qos);

    /// `create_publisher<M>(topic, depth)` — the integer-depth spelling.
    template <typename M>
    ::std::shared_ptr<::rclcpp::Publisher<M>> create_publisher(const ::std::string& topic,
                                                               ::size_t depth);

    /// `create_subscription<M>(topic, qos, callback)` — upstream's shape.
    /// Accepts ANY callable; the executor dispatches it.
    template <typename M, typename Cb>
    ::std::shared_ptr<::rclcpp::Subscription<M>> create_subscription(const ::std::string& topic,
                                                                     const QoS& qos, Cb cb);

    /// `create_subscription<M>(topic, depth, callback)`.
    template <typename M, typename Cb>
    ::std::shared_ptr<::rclcpp::Subscription<M>> create_subscription(const ::std::string& topic,
                                                                     ::size_t depth, Cb cb);

#ifdef NROS_CPP_HAS_STD_CHRONO
    /// `create_wall_timer(period, callback)` — upstream's shape.
    template <typename Rep, typename Period, typename Cb>
    ::std::shared_ptr<Timer> create_wall_timer(::std::chrono::duration<Rep, Period> period, Cb cb);
#endif

    /// Poll-style service server (`create_service<S>(name, qos)`). Not an
    /// upstream signature — upstream requires a callback — so it claims
    /// nothing. Drain with `service->take_request(...)`.
    template <typename S>
    ::std::shared_ptr<::rclcpp::Service<S>> create_service(const ::std::string& name,
                                                           const QoS& qos = QoS::services());

    /// Callback-style service server (`void(const S::Request&, S::Response&)`).
    template <typename S, typename F,
              typename = typename std::enable_if<std::is_convertible<
                  F, void (*)(const typename S::Request&, typename S::Response&)>::value>::type>
    ::std::shared_ptr<::rclcpp::Service<S>> create_service(const ::std::string& name, F callback,
                                                           const QoS& qos = QoS::services());

    /// **REFUSED** — upstream's `shared_ptr` handler shape. See
    /// `NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK`.
    template <typename S, typename F,
              typename = typename std::enable_if<
                  !::rclcpp::detail::is_qos_arg<F>::value &&
                  !std::is_convertible<F, void (*)(const typename S::Request&,
                                                   typename S::Response&)>::value>::type,
              typename = void>
    ::std::shared_ptr<::rclcpp::Service<S>> create_service(const ::std::string&, F,
                                                           const QoS& = QoS::services());

    /// Future-style service client — pair with `spin_until_future_complete`.
    template <typename S>
    ::std::shared_ptr<::rclcpp::Client<S>> create_client(const ::std::string& name,
                                                         const QoS& qos = QoS::services());

    /// Callback-style service client (`void(const S::Response&)`).
    template <typename S, typename F,
              typename = typename std::enable_if<
                  std::is_convertible<F, void (*)(const typename S::Response&)>::value>::type>
    ::std::shared_ptr<::rclcpp::Client<S>> create_client(const ::std::string& name, F callback,
                                                         const QoS& qos = QoS::services());

    /// **REFUSED** — upstream's `shared_ptr` handler shape.
    template <typename S, typename F,
              typename = typename std::enable_if<
                  !::rclcpp::detail::is_qos_arg<F>::value &&
                  !std::is_convertible<F, void (*)(const typename S::Response&)>::value>::type,
              typename = void>
    ::std::shared_ptr<::rclcpp::Client<S>> create_client(const ::std::string&, F,
                                                         const QoS& = QoS::services());

    // -- parameters (bodies in `nros.hpp`) ----------------------------------
    //
    // Forwarders onto THE parameter store — the `nros_params::ParameterServer`
    // the EXECUTOR owns, reached across the FFI by `nros/node_parameters.hpp`.
    // Declared here with the class and DEFINED in `nros.hpp` beside the other
    // out-of-line members, which is where the reasoning lives.
    //
    // Until phase-426 W4 these read an inline `ParameterServer` member on the
    // hosted block instead: a second store, node-local, that the six
    // `rcl_interfaces/srv/*` servers could not read — so a parameter declared
    // through this facade was invisible to `ros2 param get` and a sibling node
    // did not share it. That member is gone, and with it the last thing that
    // made `NROS_CPP_NODE_HOSTED` decide whether a node HAS parameters rather
    // than whether it can spell them.

    /// `rclcpp::Node::declare_parameter<T>(name, default)`.
    template <typename T> T declare_parameter(const char* name, T default_value = T());
    /// `rclcpp::Node::get_parameter<T>(name, out)` — upstream's channel is
    /// `bool`, so ours is too (RFC-0089's error-channel rule, last row).
    template <typename T> bool get_parameter(const char* name, T& out) const;
    /// Value-returning read; `T()` when the name is undeclared.
    template <typename T> T get_parameter(const char* name) const;
    /// Set a declared parameter. NOT upstream's signature — upstream takes a
    /// single `rclcpp::Parameter` and returns a `SetParametersResult`, and
    /// neither type exists here (both are generated ROS messages).
    template <typename T> Result set_parameter(const char* name, T value);
    /// `rclcpp::Node::has_parameter(name)`.
    bool has_parameter(const char* name) const;

    /// `std::string`-keyed overloads. rclcpp keys on `std::string`, which does
    /// not implicitly convert to `const char*`, so a ported call site needs
    /// these to bind at all.
    template <typename T> T declare_parameter(const ::std::string& name, T default_value = T()) {
        return this->template declare_parameter<T>(name.c_str(), default_value);
    }
    template <typename T> bool get_parameter(const ::std::string& name, T& out) const {
        return this->template get_parameter<T>(name.c_str(), out);
    }
    template <typename T> T get_parameter(const ::std::string& name) const {
        return this->template get_parameter<T>(name.c_str());
    }
    template <typename T> Result set_parameter(const ::std::string& name, T value) {
        return this->template set_parameter<T>(name.c_str(), value);
    }
    bool has_parameter(const ::std::string& name) const {
        return this->has_parameter(name.c_str());
    }

    /// @internal Hand the node co-ownership of an arena-registered cell.
    ///
    /// The executor arena stores a raw pointer as its dispatch context and has
    /// no unregister path, so the cell must outlive the registration whatever
    /// the caller does with the `shared_ptr` we hand back. The `create_*`
    /// members do this through the hosted block directly; this is the same
    /// thing for a FREE function that registers on a node
    /// (`rclcpp::create_timer`), which cannot reach a private member.
    void own_entity(const ::std::shared_ptr<void>& cell) {
        this->hosted().owned_entities.push_back(cell);
    }

#endif // NROS_CPP_NODE_HOSTED

    /// Create a new node.
    ///
    /// @param out   Receives the initialized node.
    /// @param name  Node name (null-terminated).
    /// @param ns    Node namespace (null-terminated), or nullptr for "/".
    /// @return Result indicating success or failure.
    static Result create(Node& out, const char* name, const char* ns = nullptr) {
        if (!out.executor_handle_) {
            return Result(ErrorCode::NotInitialized);
        }

        nros_cpp_ret_t ret = nros_cpp_node_create(out.executor_handle_, name, ns, &out.handle_);

        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Get the node name.
    const char* get_name() const {
        if (!initialized_) return "";
        return nros_cpp_node_get_name(&handle_);
    }

    /// Get the node namespace.
    const char* get_namespace() const {
        if (!initialized_) return "";
        return nros_cpp_node_get_namespace(&handle_);
    }

    /// This node's fully-qualified name — `rclcpp::Node::get_fully_qualified_name`.
    ///
    /// Writes `<namespace>/<name>` into `buf`, null-terminated. The root
    /// namespace collapses, so a node named `talker` at `/` is `/talker` and
    /// never `//talker`.
    ///
    /// A buffer rather than rclcpp's returned string: rclcpp hands back a
    /// `const std::string &` it already stores, and there is no allocator here
    /// to build one with. `nros::get_fully_qualified_name` (std_compat.hpp) is
    /// the `std::string` spelling where `NROS_CPP_STD` is on.
    ///
    /// @param out_len Receives the length written, excluding the terminator —
    ///        or, when the buffer is too small, the length that WOULD be
    ///        written, so a caller can size a second attempt. May be nullptr.
    Result get_fully_qualified_name(char* buf, size_t buf_len, size_t* out_len = nullptr) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_node_get_fully_qualified_name(&handle_, buf, buf_len, out_len));
    }

    /// RFC-0088 D4 — the serialization format this node's backend speaks
    /// (`"cdr"`, `"uorb"`), as its cross-image identity string.
    ///
    /// Per SESSION, not per image. A node created with an explicit
    /// `rmw` option sits on its own session, so an image linking two
    /// backends gets two answers here — the case where a compile-time
    /// constant has none.
    ///
    /// Returns `const char*`, deliberately, and never `std::string` or
    /// `std::string_view`: this header must compile against Zephyr's
    /// minimal libcpp, where `<string>` does not exist and `<string_view>`
    /// is not reliably there either (the `NROS_CPP_STD` guard exists for
    /// exactly that). The pointer is static storage owned by the backend;
    /// callers must NOT free it.
    ///
    /// Returns `nullptr` on an uninitialized node, or when the backend
    /// does not declare a format. `nullptr` does not mean `"cdr"`.
    const char* serialization_format() const {
        if (!initialized_) return nullptr;
        return nros_cpp_node_get_serialization_format(&handle_);
    }

    /// `rclcpp::Node::get_logger()` — the logger NAMED FOR THIS NODE.
    ///
    /// phase-427 W5. The two spellings used to collide on an identical
    /// signature: this one returned the real `nros_log::Logger` handle keyed on
    /// the node's name, while the shim's returned a `rclcpp::Logger` built from
    /// the hardcoded sentinel `"nros.compat"` — same name, same arity,
    /// different observable behaviour. The merged accessor takes ROS 2's
    /// behaviour, because a logger that cannot say which node emitted a record
    /// is worse than the one it replaced.
    ///
    /// ONE return type was forced by the merge (two `get_logger()` overloads
    /// differing only in return type are ill-formed), and the ported name keeps
    /// upstream's: `rclcpp::Logger`. That costs nothing at the call site —
    /// `rclcpp::Logger` converts implicitly to `nros_logger_t` (`const void*`),
    /// so `NROS_LOG_INFO(node.get_logger(), …)` and `logger == nullptr` both
    /// still compile and still carry the same opaque handle.
    ///
    /// The handle is `'static` and must NOT be freed; it is null on an
    /// uninitialized node.
    ::rclcpp::Logger get_logger() const {
        if (!initialized_) return ::rclcpp::Logger("", nullptr);
        return ::rclcpp::Logger(nros_cpp_node_get_name(&handle_),
                                nros_cpp_node_get_logger(&handle_));
    }

    /// The node's clock — `rclcpp::Node::get_clock()`.
    ///
    /// rclcpp hands back a `rclcpp::Clock::SharedPtr`. There is no allocator
    /// and no `shared_ptr` here (RFC-0022), so the clock is a member of the
    /// node and this returns a pointer to it: `node.get_clock()->now()` and
    /// `node->get_clock()->now()` both keep their rclcpp spelling. The pointer
    /// is valid for as long as the node is.
    ///
    /// The clock is ROS time, as rclcpp's node clock is. See `nros::Clock` for
    /// what ROS time does and does not yet do here (issue 0789).
    Clock* get_clock() { return &clock_; }
    /// Const overload of `get_clock()`.
    const Clock* get_clock() const { return &clock_; }

    /// The current time on the node's clock — `rclcpp::Node::now()`.
    ///
    /// Shorthand for `get_clock()->now()`, and the call a ported publisher
    /// makes to stamp a header:
    /// ```cpp
    /// node.now().to_msg(msg.header.stamp);
    /// ```
    Time now() const { return clock_.now(); }

    /// Check if the node is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Phase 235.A — internal: raw FFI node handle for the Entry-pkg
    /// NodeContext runtime.
    ///
    /// The declarative `NodeContextOps` boundary (`<nros/node_pkg.hpp>`)
    /// is **type-erased** — entities arrive as descriptor *strings*
    /// (`type_name` / `type_hash`), with no message type `M` available
    /// at the op-function-pointer callsite. The native runtime in
    /// `<nros/main.hpp>` therefore constructs publishers / subscriptions
    /// through the raw `nros_cpp_{publisher,subscription}_create` FFI,
    /// which takes a `const nros_cpp_node_t*`. This accessor hands that
    /// pointer to the runtime. Not part of the public rclcpp-style
    /// surface — user code creates entities via the typed
    /// `create_publisher<M>` / `create_subscription<M>` templates.
    ///
    /// Returns `nullptr` on an uninitialized node.
    const nros_cpp_node_t* ffi_handle() const { return initialized_ ? &handle_ : nullptr; }

    /// Phase 211.H (issue #52) — install the per-topic QoS override table the
    /// deploy plan lowered from `qos_overrides.<topic>.<role>.<policy>` launch
    /// params. Every publisher/subscription created on this node afterwards
    /// folds the matching `(topic, role)` entries into its QoS before the
    /// backend-compat check — the C++ mirror of Rust's
    /// `NodeHandle::set_qos_overrides`. Call once, before creating entities (a
    /// generated/hand-written entry does this before `configure(node)`).
    ///
    /// `overrides` must outlive the node (e.g. a `static` array in the entry).
    /// Pass `len == 0` to clear. No-op on an uninitialized node.
    void set_qos_overrides(const nros_cpp_qos_override_t* overrides, size_t len) {
        if (initialized_) {
            ::nros_cpp_node_set_qos_overrides(&handle_, overrides, len);
        }
    }

    /// Phase 240.5 (RFC-0043) — the opaque executor handle this node was opened
    /// against (from `nros_cpp_init`). The component layer needs it for the raw
    /// FFI that is executor- rather than node-scoped (action server register /
    /// complete_goal / publish_feedback) — `Node::create_*` use it internally,
    /// but a stateful component binding those transports raw needs direct access.
    /// `nullptr` on an uninitialized node.
    void* executor_handle() const { return initialized_ ? executor_handle_ : nullptr; }

    // ---- Graph queries — phase-417 stage 2b (RFC-0089) --------------------
    //
    // rclcpp puts these on the node. `nros::Executor` owns them here because
    // one session per image makes the executor the graph's receiver
    // (RFC-0002), so each method below FORWARDS to the executor this node was
    // opened against and nothing else: no state, no loop, no caching, no name
    // construction. RFC-0019 — the behaviour is Rust's and stays there.
    //
    // The envelope every one of them shares (ADOPT-BOUNDED, RFC-0089): they
    // report what has been DISCOVERED and never block, so an empty result
    // means "nobody seen yet" and never "nobody exists" — poll rather than
    // calling once and concluding. `ErrorCode::Unsupported`, which is what a
    // backend with no graph at all returns, is a DIFFERENT answer from an
    // empty one and must not be collapsed into zero.

    /// Every node on the graph, with its namespace — rclcpp's
    /// `Node::get_node_names()`.
    ///
    /// `visit(ctx, name, ns, enclave)` runs once per node; `enclave` is
    /// `nullptr` where the backend tracks none, which is what lets one call
    /// answer both `rmw_get_node_names` forms. Every string is BORROWED for
    /// the duration of the call; return `false` to stop early.
    ///
    /// rclcpp hands back a `std::vector<std::string>`. There is no allocator
    /// here (RFC-0022), so the same enumeration streams through a visitor —
    /// a plain function pointer plus `ctx`, because these headers compile
    /// `-nostdinc++` against Zephyr's minimal libcpp, where `<functional>`
    /// does not exist (issue 0112).
    ///
    /// Reports what has been DISCOVERED and never blocks: an empty
    /// enumeration means "nobody seen yet", not "nobody exists", and
    /// `ErrorCode::Unsupported` from a backend with no graph stays distinct
    /// from it.
    Result get_node_names(nros_cpp_node_visit_fn visit, void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_node_names(executor_handle_, visit, ctx));
    }

    /// Every topic on the graph, with the types on it — rclcpp's
    /// `Node::get_topic_names_and_types()`.
    ///
    /// `visit(ctx, name, types, types_count)` runs once per distinct TOPIC: a
    /// topic carrying two types is one call with two entries, not two calls.
    /// `types_count` may legitimately be 0 on a partially discovered graph.
    /// Same discovery envelope as [`get_node_names`].
    Result get_topic_names_and_types(nros_cpp_names_and_types_visit_fn visit, void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_topic_names_and_types(executor_handle_, visit, ctx));
    }

    /// Every service on the graph, with its types — rclcpp's
    /// `Node::get_service_names_and_types()`. As
    /// [`get_topic_names_and_types`], over servers and clients.
    Result get_service_names_and_types(nros_cpp_names_and_types_visit_fn visit, void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_service_names_and_types(executor_handle_, visit, ctx));
    }

    /// How many publishers are visible on `topic_name` — rclcpp's
    /// `Node::count_publishers()`.
    ///
    /// `topic_name` is a ROS name (`"/chatter"`), used as given: it is not
    /// remapped and not expanded against this node's namespace, which is what
    /// rclcpp documents for this call too. The count reflects what has been
    /// DISCOVERED, so it can be low right after startup and a zero is never a
    /// proof of absence.
    Result count_publishers(const char* topic_name, size_t* out_count) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_count_publishers(executor_handle_, topic_name, out_count));
    }

    /// How many subscribers are visible on `topic_name` — rclcpp's
    /// `Node::count_subscribers()`. See [`count_publishers`] for the caveats.
    Result count_subscribers(const char* topic_name, size_t* out_count) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_count_subscribers(executor_handle_, topic_name, out_count));
    }

    /// What one named node PUBLISHES, with the types — rclcpp's
    /// `NodeGraph::get_publisher_names_and_types_by_node()`. Same discovery
    /// envelope as [`get_node_names`].
    Result get_publisher_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                                 nros_cpp_names_and_types_visit_fn visit,
                                                 void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_publisher_names_and_types_by_node(
            executor_handle_, node_name, node_namespace, visit, ctx));
    }

    /// What one named node SUBSCRIBES to, with the types.
    ///
    /// `subscription`, not `subscriber`: the C++ surface takes rclcpp's
    /// vocabulary (`create_subscription`, `Subscription<T>`,
    /// `get_subscriptions_info_by_topic`), and `nros::Executor` spells it that
    /// way — this forwarder keeps our two spellings identical rather than
    /// introducing a third. The C surface says `subscriber` because rcl does,
    /// and the vtable slot because upstream rmw does.
    ///
    /// Same discovery envelope as [`get_node_names`].
    Result get_subscription_names_and_types_by_node(const char* node_name,
                                                    const char* node_namespace,
                                                    nros_cpp_names_and_types_visit_fn visit,
                                                    void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_subscription_names_and_types_by_node(
            executor_handle_, node_name, node_namespace, visit, ctx));
    }

    /// What services one named node SERVES, with the types — rclcpp's
    /// `Node::get_service_names_and_types_by_node()`. Servers only, not
    /// clients, as upstream. Same discovery envelope as [`get_node_names`].
    Result get_service_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                               nros_cpp_names_and_types_visit_fn visit,
                                               void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_service_names_and_types_by_node(
            executor_handle_, node_name, node_namespace, visit, ctx));
    }

    /// What services one named node CALLS, with the types — rclcpp's
    /// `NodeGraph::get_client_names_and_types_by_node()`. Same discovery
    /// envelope as [`get_node_names`].
    Result get_client_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                              nros_cpp_names_and_types_visit_fn visit,
                                              void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_client_names_and_types_by_node(
            executor_handle_, node_name, node_namespace, visit, ctx));
    }

    /// The publishers discovered on `topic_name`, one visit each — rclcpp's
    /// `Node::get_publishers_info_by_topic()`.
    ///
    /// The endpoint carries NO QoS profile: rclcpp's `qos_profile()` reports
    /// the GRANTED profile, no backend behind this API can read a remote's
    /// granted profile back, and reporting the remote's DECLARED one instead
    /// would be a confident wrong answer to the question ("why is nothing
    /// arriving?") the field exists to answer. See [`nros::TopicEndpointInfo`].
    ///
    /// rclcpp also takes `no_mangle`; there is no such parameter here, because
    /// accepting one and ignoring it would silently drop configuration —
    /// exactly what RFC-0089's rule forbids.
    ///
    /// Same discovery envelope as [`get_node_names`].
    Result get_publishers_info_by_topic(const char* topic_name,
                                        nros_cpp_endpoint_info_visit_fn visit, void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_publishers_info_by_topic(executor_handle_, topic_name,
                                                                     visit, ctx));
    }

    /// The publishers on `topic_name`, visited as [`nros::TopicEndpointInfo`]
    /// — the rclcpp-shaped overload of the call above.
    ///
    /// A pure conversion over the same query: the visitor sees an endpoint
    /// with rclcpp's accessor names (`node_name()`, `endpoint_type()`,
    /// `endpoint_gid()`) instead of the raw C struct. Every caveat of the
    /// `nros_cpp_endpoint_info_visit_fn` overload applies unchanged, including
    /// the absent QoS profile and the borrowed strings.
    Result get_publishers_info_by_topic(const char* topic_name, TopicEndpointInfoVisitFn visit,
                                        void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        detail::EndpointInfoTrampoline tramp{visit, ctx};
        return Result(nros_cpp_executor_get_publishers_info_by_topic(
            executor_handle_, topic_name, &detail::EndpointInfoTrampoline::thunk, &tramp));
    }

    /// The subscriptions discovered on `topic_name`, one visit each —
    /// rclcpp's `Node::get_subscriptions_info_by_topic()`. See
    /// [`get_publishers_info_by_topic`] for the QoS, `no_mangle` and discovery
    /// envelopes.
    Result get_subscriptions_info_by_topic(const char* topic_name,
                                           nros_cpp_endpoint_info_visit_fn visit, void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_get_subscriptions_info_by_topic(executor_handle_,
                                                                        topic_name, visit, ctx));
    }

    /// The subscriptions on `topic_name`, visited as
    /// [`nros::TopicEndpointInfo`] — the rclcpp-shaped overload of the call
    /// above.
    Result get_subscriptions_info_by_topic(const char* topic_name, TopicEndpointInfoVisitFn visit,
                                           void* ctx) const {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        detail::EndpointInfoTrampoline tramp{visit, ctx};
        return Result(nros_cpp_executor_get_subscriptions_info_by_topic(
            executor_handle_, topic_name, &detail::EndpointInfoTrampoline::thunk, &tramp));
    }

    /// Create a publisher for a topic.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out    Receives the initialized publisher.
    /// @param topic  Topic name (null-terminated).
    /// @param qos    QoS profile (default: reliable, keep-last(10)).
    template <typename M>
    Result create_publisher(::rclcpp::Publisher<M>& out, const char* topic,
                            const QoS& qos = QoS::default_profile());

    /// Create a publisher with rclcpp-style named options (Phase 189.M3.1).
    ///
    /// `options` sits alongside `qos` (rclcpp convention). `PublisherOptions`
    /// is currently a reserved/empty struct, so this overload is observably
    /// identical to the `qos`-only form — it exists for API symmetry and as
    /// the seam for future intra-process / loaned-message knobs.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out      Receives the initialized publisher.
    /// @param topic    Topic name (null-terminated).
    /// @param qos      QoS profile.
    /// @param options  Named publisher options.
    template <typename M>
    Result create_publisher(::rclcpp::Publisher<M>& out, const char* topic, const QoS& qos,
                            const PublisherOptions& options);

    /// Create a subscription for a topic.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out    Receives the initialized subscription.
    /// @param topic  Topic name (null-terminated).
    /// @param qos    QoS profile (default: reliable, keep-last(10)).
    template <typename M>
    Result create_subscription(::rclcpp::Subscription<M>& out, const char* topic,
                               const QoS& qos = QoS::default_profile());

    /// Create a subscription with rclcpp-style named options (Phase 189.M3.1).
    ///
    /// `options` sits alongside `qos` (rclcpp convention) and carries the
    /// non-QoS creation axes. `options.sched_context` (when set) lowers to a
    /// create-then-bind via `nros_cpp_bind_handle_to_sched_context`;
    /// `options.message_info` is reserved (M3.4). See `SubscriptionOptions`.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out      Receives the initialized subscription.
    /// @param topic    Topic name (null-terminated).
    /// @param qos      QoS profile.
    /// @param options  Named subscription options.
    template <typename M>
    Result create_subscription(::rclcpp::Subscription<M>& out, const char* topic, const QoS& qos,
                               const SubscriptionOptions& options);

    /// Create a **callback-style** subscription (rclcpp dispatch model; Phase
    /// 189.M3.x). The executor arena owns the subscriber and invokes `callback`
    /// during `spin_once()` on each new sample, so `options.sched_context` is
    /// functional (poll-style subscriptions have no dispatched callback to
    /// schedule). `callback` must be convertible to `void(const M&)` (a plain
    /// function pointer or empty-capture lambda); the SFINAE guard keeps the
    /// poll-style overloads unambiguous (a `QoS` is not convertible to the
    /// handler type).
    ///
    /// CONSTRAINT: do not move `out` after this returns — the executor arena
    /// holds `&out` as the dispatch context.
    ///
    /// @tparam M  Message type (must define TYPE_NAME, TYPE_HASH, ffi_deserialize).
    /// @param out       Receives the initialized subscription (callback mode).
    /// @param topic     Topic name (null-terminated).
    /// @param callback  Handler invoked as `callback(const M&)` per sample.
    /// @param qos       QoS profile.
    /// @param options   Named subscription options (e.g. sched_context).
    template <
        typename M, typename F,
        typename = typename std::enable_if<std::is_convertible<F, void (*)(const M&)>::value>::type>
    Result create_subscription(::rclcpp::Subscription<M>& out, const char* topic, F callback,
                               const QoS& qos = QoS::default_profile(),
                               const SubscriptionOptions& options = {});

    /// Create a **callback-style** subscription that also delivers each sample's
    /// wire **attachment** (Phase 189.M3.4 — the callback analogue of
    /// `Subscription::take_serialized_with_attachment`). Arena-registered like the
    /// callback overload above (so `options.sched_context` is functional), but the
    /// handler is invoked as `callback(const M&, const uint8_t* attachment, size_t
    /// attachment_len)`; `attachment_len == 0` means the sample carried none.
    /// Cross-RMW bridges read the `bridge_origin` tag from the attachment.
    template <typename M, typename F,
              typename = typename std::enable_if<
                  std::is_convertible<F, void (*)(const M&, const uint8_t*, size_t)>::value>::type>
    Result create_subscription_with_info(::rclcpp::Subscription<M>& out, const char* topic,
                                         F callback, const QoS& qos = QoS::default_profile(),
                                         const SubscriptionOptions& options = {});

#if defined(NANO_ROS_SAFETY_E2E)
    /// Phase 269 W3 — Create a **callback-style** subscription that surfaces the
    /// sample's E2E integrity status (CRC + sequence gap/dup) alongside the typed
    /// message — the C++ component-callback analog of Rust's
    /// `create_subscription_…_with_safety` / `CallbackCtx::integrity()`.
    ///
    /// Arena-registered like the `create_subscription_with_info` overload above
    /// (so `options.sched_context` is functional). The handler is invoked as
    /// `callback(const M&, const nros_cpp_integrity_status_t&)` on each new sample.
    ///
    /// Requires `NANO_ROS_SAFETY_E2E=ON` (lowered from
    /// `[system].features = ["safety"]` via `NanoRosCapabilities.cmake`).
    ///
    /// CONSTRAINT: do not move `out` after this returns — the executor arena
    /// holds `&out` as the trampoline context.
    template <typename M, typename F,
              typename = typename std::enable_if<std::is_convertible<
                  F, void (*)(const M&, const nros_cpp_integrity_status_t&)>::value>::type>
    Result create_subscription_with_safety(::rclcpp::Subscription<M>& out, const char* topic,
                                           F callback, const QoS& qos = QoS::default_profile(),
                                           const SubscriptionOptions& options = {});
#endif // NANO_ROS_SAFETY_E2E

    /// Create a service server.
    ///
    /// @tparam S  Service type (must define nested Request and Response with TYPE_NAME/TYPE_HASH).
    /// @param out           Receives the initialized service server.
    /// @param service_name  Service name (null-terminated).
    /// @param qos           QoS profile (default: services preset).
    template <typename S>
    Result create_service(::rclcpp::Service<S>& out, const char* service_name,
                          const QoS& qos = QoS::services());

    /// Create a **callback-style** service server (rclcpp dispatch model;
    /// Phase 189.M3.3.e). Unlike the poll-style overload above, this
    /// arena-registers the service so it owns a real executor handle and its
    /// request handler runs during `spin_once` — making `options.sched_context`
    /// functional. `callback` must be convertible to
    /// `void(const S::Request&, S::Response&)` (a plain function pointer or
    /// empty-capture lambda); it fills `response` from `request`. The SFINAE
    /// guard keeps the poll-style 3-arg overload unambiguous (a `QoS` is not
    /// convertible to the handler type).
    ///
    /// CONSTRAINT: do not move `out` after this returns — the executor arena
    /// holds `&out` as the dispatch context.
    template <typename S, typename F,
              typename = typename std::enable_if<std::is_convertible<
                  F, void (*)(const typename S::Request&, typename S::Response&)>::value>::type>
    Result create_service(::rclcpp::Service<S>& out, const char* service_name, F callback,
                          const QoS& qos = QoS::services(), const ServiceOptions& options = {});

    /// Create a service client.
    ///
    /// @tparam S  Service type (must define nested Request and Response with TYPE_NAME/TYPE_HASH).
    /// @param out           Receives the initialized service client.
    /// @param service_name  Service name (null-terminated).
    /// @param qos           QoS profile (default: services preset).
    template <typename S>
    Result create_client(::rclcpp::Client<S>& out, const char* service_name,
                         const QoS& qos = QoS::services());

    /// Create a **callback-style** service client (rclcpp async dispatch;
    /// Phase 189.M3.3.f). Arena-registered, so it owns a real executor handle and
    /// its response handler runs during `spin_once` — making
    /// `options.sched_context` functional. `callback` must be convertible to
    /// `void(const S::Response&)`; send requests with
    /// `Client<S>::async_send_request`. The SFINAE guard keeps the future-style
    /// 3-arg overload unambiguous.
    ///
    /// CONSTRAINT: do not move `out` after this returns — the executor arena
    /// holds `&out` as the response dispatch context.
    template <typename S, typename F,
              typename = typename std::enable_if<
                  std::is_convertible<F, void (*)(const typename S::Response&)>::value>::type>
    Result create_client(::rclcpp::Client<S>& out, const char* service_name, F callback,
                         const QoS& qos = QoS::services(), const ClientOptions& options = {});

    /// Create an action server.
    ///
    /// Goals are auto-accepted during spin_once(). Use try_recv_goal() to poll.
    ///
    /// @tparam A  Action type (must define nested Goal, Result, Feedback with TYPE_NAME/TYPE_HASH).
    /// @param out          Receives the initialized action server.
    /// @param action_name  Action name (null-terminated).
    /// @param qos          QoS profile (default: services preset).
    /// @param options      Named options; `options.sched_context` (M3.3.c) binds
    ///                     the goal-service dispatch onto a scheduling context.
    template <typename A>
    Result create_action_server(::rclcpp_action::Server<A>& out, const char* action_name,
                                const QoS& qos = QoS::services(),
                                const ActionServerOptions& options = {});

    /// Create an action client.
    ///
    /// @tparam A  Action type (must define nested Goal, Result, Feedback with TYPE_NAME/TYPE_HASH).
    /// @param out          Receives the initialized action client.
    /// @param action_name  Action name (null-terminated).
    /// @param qos          QoS profile (default: services preset).
    template <typename A>
    Result create_action_client(::rclcpp_action::Client<A>& out, const char* action_name,
                                const QoS& qos = QoS::services());

    /// Phase 122.3.d.b — Create an L1 polling-mode action server.
    /// Caller drives the lifecycle (no executor callback). See
    /// `polling_action_server.hpp` for usage.
    template <typename A>
    Result create_polling_action_server(PollingActionServer<A>& out, const char* action_name);

    /// Phase 122.3.d.b — Create an L1 polling-mode action client.
    template <typename A>
    Result create_polling_action_client(PollingActionClient<A>& out, const char* action_name);

    /// Issue 0278 — Create a latest-value polling subscription (the nano-ros
    /// analog of `autoware_utils::InterProcessPollingSubscriber`): a poll-mode
    /// subscription plus a retained last value, read repeatably via
    /// `PollingSubscription<M>::take_data()` / `take_new_data()`. See
    /// `polling_subscription.hpp`.
    template <typename M>
    Result create_polling_subscription(PollingSubscription<M>& out, const char* topic,
                                       const QoS& qos = QoS::default_profile());

    /// Create a repeating timer on a CLOCK — `rclcpp::create_timer(node, clock,
    /// period, callback)`, phase-425 W4.
    ///
    /// `create_wall_timer` is the steady one: it advances with the executor's
    /// monotonic spin delta, so no simulator can slow it down. This one advances
    /// with `clock`, which is what a node in a simulation or a bag replay wants:
    /// a `NROS_CLOCK_ROS_TIME` timer stops while `/clock` is paused, tracks the
    /// replay rate, and restarts its period on a backwards jump rather than
    /// stalling for the length of it.
    ///
    /// With no `/clock` source installed a ROS-time clock reads system time, the
    /// same fallback `rclcpp::Clock` has — a node written for simulation still
    /// runs standalone.
    ///
    /// The node's own clock (`get_clock()`) is ROS time, as in rclcpp, so
    /// `node.create_timer(t, *node.get_clock(), 100, on_tick)` is the usual
    /// call.
    ///
    /// @param out        Receives the initialized timer.
    /// @param clock      The clock that advances the timer.
    /// @param period_ms  Timer period in milliseconds, ON THAT CLOCK.
    /// @param callback   C function pointer invoked on each tick.
    /// @param context    User context passed to the callback (may be nullptr).
    Result create_timer(Timer& out, const Clock& clock, uint64_t period_ms,
                        nros_cpp_timer_callback_t callback, void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret = nros_cpp_timer_create_on_clock(
            executor_handle_, static_cast<uint8_t>(clock.get_clock_type()), period_ms, callback,
            context, &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a repeating WALL timer — `rclcpp::Node::create_wall_timer`.
    ///
    /// The callback fires during `spin_once()` at the specified period, measured
    /// on the platform's monotonic clock. For a timer that follows simulated
    /// time, see `create_timer` above.
    ///
    /// @param out        Receives the initialized timer.
    /// @param period_ms  Timer period in milliseconds.
    /// @param callback   C function pointer invoked on each tick.
    /// @param context    User context passed to the callback (may be nullptr).
    Result create_wall_timer(Timer& out, uint64_t period_ms, nros_cpp_timer_callback_t callback,
                             void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret =
            nros_cpp_timer_create(executor_handle_, period_ms, callback, context, &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    // ==== phase-427 W3 — member-function binding, on the upstream name ======
    //
    // `nros::bind_timer<C, &C::m>(node, out, ms, self)` was a FREE function
    // with an invented name doing what `create_wall_timer` does. Folding it in
    // removes an invention by reusing an upstream name, which is what clause 2
    // asks for — and it is the shape a component actually writes, so the verb a
    // reader meets first is the one they need.
    //
    // NO ALLOCATION and NO `std::function`: the member pointer is a TEMPLATE
    // PARAMETER, so the trampoline is a capture-less lambda that converts to
    // the executor's raw `void(*)(void*)` and `self` is the context. That is
    // why this overload, unlike the `std::chrono` one, reaches every target.

    /// Bind a member `void C::on_tick()` as a wall-timer callback.
    ///
    /// ```cpp
    /// return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
    /// ```
    template <class C, void (C::*Method)()>
    Result create_wall_timer(Timer& out, uint64_t period_ms, C* self) {
        return this->create_wall_timer(
            out, period_ms, [](void* ctx) { (static_cast<C*>(ctx)->*Method)(); }, self);
    }

    /// Bind a member `void C::on_tick()` as a timer on an explicit CLOCK —
    /// phase-430 W6, the member-binding half of `create_timer`.
    ///
    /// A `NROS_CLOCK_ROS_TIME` clock follows `/clock` and stops when the bag
    /// stops; a `NROS_CLOCK_STEADY_TIME` one does not. `create_wall_timer`
    /// above is the steady verb and is unaffected by simulated time.
    template <class C, void (C::*Method)()>
    Result create_timer(Timer& out, const Clock& clock, uint64_t period_ms, C* self) {
        return this->create_timer(
            out, clock, period_ms, [](void* ctx) { (static_cast<C*>(ctx)->*Method)(); }, self);
    }

    /// Bind a member `void C::on_tick()` as a timer IN a callback group
    /// (RFC-0047) — the member-binding half of `create_timer_in_group`.
    template <class C, void (C::*Method)()>
    Result create_timer_in_group(const CallbackGroup& group, Timer& out, uint64_t period_ms,
                                 C* self) {
        return this->create_timer_in_group(
            group, out, period_ms, [](void* ctx) { (static_cast<C*>(ctx)->*Method)(); }, self);
    }

    /// Create a one-shot timer.
    ///
    /// The callback fires once after the specified delay.
    ///
    /// @param out       Receives the initialized timer.
    /// @param delay_ms  Delay in milliseconds before the callback fires.
    /// @param callback  C function pointer invoked once.
    /// @param context   User context passed to the callback (may be nullptr).
    Result create_timer_oneshot(Timer& out, uint64_t delay_ms, nros_cpp_timer_callback_t callback,
                                void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret = nros_cpp_timer_create_oneshot(executor_handle_, delay_ms, callback,
                                                           context, &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    // -- Phase 273 (RFC-0047) — Callback-group API -------------------------

    /// Create a named callback-group token.
    ///
    /// The returned `CallbackGroup` may be passed to `create_timer_in_group`,
    /// `create_subscription_in_group`, or `create_publisher_in_group` to associate entities
    /// with the group's SchedContext (resolved via `group_sched_table`).
    ///
    /// @param name  Group name — must be a string literal or static-lifetime
    ///              string; the pointer is stored directly (no copy).
    CallbackGroup create_callback_group(const char* name) { return CallbackGroup{name}; }

    /// Create a repeating timer **in** a callback group (RFC-0047).
    ///
    /// Like `create_wall_timer` but associates the timer with `group` so the
    /// executor binds it to the group's SchedContext via `group_sched_table`.
    /// `group.get_name() == nullptr` or empty falls back to node default.
    ///
    /// @param group      Callback group (from `create_callback_group`).
    /// @param out        Receives the initialized timer.
    /// @param period_ms  Timer period in milliseconds.
    /// @param callback   C function pointer invoked on each tick.
    /// @param context    User context passed to the callback (may be nullptr).
    Result create_timer_in_group(const CallbackGroup& group, Timer& out, uint64_t period_ms,
                                 nros_cpp_timer_callback_t callback, void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret = nros_cpp_timer_create_in_group(
            executor_handle_, &handle_, period_ms, callback, context, group.get_name(), &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a **callback-style** subscription **in** a callback group (RFC-0047).
    ///
    /// Like the callback-style `create_subscription` but associates the
    /// subscription with `group` so the executor binds it to the group's
    /// SchedContext via `group_sched_table`. Out-of-line definition in
    /// `subscription.hpp`.
    ///
    /// @tparam M  Message type (must define TYPE_NAME, TYPE_HASH, ffi_deserialize).
    /// @param group      Callback group.
    /// @param out        Receives the initialized subscription (callback mode).
    /// @param topic      Topic name (null-terminated).
    /// @param callback   Handler invoked as `callback(const M&)` per sample.
    /// @param qos        QoS profile.
    /// @param options    Named subscription options.
    template <
        typename M, typename F,
        typename = typename std::enable_if<std::is_convertible<F, void (*)(const M&)>::value>::type>
    Result create_subscription_in_group(const CallbackGroup& group, ::rclcpp::Subscription<M>& out,
                                        const char* topic, F callback,
                                        const QoS& qos = QoS::default_profile(),
                                        const SubscriptionOptions& options = {});

    /// Create a publisher **in** a callback group (API symmetry; RFC-0047).
    ///
    /// Publishers have no dispatched callback — the group parameter is accepted
    /// for API symmetry but has no scheduling effect (documented in RFC-0047 §OQ1
    /// follow-up).
    ///
    /// @tparam M  Message type.
    /// @param group  Callback group (accepted for symmetry; no scheduling effect).
    /// @param out    Receives the initialized publisher.
    /// @param topic  Topic name.
    /// @param qos    QoS profile.
    template <typename M>
    Result create_publisher_in_group(const CallbackGroup& /* group */, ::rclcpp::Publisher<M>& out,
                                     const char* topic, const QoS& qos = QoS::default_profile()) {
        return create_publisher<M>(out, topic, qos);
    }

    /// Create a guard condition for cross-thread signaling.
    ///
    /// The callback fires during `spin_once()` when `guard.trigger()` is called.
    ///
    /// @param out       Receives the initialized guard condition.
    /// @param callback  C function pointer invoked when triggered.
    /// @param context   User context passed to the callback (may be nullptr).
    Result create_guard_condition(GuardCondition& out, nros_cpp_guard_callback_t callback,
                                  void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_ret_t ret =
            nros_cpp_guard_condition_create(executor_handle_, callback, context, out.storage_);
        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

    // ==== phase-427 W4 — the ours-only family, under `_in` names =============
    //
    // These came off `nros::ComponentNode`. Every one of them is spelled with
    // an `_in` SUFFIX rather than upstream's bare verb, and the rename is the
    // whole point of the item.
    //
    // `ComponentNode::create_publisher<M>(const char*, const QoS&)` returned by
    // value and reported failure through the `ok()` latch; upstream's
    // `create_publisher<M>(const std::string&, …)` returns a `shared_ptr` and
    // throws. As OVERLOADS on one type those differ only in SIGNATURE, and C++
    // resolves a signature-only difference SILENTLY. Measured on the merged
    // shape before the rename, with gcc 13:
    //
    //   create_publisher<M>("chatter", 10)                -> UPSTREAM (shared_ptr)
    //   create_publisher<M>("chatter", rclcpp::QoS(10))    -> OURS (by value),
    //                                                         accepted with only
    //                                                         a -Wextra-ish note
    //                                                         that ISO calls it
    //                                                         ambiguous
    //
    // So a ported file compiles either way and gets a different type, lifetime
    // and failure channel depending on which spelling of the depth it used.
    // That is exactly the compile-and-differ RFC-0089 forbids, and the merge is
    // what would have manufactured it. The fix is a different NAME, not a
    // cleverer signature — the third application of that rule in this campaign,
    // after the reordered C node initialiser and the clock-taking C timer verb.
    //
    // `tests/compile/one_node_type_ours_only_names.cpp` pins both halves: that
    // the ported spellings reach upstream's overload on the real type, and
    // (the negative control) that the pre-rename shape bound the wrong one.

    /// Create a publisher, returning it BY VALUE — the ours-only shape.
    ///
    /// Was `ComponentNode::create_publisher<M>(topic, qos)`. Move it into a
    /// member: `pub_ = create_publisher_in<M>("/topic")`. Latches `ok()=false`
    /// on failure instead of throwing.
    template <typename M>
    ::rclcpp::Publisher<M> create_publisher_in(const char* topic,
                                               const QoS& qos = QoS::default_profile()) {
        ::rclcpp::Publisher<M> pub;
        Result r = this->create_publisher(pub, topic, qos);
        if (!r.ok()) {
            this->set_error("create_publisher_in", r.raw());
        }
        return pub;
    }

    /// Create a **typed member-callback** subscription — the ours-only shape.
    ///
    /// Was `ComponentNode::create_subscription<M, C, Method>(topic, qos)`.
    /// Registers a raw subscription on the executor keyed on `M::TYPE_NAME`
    /// with a no-alloc deserialize-then-dispatch trampoline; the executor arena
    /// owns the subscriber, so there is no `Subscription<M>` storage to supply
    /// and nothing to park.
    ///
    /// DECLARED here and DEFINED in `component.hpp`: the body calls
    /// `nros::bind_subscription`, which lives there, and `component.hpp`
    /// includes THIS file — so defining it here would close an include cycle.
    /// `nros.hpp` pulls in `component.hpp`, so the definition is visible
    /// wherever the umbrella is.
    template <typename M, class C, void (C::*Method)(const M& msg)>
    void create_subscription_in(const char* topic, const QoS& qos = QoS::default_profile());

    /// The same, **in** a callback group (RFC-0047) — the group's SchedContext
    /// is resolved via `group_sched_table`. Also defined in `component.hpp`.
    template <typename M, class C, void (C::*Method)(const M& msg)>
    void create_subscription_in_group(const CallbackGroup& group, const char* topic,
                                      const QoS& qos = QoS::default_profile());

    /// phase-403 step 2 — the BOOT-TIME half of the declared-depth check.
    ///
    /// The compile-time half (`NROS_SUBSCRIBE`'s `static_assert`) is the
    /// primary path and covers every call site whose topic is a string literal,
    /// which is all of them in this tree. It cannot cover two shapes: a topic
    /// built at runtime or forwarded through a variable (the lookup key is not
    /// a constant expression), and a caller that reaches the member directly
    /// rather than through the macro. Both get the same comparison here,
    /// against the same table, through the same `nros::declared_depth`.
    ///
    /// A disagreement is a named boot failure: it goes through `set_error`,
    /// which makes the entry's post-construct `ok()` check halt boot. It does
    /// NOT create the subscription — an entity built at a depth its own image
    /// did not declare is exactly the arena mis-sizing this step prevents.
    ///
    /// Returns true when the subscription may be created.
    bool check_declared_depth(const char* type_name, const char* topic, const QoS& qos) {
        const int declared = ::nros::declared_depth(type_name, topic);
        // Nobody declared this endpoint. Not an error: the image has not opted
        // in, and anything sizing from depth refuses rather than defaulting.
        if (declared == ::nros::DECLARED_DEPTH_UNDECLARED) {
            return true;
        }
        if (declared == qos.depth()) {
            return true;
        }
        // The two NUMBERS and the TOPIC go out first: `set_error` records one
        // `const char*`, which cannot carry them.
        detail::report_declared_depth_mismatch(this->get_name(), topic, declared, qos.depth());
        this->set_error("create_subscription_in: QoS depth disagrees with the depth declared for "
                        "this topic in the contract sidecar. Depth multiplies the arena, so the "
                        "declaration and the code must state one number, not two.",
                        detail::DECLARED_DEPTH_MISMATCH);
        return false;
    }

    /// Record the first creation failure (RFC-0044 Q2). Idempotent on the
    /// *first* failure — later failures don't clobber the original diagnostic.
    ///
    /// PUBLIC rather than protected, unlike `ComponentNode::set_error`: the
    /// out-ref `create_*` family reports through `Result`, so a derived node
    /// that wires entities with it has no other way to join the same latch the
    /// entry's post-construct check reads.
    void set_error(const char* what, int32_t code) {
        // Relaxed read for the first-failure guard: set_error runs during
        // construction, on the constructing thread, never concurrently with a
        // reader (issue #230).
        if (!__atomic_load_n(&has_error_, __ATOMIC_RELAXED)) {
            error_what_ = what;
            error_code_ = code;
            // RELEASE publishes the two plain writes above: a reader that
            // acquire-observes has_error_ == true is guaranteed to see them.
            __atomic_store_n(&has_error_, true, __ATOMIC_RELEASE);
            detail::report_component_failure(this->get_name(), what, code);
        }
    }

    /// Destructor — releases node resources.
    ///
    /// BYTE-IDENTICAL IN EVERY CONFIGURATION (phase-427 W1). It never names the
    /// hosted type: the block carries its own `destroy` function pointer, so a
    /// freestanding TU and a hosted TU of the same image emit the same inline
    /// destructor. A `#if`-gated `delete` here would be the ODR half of exactly
    /// the defect the layout rule exists to prevent.
    ~Node() {
        if (hosted_ != nullptr) {
            detail::NodeHostedBase* h = static_cast<detail::NodeHostedBase*>(hosted_);
            hosted_ = nullptr;
            h->destroy(h);
        }
        if (initialized_) {
            nros_cpp_node_destroy(&handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Node(Node&& other)
        : handle_(other.handle_), initialized_(other.initialized_),
          executor_handle_(other.executor_handle_), clock_(other.clock_), hosted_(other.hosted_),
          // The latch travels with the node: a moved-from node's failure is
          // still this node's failure, and the entry checks `ok()` on whichever
          // object it ended up holding.
          has_error_(other.has_error_), error_what_(other.error_what_),
          error_code_(other.error_code_) {
        other.initialized_ = false;
        other.executor_handle_ = nullptr;
        other.hosted_ = nullptr;
    }

    Node& operator=(Node&& other) {
        if (this != &other) {
            if (hosted_ != nullptr) {
                detail::NodeHostedBase* h = static_cast<detail::NodeHostedBase*>(hosted_);
                hosted_ = nullptr;
                h->destroy(h);
            }
            if (initialized_) {
                nros_cpp_node_destroy(&handle_);
            }
            handle_ = other.handle_;
            initialized_ = other.initialized_;
            executor_handle_ = other.executor_handle_;
            clock_ = other.clock_;
            hosted_ = other.hosted_;
            has_error_ = other.has_error_;
            error_what_ = other.error_what_;
            error_code_ = other.error_code_;
            other.initialized_ = false;
            other.executor_handle_ = nullptr;
            other.hosted_ = nullptr;
        }
        return *this;
    }

  private:
    Node(const Node&) = delete;
    Node& operator=(const Node&) = delete;

    nros_cpp_node_t handle_;
    bool initialized_;
    void* executor_handle_; // Set by nros::init() via friendship
    // Issue 0789 — the node's own clock, ROS time as in rclcpp. Constructing
    // it touches no platform service (only a steady clock records an epoch),
    // so a Node in static storage stays as cheap to create as it was.
    Clock clock_;
#ifdef NROS_CPP_NODE_HOSTED
    /// Allocate-on-first-use accessor for the hosted block.
    ///
    /// LAZY, so a hosted image whose nodes only ever take the out-ref
    /// `create_*` family never calls `operator new` either. `const` reads go
    /// through the same allocation because `get_node_options()` and
    /// `parameters() const` must answer on a node nobody has written to yet.
    detail::NodeHosted& hosted() const {
        if (hosted_ == nullptr) {
            const_cast<Node*>(this)->hosted_ =
                static_cast<detail::NodeHostedBase*>(new detail::NodeHosted());
        }
        return *static_cast<detail::NodeHosted*>(static_cast<detail::NodeHostedBase*>(hosted_));
    }
#endif

    /// phase-427 W1 — `detail::NodeHosted*`, held as the address of its
    /// `NodeHostedBase` subobject. UNCONDITIONAL, and null on a freestanding
    /// target and on any hosted node that never made a hosted-shape call. One
    /// pointer is what the whole hosted surface costs a node that does not use
    /// it; the alternative — the members themselves behind a `#if` — is the
    /// layout-follows-a-probe bug this class already shipped once.
    void* hosted_;

    // phase-427 W4 — the error latch, 24 UNCONDITIONAL bytes on every node.
    //
    // These do NOT go behind `hosted_`, and the decision is deliberate rather
    // than an oversight the layout rule missed. This is the error channel a
    // `-fno-exceptions` target has INSTEAD of a throwing constructor: the
    // boot-halt mechanism (`NanoRosEntityInventory.cmake`, RFC-0044 Q2) is
    // exactly what a freestanding image needs, and a latch reachable only on a
    // hosted target would leave the firmware case — the one that cannot throw —
    // with no channel at all. So every node pays 24 bytes, which is the price
    // of one node type whose failure story works on both.
    //
    // Issue #230 — inverted flag: the HEALTHY value is the zero-init default,
    // so a cross-core reader sees "ok" without waiting for any store. Accessed
    // via __atomic builtins (release in set_error, acquire in ok()/error_*) —
    // NOT `<atomic>`, which the Zephyr `-nostdinc++` minimal libcpp may lack
    // (issue 0112 class). All three fields are zero-init-safe.
    bool has_error_ = false;
    const char* error_what_ = nullptr;
    int32_t error_code_ = 0;

    friend class Executor;
    friend class NodeBuilder;
    friend Result init(const char* locator, uint8_t domain_id);
    friend Result init(const char* locator, uint8_t domain_id, const char* session_name);
    friend Result init_with_rmw(const char* rmw, const char* locator, uint8_t domain_id,
                                const char* session_name);
    friend Result shutdown();
    friend bool ok();
    friend Result create_node(Node& out, const char* name, const char* ns);
    friend Result create_node_on(Node& out, void* executor_handle, const char* name,
                                 const char* ns);
    friend Result spin_once(int32_t timeout_ms);
    friend Result spin();
    friend Result spin(uint32_t duration_ms, int32_t poll_ms);
    friend void* global_handle();

    // Global executor inline storage for init/shutdown free functions.
    //
    // Use a template-static-member trick instead of a function-local static.
    // Function-local statics need __cxa_guard_acquire/release on first-call
    // initialisation; on NuttX the resulting guard logic returns NULL for
    // the storage pointer (observed empirically with LTO on armv7a-nuttx-eabihf,
    // even with constant-initialisation `= {}`). A template static member is
    // emitted into .bss like a file-scope variable and gets COMDAT-folded by
    // the linker, sidestepping the guarded-init path entirely.
    template <int = 0> struct GlobalStorageHolder {
        alignas(8) static uint8_t storage[NROS_CPP_EXECUTOR_STORAGE_SIZE];
    };
    static uint8_t* global_storage() { return GlobalStorageHolder<>::storage; }

    /// phase-432 W3.1 — DERIVED, not stored.
    ///
    /// This used to be a `static bool` beside the storage, set by `init` and
    /// cleared by `shutdown`. Two problems, one fix.
    ///
    /// It was a SECOND answer: the context's own tag already says whether the
    /// storage is live, and the flag was maintained by hand at four sites. They
    /// could disagree — a direct `nros_cpp_fini` (which `run_tiers.c` does)
    /// tore the context down without touching the flag, so `ok()` kept saying
    /// yes over a dropped executor.
    ///
    /// And it was UNREACHABLE FROM C, being a C++ template static emitted by
    /// this header. That is what blocked a pure-C `run_components`: its spin
    /// loop's exit condition is `ok()`, and C had nothing to ask.
    ///
    /// Now both languages ask the same function. The call is not on any hot
    /// path — every `ok()` call site in the tree is a spin-loop condition
    /// paired with a millisecond-scale blocking `spin_once`, so a load became a
    /// call once per tick.
    static bool global_initialized() {
        return nros_cpp_context_is_live(GlobalStorageHolder<>::storage);
    }
};

// Out-of-class definitions for Node::GlobalStorageHolder<> — the template
// machinery means these get emitted as COMDAT symbols, so multiple TUs
// including this header all collapse to a single .bss allocation.
template <int N>
alignas(8) uint8_t Node::GlobalStorageHolder<N>::storage[NROS_CPP_EXECUTOR_STORAGE_SIZE] = {};

// ==== phase-427 W4 — the timer pool, as a TEMPLATE PARAMETER ================
//
// `ComponentNode` carried `Timer timers_[NROS_COMPONENT_MAX_TIMERS]` — 192
// unconditional bytes at the default depth of 8. Merging that onto `Node` would
// have charged every node in the tree for a pool most of them never touch, on
// targets chosen for having no memory to spare.
//
// So the depth is a template parameter and the DEFAULT IS ZERO: `Node` itself
// carries no pool and is unchanged in size by this half of the merge. A node
// that wants the storage-free timer verbs derives `NodeWithTimers<N>` and names
// the depth it needs.
//
// Why a derived template and not `template <size_t N> class Node` — measured,
// not preferred. `Node` has to stay ONE non-template type: `rclcpp::Node` is an
// alias for it and `tests/compile/one_node_type.cpp` asserts
// `std::is_same<rclcpp::Node, nros::Node>`; 218 in-tree sites spell `Node` with
// no argument list, which a class template with a defaulted parameter does not
// permit; and every `Node&` parameter in the tree — `create_node(Node&)`,
// `Executor`, `spin` — would otherwise accept only ONE depth, so a component
// with a pool would not be a node the executor could take. Deriving keeps the
// IS-A that `ComponentNode` never had (it WRAPPED a node, which is the reason
// this merge exists at all) while keeping the bytes opt-in.
//
// `NodeWithTimers` is NOT a second node type. It adds storage and two verbs;
// it declares no identity, and every one of its instances IS-A `Node`.

/// Max timers a `NodeWithTimers` may own via the storage-free `create_*_in`
/// members. Overridable per build with a `#define` before including this
/// header; it is the DEFAULT depth, not a cap on what a node may ask for.
#ifndef NROS_COMPONENT_MAX_TIMERS
#define NROS_COMPONENT_MAX_TIMERS 8
#endif
/// Issue 1131 — the pool is a fixed inline C array, and zero is not a smaller
/// one: `Timer timers_[0]` is not ISO C++ (GCC/Clang accept it only as the
/// zero-length-array extension). A node that owns no timer does not derive this
/// template at all, which is the real zero.
///
/// Below the `#ifndef`, never above it: an undefined identifier reads as 0 in
/// `#if`, so a guard above its own default fires on every build (issue 1167).
#if NROS_COMPONENT_MAX_TIMERS < 1
#error "NROS_COMPONENT_MAX_TIMERS must be >= 1: it sizes a C array (issue 1015)"
#endif

/// A `Node` plus an inline pool of `MaxTimers` `nros::Timer` slots.
///
/// `nros::Timer`'s destructor cancels its timer, so a timer created in a
/// constructor must outlive the call. The out-ref `create_wall_timer(Timer&,
/// …)` / `create_timer_in_group(group, Timer&, …)` family on `Node` makes that the
/// caller's problem, which is the right default. This template is for the
/// component shape, where the storage-free spelling is the ergonomic one:
///
/// ```cpp
/// class Talker : public nros::NodeWithTimers<1> {
///     rclcpp::Publisher<Int32> pub_;
///   public:
///     explicit Talker(nros::NodeHandle h) : nros::NodeWithTimers<1>(h, "talker") {
///         pub_ = create_publisher_in<Int32>("/chatter");
///         create_wall_timer_in<Talker, &Talker::on_tick>(500);
///     }
///     void on_tick();
/// };
/// ```
template <::size_t MaxTimers = NROS_COMPONENT_MAX_TIMERS> class NodeWithTimers : public Node {
    static_assert(MaxTimers >= 1, "NodeWithTimers<0> has no pool -- derive Node directly, or use "
                                  "the out-ref create_wall_timer(Timer&, ...) family");

  public:
    using Node::Node;

    /// Create a **typed member** repeating wall timer parked in the pool.
    ///
    /// Was `ComponentNode::create_wall_timer<C, Method>(period_ms)`; `_in`
    /// because upstream's `create_wall_timer(duration, callback)` is a
    /// different shape on the same type and a signature-only difference is
    /// resolved silently. Latches `ok()=false` on failure or pool exhaustion.
    template <class C, void (C::*Method)()> void create_wall_timer_in(uint64_t period_ms) {
        Timer* slot = this->next_timer_slot("create_wall_timer_in");
        if (slot == nullptr) return;
        Result r = this->Node::template create_wall_timer<C, Method>(*slot, period_ms,
                                                                     static_cast<C*>(this));
        if (!r.ok()) {
            this->set_error("create_wall_timer_in", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// The plain C callback + ctx escape hatch, parked in the pool.
    void create_wall_timer_in(uint64_t period_ms, nros_cpp_timer_callback_t callback,
                              void* context = nullptr) {
        Timer* slot = this->next_timer_slot("create_wall_timer_in");
        if (slot == nullptr) return;
        Result r = this->Node::create_wall_timer(*slot, period_ms, callback, context);
        if (!r.ok()) {
            this->set_error("create_wall_timer_in", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// A **typed member** repeating timer **in** a callback group (RFC-0047),
    /// parked in the pool. Was `ComponentNode::create_timer_in<C, Method>`.
    template <class C, void (C::*Method)()>
    void create_timer_in_group(const CallbackGroup& group, uint64_t period_ms) {
        Timer* slot = this->next_timer_slot("create_timer_in_group");
        if (slot == nullptr) return;
        Result r = this->Node::template create_timer_in_group<C, Method>(group, *slot, period_ms,
                                                                         static_cast<C*>(this));
        if (!r.ok()) {
            this->set_error("create_timer_in_group", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// The plain C callback + ctx form, in a group, parked in the pool.
    void create_timer_in_group(const CallbackGroup& group, uint64_t period_ms,
                               nros_cpp_timer_callback_t callback, void* context = nullptr) {
        Timer* slot = this->next_timer_slot("create_timer_in_group");
        if (slot == nullptr) return;
        Result r = this->Node::create_timer_in_group(group, *slot, period_ms, callback, context);
        if (!r.ok()) {
            this->set_error("create_timer_in_group", r.raw());
            return;
        }
        ++timer_count_;
    }

    // The out-ref forms on `Node` share these names; bring them in so a derived
    // node can still spell `create_timer_in_group(group, my_timer_, 100, cb, ctx)`
    // without the pool overloads hiding the base by name lookup.
    using Node::create_timer_in_group;
    using Node::create_wall_timer;

  private:
    /// The next free pool slot, or `nullptr` after latching an exhaustion
    /// error. One place so all four verbs report it identically.
    Timer* next_timer_slot(const char* what) {
        if (timer_count_ >= MaxTimers) {
            this->set_error(what, -1);
            return nullptr;
        }
        return &timers_[timer_count_];
    }

    Timer timers_[MaxTimers];
    ::size_t timer_count_ = 0;
};

// -- Free function implementations --

inline Result init(const char* locator, uint8_t domain_id) {
    // Issue 0329 — forward RAW to the 3-arg overload, which resolves the whole
    // ladder (baked `NROS_ENTRY_LOCATOR`/`NROS_ENTRY_DOMAIN_ID` rungs, hosted
    // default, then the env overlay in the Rust resolver behind `nros_cpp_init`).
    // This overload previously re-resolved the locator rung itself — a second,
    // partial copy of the same ladder (it never applied `NROS_ENTRY_DOMAIN_ID`),
    // duplicating what the 3-arg already does. Phase 266: unified default session
    // name "node".
    return init(locator, domain_id, "node");
}

inline Result init(const char* locator, uint8_t domain_id, const char* session_name) {
    // NROS_CPP_RET_INVALID_ARGUMENT = -3 (defined in nros_cpp_ffi.h
    // which isn't included from this header — duplicate the value
    // inline; generated header is the source of truth).
    if (session_name == nullptr) {
        return Result(-3);
    }
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    // Issue #39 — apply the same `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` env
    // fallback as the 2-arg `init()` when `locator` is null / `domain_id` is
    // 0. This makes `init_with_launch_auto()` (which delegates here with a
    // null locator) honor the env overlay instead of passing a null locator
    // to the backend → TransportError / degraded session.
    // Issue 0330 — the hard local default is GONE entirely (it was a zenoh
    // fact in an RMW-blind header); the backend now supplies it.
    // RFC-0045 / issue #206 — the env overlay (NROS_LOCATOR / ROS_DOMAIN_ID /
    // NROS_NODE_NAME) moved into the shared Rust resolver behind
    // nros_cpp_init (precedence model A: hosted env > this baked chain >
    // compiled default; malformed or >232 ROS_DOMAIN_ID is an init ERROR,
    // never a silent domain 0). This header only assembles the baked rung.
#endif
    // Phase-287 W6 — compile-time connect defaults, so ONE portable source
    // works native + embedded. `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`
    // are target compile definitions the embedded board gate bakes
    // (NanoRosEntry.cmake; Kconfig on Zephyr via <nros/main.hpp>); on native
    // they are undefined. Precedence (model A, RFC-0045/#206): env (hosted,
    // applied in the Rust resolver) > explicit arg > baked macro > default.
#ifdef NROS_ENTRY_LOCATOR
    if (locator == nullptr) {
        locator = NROS_ENTRY_LOCATOR;
    }
#endif
#ifdef NROS_ENTRY_DOMAIN_ID
    // Only the UNSET sentinel (0) folds the baked macro in; an explicit
    // argument — including kDomainIdExplicitZero (255) for a literal
    // domain 0 (issue #227) — passes through untouched.
    if (domain_id == 0) {
        domain_id = static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID);
    }
#endif
    // Issue 0330 — there is deliberately NO hosted "tcp/127.0.0.1:7447"
    // fallback here. That value is a *zenoh* fact and this header is
    // RMW-blind; a cyclonedds or xrce build must not carry it. A null
    // locator flows through `nros_cpp_init` (→ `None` → the RFC-0045
    // resolver's empty bottom rung) to whichever backend is linked, and
    // that backend applies its own default (zenoh:
    // `nros_rmw_zenoh::DEFAULT_LOCATOR`; xrce: its agent default;
    // cyclonedds: ignores the locator). Precedence is otherwise unchanged:
    // hosted env > explicit arg > baked macro > backend default.

    // Phase 128.C.1 / phase-241.D3-rev — RMW-blind init. The selected
    // backend registers itself before `main` via its `.init_array` ctor
    // (RFC-0042 §D3.3), and `nros_cpp_init` additionally calls the weak
    // `nros_app_register_backends()` board-override hook for RTOS targets
    // where `.init_array` ctors do not run. No `#ifdef NROS_RMW_*` chain
    // here, no CMake-driven fan-out — the user's
    // `target_link_libraries(... NanoRos::Rmw::<name>)` is the only
    // selector.
    // Issue 1050 defect (3) — the RMW selector's BAKED rung. `NROS_ENTRY_RMW`
    // is a target compile definition the entry gate bakes, exactly like
    // `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` above; undefined means the
    // image names no backend and the registry must contain exactly one.
    //
    // This is what the paragraph above could not express. "The user's
    // `target_link_libraries(... NanoRos::Rmw::<name>)` is the only selector"
    // is true of the LINK and false of the REGISTRY: a hosted archive
    // registers whatever it carries, from `.init_array`, before `main`.
#ifdef NROS_ENTRY_RMW
    const char* rmw = NROS_ENTRY_RMW;
#else
    const char* rmw = nullptr;
#endif
    nros_cpp_ret_t ret =
        nros_cpp_init_rmw(rmw, locator, domain_id, session_name, nullptr, Node::global_storage());
    // No flag to set: `nros_cpp_init_rmw` stamps the context tag, and
    // `global_initialized()` reads it.
    return Result(ret);
}

inline Result init_with_rmw(const char* rmw, const char* locator, uint8_t domain_id,
                            const char* session_name) {
    // NROS_CPP_RET_INVALID_ARGUMENT = -3; see the 3-arg overload for why the
    // value is duplicated here rather than included.
    if (session_name == nullptr) {
        return Result(-3);
    }
#ifdef NROS_ENTRY_LOCATOR
    if (locator == nullptr) {
        locator = NROS_ENTRY_LOCATOR;
    }
#endif
#ifdef NROS_ENTRY_DOMAIN_ID
    if (domain_id == 0) {
        domain_id = static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID);
    }
#endif
    // No `NROS_ENTRY_RMW` fallback here: an explicit argument that resolves to
    // nullptr is the caller saying "no selector", and quietly substituting the
    // bake would make this overload unable to express that.
    nros_cpp_ret_t ret =
        nros_cpp_init_rmw(rmw, locator, domain_id, session_name, nullptr, Node::global_storage());
    // No flag to set: `nros_cpp_init_rmw` stamps the context tag, and
    // `global_initialized()` reads it.
    return Result(ret);
}

inline Result shutdown() {
    if (!Node::global_initialized()) {
        return Result::success();
    }
    // No flag to clear: `nros_cpp_fini` unstamps the tag, which is what
    // `global_initialized()` reads. One owner of the fact.
    return Result(nros_cpp_fini(Node::global_storage()));
}

// -- Phase 212.L.5 launch-aware init --
//
// Both `init_with_launch_auto` and `init_with_launch(path)` delegate to
// the existing 3-arg `init` after resolving the launch overlay (today:
// env vars only — see header docs for the follow-up plan). The session
// name falls back to `"nros_cpp"` so existing callsites keep working.

inline Result init_with_launch_auto(int argc, char** argv, const char* session_name) {
    (void)argc;
    (void)argv;
    // TODO (Phase 212.L.5 follow-up):
    //   1. If $NROS_RUNTIME_OVERLAY is set, read the JSON sidecar and
    //      fold its params/remaps/env into the init call.
    //   2. Else walk <CARGO_MANIFEST_DIR>/launch/* and parse the XML
    //      in-process.
    // For now the env overlay (NROS_LOCATOR / ROS_DOMAIN_ID consumed by
    // the 2-arg `init`) is the only channel.
    const char* name = (session_name != nullptr) ? session_name : "nros_cpp";
    return init(nullptr, 0, name);
}

inline Result init_with_launch(const char* path, int argc, char** argv, const char* session_name) {
    (void)argc;
    (void)argv;
    // NROS_CPP_RET_INVALID_ARGUMENT = -3 (mirrors the 3-arg init guard).
    if (path == nullptr) {
        return Result(-3);
    }
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    // Verify the file exists so misspelled paths fail fast at init time
    // instead of surfacing as a silently-empty overlay later.
    if (FILE* f = std::fopen(path, "rb")) {
        std::fclose(f);
    } else {
        return Result(ErrorCode::NotInitialized);
    }
#endif
    // TODO (Phase 212.L.5 follow-up): parse `path` as launch XML and
    // fold params/remaps/env into the init call. Today the env overlay
    // is the only channel.
    const char* name = (session_name != nullptr) ? session_name : "nros_cpp";
    return init(nullptr, 0, name);
}

/// Check if the nros session is initialized.
inline bool ok() {
    return Node::global_initialized();
}

/// Create a node (convenience — uses the global executor).
///
/// This is the primary way to create nodes after calling `nros::init()`.
///
/// @param out   Receives the initialized node.
/// @param name  Node name.
/// @param ns    Node namespace, or nullptr for "/".
inline Result create_node(Node& out, const char* name, const char* ns = nullptr) {
    if (!Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    out.executor_handle_ = Node::global_storage();
    return Node::create(out, name, ns);
}

/// Phase 274.W2 — create a node on an explicit executor handle.
///
/// Used by per-tier setup functions (emitted by `nros codegen entry --lang
/// cpp` for multi-tier workspaces) where each tier's setup runs on the
/// tier's borrowed executor, not the global one. The executor handle is the
/// `void*` passed to the tier's `setup(void* executor)` callback.
///
/// @param out              Receives the initialized node.
/// @param executor_handle  Explicit executor handle (from a tier setup param).
/// @param name             Node name.
/// @param ns               Node namespace, or nullptr for "/".
inline Result create_node_on(Node& out, void* executor_handle, const char* name,
                             const char* ns = nullptr) {
    if (executor_handle == nullptr) {
        return Result(ErrorCode::NotInitialized);
    }
    out.executor_handle_ = executor_handle;
    return Node::create(out, name, ns);
}

/// Phase 123.B.4 — value-returning factory. Wraps `create_node`
/// in the `ResultOf<Node>` envelope so users can write
/// `auto n = nros::make_node("foo");` in the rclcpp-style.
inline ResultOf<Node> make_node(const char* name, const char* ns = nullptr) {
    Node n;
    Result r = create_node(n, name, ns);
    if (!r.ok()) return ResultOf<Node>::error(r);
    return ResultOf<Node>::ok(::std::move(n));
}

// -- Executor::create_node implementation (requires full Node definition) --

inline Result Executor::create_node(Node& out, const char* name, const char* ns) {
    if (!initialized_) return Result(ErrorCode::NotInitialized);
    out.executor_handle_ = storage_;
    return Node::create(out, name, ns);
}

// -- Phase 104.C.9 — NodeBuilder ----------------------------------------
//
// Mirrors Rust's `Executor::node_builder(name).rmw(...).locator(...).
// domain_id(...).namespace(...).sched(...).build()` chain. The C++
// wrapper is value-typed and stack-allocated; it accumulates options
// into an inline `nros_cpp_node_options_t` and ships it to
// `nros_cpp_node_create_ex` on `.build()`.
//
// Usage:
// ```cpp
// nros::Node node;
// NROS_TRY(executor.node_builder("egress")
//              .rmw("cyclonedds")
//              .domain_id(0)
//              .build(node));
// ```

class NodeBuilder {
  public:
    NodeBuilder(void* executor_handle, const char* name)
        : executor_handle_(executor_handle), name_(name),
          options_(nros_cpp_node_get_default_options()) {}

    /// Bind this Node to the named RMW backend. The name must match a
    /// backend registered with `nros_rmw_cffi_register_named` (or its
    /// auto-ctor equivalent). Empty/nullptr selects the first-
    /// registered backend — the single-backend convenience path.
    NodeBuilder& rmw(const char* name) {
        copy_bounded(name, options_.rmw_name, &options_.rmw_name_len, NROS_CPP_RMW_NAME_LEN);
        return *this;
    }

    /// Override the Node's locator (`tcp/...`, `udp/...`, `serial:...`).
    /// Empty/nullptr inherits the executor's locator.
    NodeBuilder& locator(const char* loc) {
        copy_bounded(loc, options_.locator, &options_.locator_len, NROS_CPP_LOCATOR_LEN);
        return *this;
    }

    /// Override the Node's domain ID. Pass `NROS_CPP_DOMAIN_ID_INHERIT`
    /// (the default) to inherit from the executor.
    NodeBuilder& domain_id(uint32_t id) {
        options_.domain_id_override = id;
        return *this;
    }

    /// Set the Node's namespace (mirrors `rclcpp::Node`'s ctor). Empty
    /// or nullptr defaults to `"/"` at build time.
    NodeBuilder& namespace_(const char* ns) {
        copy_bounded(ns, options_.namespace_, &options_.namespace_len, NROS_CPP_NAMESPACE_LEN);
        return *this;
    }

    /// Bind every handle created via this Node to `sc_id` as its
    /// default SchedContext. 0 = executor default Fifo.
    NodeBuilder& sched(uint8_t sc_id) {
        options_.sched_context_id = sc_id;
        return *this;
    }

    /// Materialize the Node.
    Result build(Node& out) const {
        if (!executor_handle_) return Result(ErrorCode::NotInitialized);
        out.executor_handle_ = executor_handle_;
        nros_cpp_ret_t ret =
            nros_cpp_node_create_ex(executor_handle_, name_, &options_, &out.handle_);
        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

  private:
    static void copy_bounded(const char* src, uint8_t* dst, size_t* dst_len, size_t cap) {
        size_t n = 0;
        if (src != nullptr) {
            while (src[n] != '\0' && n < cap) {
                dst[n] = static_cast<uint8_t>(src[n]);
                ++n;
            }
        }
        // Zero out the tail so stale bytes don't leak across reuses.
        for (size_t i = n; i < cap; ++i) {
            dst[i] = 0;
        }
        *dst_len = n;
    }

    void* executor_handle_;
    const char* name_;
    nros_cpp_node_options_t options_;
};

inline NodeBuilder Executor::node_builder(const char* name) {
    return NodeBuilder(initialized_ ? handle() : nullptr, name);
}

} // namespace nros

// ============================================================================
// rclcpp:: — the ROS 2 spelling (RFC-0089 stage 6; phase-427 W1-W3, W7)
// ============================================================================

namespace rclcpp {

/// `rclcpp::Node` — the ROS 2 spelling, and the SAME TYPE as `nros::Node`.
/// `std::is_same<rclcpp::Node, nros::Node>::value` is true: one class, one set
/// of entities, one arena registration path, one parameter facade. See the
/// class above for why the definition lives in `nros::` and the alias here
/// rather than the other way round (it is a measurement about
/// `scripts/api-parity.py`'s namespace roots, not a preference).
///
/// UNCONDITIONAL, unlike the shim class it replaces. That class was declared
/// only where `<memory>`/`<string>`/`<vector>`/`<functional>` were, so a
/// freestanding target had the node and NOT its ROS 2 name — the two
/// vocabularies split exactly where the port matters most. The hosted-shape
/// METHODS are still gated; the NAME is not.
using Node = ::nros::Node;

/// The process-level verbs — `rclcpp::init()`, `rclcpp::ok()`,
/// `rclcpp::shutdown()`, `rclcpp::spin_once()` — are declared in `nros.hpp`,
/// which is where the free functions they name live. This header carries only
/// what the node itself needs.

} // namespace rclcpp

#endif // NROS_CPP_NODE_HPP
