// nros-cpp: Future<T> -- single-shot deferred result
// Freestanding C++ -- no exceptions, no STL required

/**
 * @file future.hpp
 * @ingroup grp_executor
 * @brief `rclcpp::Future<T>` — single-shot deferred result.
 */

#ifndef NROS_RCLCPP_FUTURE_HPP
#define NROS_RCLCPP_FUTURE_HPP

#include <cstdint>
#include <cstddef>

#include "rclcpp/result.hpp"
#include "rclcpp/size_bound.hpp" // rclcpp::rx_buffer_capacity<T> — the receive-buffer size
// issue 1996 — `refuse<T>`, `say_refused` and the `NROS_RCLCPP_REFUSE_FUTURE_GET*`
// messages; it also brings `std_detect.hpp`, which defines the capability
// macros `get()` and `wait_for()` are gated on.
#include "rclcpp/logging.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);
/// Monotonic time in nanoseconds. Used by Future::wait() to budget its
/// spin loop by wall-clock, not iteration count (Phase 89.2).
uint64_t nros_cpp_time_ns(void);
}

// phase-428 (RFC-0089): the entity types that reach into this one's private
// constructor are DEFINED in the upstream namespace now, so the friend
// declarations below name them there. A qualified friend cannot introduce a
// name, hence these declarations.
namespace rclcpp {
template <typename M> class Subscription;
} // namespace rclcpp

namespace rclcpp_action {
template <typename A> class Client;
} // namespace rclcpp_action

namespace rclcpp {

// phase-456 W9 — `send_request` and the `Future` it hands back moved off
// `rclcpp::Client<S>` onto `rclcpp::PollClient<S>`, the half that owns the
// `RmwServiceClient` this future takes from. The friend list FOLLOWS the method:
// the poll client is declared here, and `rclcpp::Client<S>` left both this
// declaration and the friend list below, because a friendship nothing uses is a
// claim about the code that is not true.
template <typename S> class PollClient;

namespace detail {

/// Issue 2009 — the id a backend reports when it cannot assign one: the RMW
/// ABI's `NROS_RMW_SEQUENCE_ID_UNKNOWN` (`INT64_MIN`). `polling_client.hpp`
/// asserts it equals the FFI's `NROS_CPP_SEQUENCE_ID_UNKNOWN`.
constexpr int64_t SEQUENCE_ID_UNKNOWN = (-9223372036854775807LL - 1);

/// Issue 2009 — stale replies one poll drops before reporting "nothing yet";
/// the twin of Rust's `nros_rmw::MAX_STALE_DRAIN_PER_POLL`.
constexpr int MAX_STALE_DRAIN_PER_POLL = 8;

/// The take hook a service future polls: one reply, its length AND the id of
/// the request it answers (`nros_cpp_service_client_take_response_seq`).
using TakeSeqFn = nros_cpp_ret_t (*)(void*, uint8_t*, size_t, size_t*, int64_t*);

/// Issue 2011 — what an ACTION future polls: one pass of the action client's
/// own dispatch (`nros_cpp_action_client_poll`), which takes any reply off the
/// wire and hands it to the client's callbacks — and so, keyed by goal id, to
/// the future waiting on that goal. A reply the executor dispatched during
/// `spin_once` arrives the same way, so a future gets its reply whichever path
/// took it.
using PumpFn = nros_cpp_ret_t (*)(void*);

/// Issue 2009 — the part of a `Future` the client's pending list links. Not a
/// template, so futures of different reply capacities on one client share a
/// list, and a future that takes ANOTHER future's reply can hand it over
/// without knowing that future's type.
struct PendingList;
struct PendingLink {
    PendingLink* next = nullptr;
    /// The list this future is on while its request is pending; nullptr for an
    /// unkeyed future, or once the future is consumed or cancelled.
    PendingList* list = nullptr;
    /// The client storage the future takes from (the poll client's, or the
    /// action client's for an unkeyed future).
    void* storage = nullptr;
    int64_t seq = SEQUENCE_ID_UNKNOWN;
    /// Issue 2011 — an ACTION future is keyed by its GOAL instead of a
    /// sequence id: an action client's acceptance and result arrive attributed
    /// to a goal (the Rust core correlates each sub-client's replies by id,
    /// issue 2009), so the goal is the key a reply carries up here.
    bool by_goal = false;
    uint8_t goal_id[16] = {};
    uint8_t* buf = nullptr; ///< the future's own receive buffer
    size_t cap = 0;
    size_t len = 0;
    bool ready = false;
    /// The client went away while the request was pending: never answerable.
    bool orphaned = false;
};

/// Issue 2009 — the requests ONE poll client has in flight, as an intrusive
/// list of the futures waiting on them, each keyed by its request's sequence
/// id. A reply is delivered only to the future whose id it carries; a reply
/// naming no pending future (a removed or destroyed one's) is dropped.
///
/// The same rule as Rust's `nros_rmw::RequestCorrelator`, which every other
/// client surface uses. It lives in the futures here rather than in a table
/// because each future already owns the buffer its reply must land in.
///
/// BOUNDED, NO HEAP, NO CAPACITY TO MIS-SIZE: an entry is a future the caller
/// already holds, so the list is as long as the futures in flight. What caps
/// that is the transport's own outstanding-request limit (zenoh
/// `NROS_ZENOH_MAX_PENDING_GETS`, Cyclone's 8 per client): a send past it is
/// refused by the backend, and `send_request` then returns an empty future
/// (`valid()` false) without linking anything.
///
/// A backend that cannot assign ids (`SEQUENCE_ID_UNKNOWN`) gets one request
/// in flight per client — `can_send()` refuses a second while the first is
/// pending — and an unkeyed reply goes to the sole pending future. A removed
/// unkeyed request costs the next unkeyed reply (`orphans`), which is dropped
/// rather than delivered to a newer request it cannot be told apart from.
struct PendingList {
    PendingLink* head = nullptr;
    uint8_t orphans = 0;

    size_t count() const {
        size_t n = 0;
        for (PendingLink* l = head; l != nullptr; l = l->next)
            ++n;
        return n;
    }

    bool can_send() const {
        for (PendingLink* l = head; l != nullptr; l = l->next)
            if (l->seq == SEQUENCE_ID_UNKNOWN) return false;
        return true;
    }

    void link(PendingLink* l) {
        l->next = head;
        head = l;
    }

    /// Unlink `l` if present. `abandoned`: its request will never be taken
    /// through it (removed / destroyed), as opposed to answered.
    bool unlink(PendingLink* l, bool abandoned) {
        for (PendingLink** pp = &head; *pp != nullptr; pp = &(*pp)->next) {
            if (*pp == l) {
                *pp = l->next;
                l->next = nullptr;
                // A goal-keyed (action) future names its reply exactly, so its
                // removal costs no later reply (issue 2011).
                if (abandoned && !l->by_goal && l->seq == SEQUENCE_ID_UNKNOWN && orphans < 255)
                    ++orphans;
                return true;
            }
        }
        return false;
    }

    /// A moved future: the list must point at its new address.
    void relink(PendingLink* from, PendingLink* to) {
        for (PendingLink** pp = &head; *pp != nullptr; pp = &(*pp)->next) {
            if (*pp == from) {
                to->next = from->next;
                *pp = to;
                from->next = nullptr;
                return;
            }
        }
    }

    /// The pending future a reply carrying `seq` answers, or nullptr (stale:
    /// drop it). Does not unlink.
    PendingLink* resolve(int64_t seq) {
        if (seq == SEQUENCE_ID_UNKNOWN) {
            if (orphans > 0) {
                --orphans;
                return nullptr;
            }
            return (head != nullptr && head->next == nullptr) ? head : nullptr;
        }
        for (PendingLink* l = head; l != nullptr; l = l->next)
            if (l->seq == seq) return l;
        // The backend gave the REPLY an id but not the request: the sole
        // pending request is the only candidate.
        if (head != nullptr && head->next == nullptr && head->seq == SEQUENCE_ID_UNKNOWN)
            return head;
        return nullptr;
    }

    /// Issue 2011 — the pending ACTION future waiting on `goal`, or nullptr
    /// (nobody waits: the reply is dropped, never handed to another goal's
    /// future). Exact match only; does not unlink.
    PendingLink* resolve_goal(const uint8_t goal[16]) {
        for (PendingLink* l = head; l != nullptr; l = l->next) {
            if (!l->by_goal) continue;
            bool same = true;
            for (int i = 0; i < 16; ++i) {
                if (l->goal_id[i] != goal[i]) {
                    same = false;
                    break;
                }
            }
            if (same) return l;
        }
        return nullptr;
    }

    /// The poll client that owns this list MOVED to `to` (whose storage is
    /// `storage`): every pending future follows it.
    void move_to(PendingList& to, void* storage) {
        to.head = head;
        to.orphans = orphans;
        for (PendingLink* l = head; l != nullptr; l = l->next) {
            l->list = &to;
            l->storage = storage;
        }
        head = nullptr;
        orphans = 0;
    }

    /// Forget every pending request, as `remove_pending_request` does for one:
    /// each waiting future becomes invalid and its reply, if it still comes, is
    /// dropped. Returns how many were pending.
    size_t prune() {
        size_t n = 0;
        while (head != nullptr) {
            PendingLink* l = head;
            (void)unlink(l, true);
            l->list = nullptr;
            l->orphaned = true;
            ++n;
        }
        return n;
    }

    /// The poll client that owns this list is going away: every pending future
    /// becomes invalid (`valid()` false) instead of pointing at freed storage.
    void orphan_all() {
        PendingLink* l = head;
        while (l != nullptr) {
            PendingLink* next = l->next;
            l->next = nullptr;
            l->list = nullptr;
            l->storage = nullptr;
            l->orphaned = true;
            l = next;
        }
        head = nullptr;
        orphans = 0;
    }

    /// Hand a reply to `owner`, which is answered and leaves the list.
    void deliver(PendingLink* owner, const uint8_t* data, size_t len) {
        const size_t n = len < owner->cap ? len : owner->cap;
        if (owner->buf != data) {
            for (size_t i = 0; i < n; ++i)
                owner->buf[i] = data[i];
        }
        owner->len = n;
        owner->ready = true;
        (void)unlink(owner, false);
    }
};

} // namespace detail

/// Single-shot deferred result for request/response operations.
///
/// Returned by PollClient<S>::send_request (phase-456 W9; it was
/// `Client<S>::send_request` before the split). The future is consumed
/// when the result is taken -- move-only, single-shot.
///
/// Usage:
/// ```cpp
/// auto fut = client.send_request(req);
/// ResponseType resp;
/// RCLCPP_TRY(fut.wait(executor.handle(), 5000, resp));
/// ```
///
/// @tparam Cap  Bytes the RECEIVE buffer holds. Defaults to `T`'s own derived
///              bound where it has one, and to the legacy
///              `T::SERIALIZED_SIZE_MAX` estimate where it does not. The
///              `_sized` escape hatches are `Client::send_request_sized` and
///              `ActionClient::get_result_future_sized`, which hand back a
///              `Future<T, N>` (issue 0964). This buffer IS the capacity:
///              under-sizing it truncates the reply.
///
/// A service future is KEYED BY ITS REQUEST'S SEQUENCE ID (issue 2009): any
/// number of futures may be in flight on one `PollClient`, each receives only
/// the reply to its own request whichever future happens to poll first, and a
/// reply that arrives after the future was cancelled, removed
/// (`remove_pending_request`) or destroyed is dropped — never handed to a later
/// future. See `detail::PendingList` for the bound and for backends that
/// cannot assign ids.
///
/// An ACTION future (`rclcpp_action::Client<A>::send_goal_future` /
/// `get_result_future`) is KEYED BY ITS GOAL the same way (issue 2011): it is
/// on its client's list of acceptances or of results, it receives only its own
/// goal's, and a reply for a goal nobody waits on — or arriving after the
/// future was cancelled or destroyed — is dropped. Two action clients keep two
/// sets of lists, so they never see each other's replies.
template <typename T, size_t Cap = ::rclcpp::rx_buffer_capacity<T>::value> class Future {
  public:
    /// Check if the result has arrived (non-blocking).
    ///
    /// For a service future this takes replies off the client until its own
    /// arrives: one for another pending future is handed to that future, one
    /// for no pending future is dropped (at most
    /// `detail::MAX_STALE_DRAIN_PER_POLL` per call).
    bool is_ready() {
        if (!live()) return false;
        if (link_.ready) return true;
        if (take_seq_fn_ != nullptr) return poll_keyed();
        if (pump_fn_ != nullptr) return poll_pumped();
        if (!take_fn_) return false;
        size_t len = 0;
        nros_cpp_ret_t ret = take_fn_(link_.storage, cached_buf_, sizeof(cached_buf_), &len);
        if (ret == 0 && len > 0) {
            link_.ready = true;
            link_.len = len < sizeof(cached_buf_) ? len : sizeof(cached_buf_);
            return true;
        }
        return false;
    }

    /// Take the result if ready, consuming the future.
    ///
    /// @param out  Output object (filled on success).
    /// @return Result::success() on ready, ErrorCode::Error if not ready or failed.
    Result try_take(T& out) {
        if (!live()) return Result(ErrorCode::Error);
        if (!link_.ready && !is_ready()) return Result(ErrorCode::Error);
        slot_ = -1; // consume
        if (T::ffi_deserialize(cached_buf_, link_.len, &out) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Block until the result arrives, spinning the executor.
    ///
    /// @param executor_handle  Raw executor handle (Executor::handle() or global).
    /// @param timeout_ms       Maximum wait time in milliseconds.
    /// @param out              Output object (filled on success).
    /// @param poll_ms          Per-iteration spin_once timeout in ms
    ///                         (default 10). Smaller values give lower
    ///                         latency when the reply arrives mid-step
    ///                         but wake the executor more often; larger
    ///                         values reduce wakeups for long waits.
    /// @return Result::success(), ErrorCode::Timeout, or error.
    Result wait(void* executor_handle, uint32_t timeout_ms, T& out, uint32_t poll_ms = 10) {
        if (!live()) return Result(ErrorCode::Error);
        if (poll_ms == 0) poll_ms = 1;
        // Phase 89.2: budget by wall-clock. Accumulating `step` per iteration
        // breaks when the underlying `zpico_spin_once` returns early on a
        // signaled condvar (keep-alives, discovery gossip) — the 500-step
        // default-timeout loop can then collapse into milliseconds and
        // return ErrorCode::Timeout before the reply has a chance to land.
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(timeout_ms) * 1000000ULL;
        while (true) {
            nros_cpp_ret_t ret = nros_cpp_spin_once(executor_handle, static_cast<int32_t>(poll_ms));
            // Transient conditions: keep polling. Anything else propagates.
            // - Ok (0): nothing to dispatch this round.
            // - Timeout (-2): spin_once returned after its timeout — normal.
            // - TryAgain (-6): transport hint to retry.
            if (ret != 0 && ret != static_cast<nros_cpp_ret_t>(ErrorCode::Timeout) &&
                ret != static_cast<nros_cpp_ret_t>(ErrorCode::TryAgain)) {
                return Result(ret);
            }
            if (is_ready()) return try_take(out);
            const uint64_t now_ns = nros_cpp_time_ns();
            if (now_ns - start_ns >= budget_ns) break;
        }
        return Result(ErrorCode::Timeout);
    }

    /// Cancel the pending operation (idempotent). A service future leaves its
    /// client's pending list, so its reply — if it still comes — is dropped
    /// (issue 2009).
    void cancel() {
        detach(true);
        slot_ = -1;
    }

    /// `std::future::valid()` — true until the result is taken or the future
    /// is cancelled (issue 1996). The negation of @ref is_consumed.
    ///
    /// Issue 2009 — also false once the client it was sent on was destroyed
    /// (or moved over) while the request was pending.
    bool valid() const { return live(); }

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    /// `FutureAndRequestId::get()` — issue 1996. ADOPT-BOUNDED.
    ///
    /// Upstream returns the reply as an owning `std::shared_ptr<Response>`, and
    /// so does this: one heap copy of the reply, deserialized into a
    /// `std::make_shared<T>()`, which the caller may keep. It consumes the
    /// future, as `std::future::get()` does.
    ///
    /// THE BOUND: upstream BLOCKS until the reply arrives; this never waits.
    /// A wait that does not drive the executor is what RFC-0021 forbids, and
    /// a future has no executor to drive on its own. So call it where the
    /// stock examples do — after `rclcpp::spin_until_future_complete(node,
    /// fut)` returned `SUCCESS`, or `wait_for(...)` returned `ready`. Called
    /// before the reply (or on a consumed future) it says
    /// `NROS_RCLCPP_REFUSE_FUTURE_GET_NOT_READY_RUNTIME` and returns nullptr;
    /// a reply that fails to deserialize also returns nullptr.
    ///
    /// Absent on a freestanding build, where there is no `<memory>`: there the
    /// name is a `static_assert` naming `try_take` (`NROS_RCLCPP_REFUSE_FUTURE_GET`).
    ::std::shared_ptr<T> get() {
        if (!live() || (!link_.ready && !is_ready())) {
            NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_REFUSE_FUTURE_GET_NOT_READY_RUNTIME);
            return ::std::shared_ptr<T>();
        }
        ::std::shared_ptr<T> out = ::std::make_shared<T>();
        if (!try_take(*out).ok()) return ::std::shared_ptr<T>();
        return out;
    }
#else
    /// Freestanding: `get()` is REFUSED at compile time — no `<memory>`, no
    /// allocator. The message names `try_take` and `wait` (issue 1996).
    template <typename U = void> void get() {
        static_assert(::rclcpp::detail::refuse<U>::value, NROS_RCLCPP_REFUSE_FUTURE_GET);
    }
#endif // NROS_CPP_HAS_SHARED_PTR

#if defined(NROS_CPP_HAS_STD_FUTURE) // hosted-family: future-interop
    /// `FutureAndRequestId::wait_for(duration)` — issue 1996. ADOPT-BOUNDED.
    ///
    /// Returns `std::future_status::ready` once the reply has arrived and
    /// `std::future_status::timeout` when the budget runs out first. It DRIVES
    /// the executor the client was created on while it waits, in `spin_once`
    /// slices of at most 10 ms — the wait RFC-0021 allows — so other entities
    /// keep being served and the reply can land on a single-threaded
    /// transport. Upstream's `wait_for` merely blocks on a promise a second
    /// thread fulfils.
    ///
    /// The budget is rounded UP to milliseconds and clamped to `UINT32_MAX`;
    /// a zero or negative duration is one non-waiting check, as for
    /// `std::future::wait_for`. Never `deferred`. On a consumed or cancelled
    /// future (where upstream's is undefined) it returns `timeout` at once.
    /// A future with no executor (default-constructed) does not wait: it checks once.
    template <typename Rep, typename Period>
    ::std::future_status wait_for(::std::chrono::duration<Rep, Period> timeout) {
        if (!live()) return ::std::future_status::timeout;
        if (link_.ready || is_ready()) return ::std::future_status::ready;
        const int64_t ms = ::rclcpp::detail::chrono_wait_budget_ms(timeout);
        if (ms <= 0 || executor_ == nullptr) {
            if (executor_ != nullptr) (void)nros_cpp_spin_once(executor_, 0);
            return is_ready() ? ::std::future_status::ready : ::std::future_status::timeout;
        }
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(ms) * 1000000ULL;
        while (true) {
            const uint64_t elapsed_ns = nros_cpp_time_ns() - start_ns;
            if (elapsed_ns >= budget_ns) break;
            const uint64_t left_ms = (budget_ns - elapsed_ns + 999999ULL) / 1000000ULL;
            const int32_t slice = left_ms < 10ULL ? static_cast<int32_t>(left_ms) : 10;
            (void)nros_cpp_spin_once(executor_, slice);
            if (is_ready()) return ::std::future_status::ready;
        }
        return is_ready() ? ::std::future_status::ready : ::std::future_status::timeout;
    }
#endif // NROS_CPP_HAS_STD_FUTURE

    /// Check if the future has been consumed or cancelled.
    bool is_consumed() const { return !live(); }

    // Move semantics (non-copyable, single-shot). A linked future's list entry
    // FOLLOWS it to the new address (issue 2009), so a future may be returned
    // by value and stored wherever the caller likes while it is in flight.
    Future(Future&& other) noexcept
        : take_fn_(other.take_fn_), take_seq_fn_(other.take_seq_fn_), pump_fn_(other.pump_fn_),
          executor_(other.executor_), slot_(other.slot_) {
        adopt(other);
    }

    Future& operator=(Future&& other) noexcept {
        if (this != &other) {
            cancel();
            take_fn_ = other.take_fn_;
            take_seq_fn_ = other.take_seq_fn_;
            pump_fn_ = other.pump_fn_;
            executor_ = other.executor_;
            slot_ = other.slot_;
            adopt(other);
        }
        return *this;
    }

    ~Future() { cancel(); }

    /// Default constructor -- creates an empty/consumed future.
    Future()
        : take_fn_(nullptr), take_seq_fn_(nullptr), pump_fn_(nullptr), executor_(nullptr),
          slot_(-1) {
        link_.buf = cached_buf_;
        link_.cap = sizeof(cached_buf_);
    }

  private:
    Future(const Future&) = delete;
    Future& operator=(const Future&) = delete;

    template <typename S> friend class ::rclcpp::PollClient;
    template <typename A> friend class ::rclcpp_action::Client;

    using TryRecvFn = nros_cpp_ret_t (*)(void*, uint8_t*, size_t, size_t*);

    // `executor` is what `wait_for` drives (issue 1996); nullptr = no
    // executor known, and `wait_for` then checks once instead of waiting.
    // An UNKEYED future: the take hook carries no request id, so the future
    // trusts whatever reply it takes. Nothing in-tree builds one any more —
    // the action futures are goal-keyed since issue 2011 — but the shape stays
    // for a caller with exactly one request in flight.
    Future(void* storage, TryRecvFn fn, int slot, void* executor = nullptr)
        : take_fn_(fn), take_seq_fn_(nullptr), pump_fn_(nullptr), executor_(executor), slot_(slot) {
        link_.buf = cached_buf_;
        link_.cap = sizeof(cached_buf_);
        link_.storage = storage;
    }

    // Issue 2009 — a KEYED future: the request went out as `seq`, and the
    // future joins `list`, the pending list of the client it was sent on.
    Future(void* storage, ::rclcpp::detail::TakeSeqFn fn, int64_t seq,
           ::rclcpp::detail::PendingList* list, void* executor)
        : take_fn_(nullptr), take_seq_fn_(fn), pump_fn_(nullptr), executor_(executor), slot_(0) {
        link_.buf = cached_buf_;
        link_.cap = sizeof(cached_buf_);
        link_.storage = storage;
        link_.seq = seq;
        link_.list = list;
        if (list != nullptr) list->link(&link_);
    }

    // Issue 2011 — a GOAL-KEYED action future: it joins `list` (the action
    // client's acceptances or results) under `goal`. The client's callbacks
    // deliver into it (`PendingList::resolve_goal` + `deliver`); `pump` is the
    // client's own dispatch, for a reply no `spin_once` has taken yet.
    Future(void* storage, ::rclcpp::detail::PumpFn pump, const uint8_t goal[16],
           ::rclcpp::detail::PendingList* list, void* executor)
        : take_fn_(nullptr), take_seq_fn_(nullptr), pump_fn_(pump), executor_(executor), slot_(0) {
        link_.buf = cached_buf_;
        link_.cap = sizeof(cached_buf_);
        link_.storage = storage;
        link_.by_goal = true;
        for (int i = 0; i < 16; ++i)
            link_.goal_id[i] = goal[i];
        link_.list = list;
        if (list != nullptr) list->link(&link_);
    }

    bool live() const { return slot_ >= 0 && !link_.orphaned; }

    /// Take replies off the client until this future's own arrives; hand each
    /// other pending future its reply, drop the rest.
    bool poll_keyed() {
        ::rclcpp::detail::PendingList* list = link_.list;
        if (list == nullptr) return false;
        for (int i = 0; i < ::rclcpp::detail::MAX_STALE_DRAIN_PER_POLL; ++i) {
            size_t len = 0;
            int64_t seq = ::rclcpp::detail::SEQUENCE_ID_UNKNOWN;
            nros_cpp_ret_t ret =
                take_seq_fn_(link_.storage, cached_buf_, sizeof(cached_buf_), &len, &seq);
            if (ret != 0) break; // TRY_AGAIN (nothing yet) or a transport error
            ::rclcpp::detail::PendingLink* owner = list->resolve(seq);
            if (owner != nullptr) list->deliver(owner, cached_buf_, len);
            if (link_.ready) return true;
        }
        return link_.ready;
    }

    /// Issue 2011 — one pass of the action client's dispatch; whatever it
    /// takes reaches its own goal's future through the client's callbacks.
    bool poll_pumped() {
        if (link_.list == nullptr || link_.storage == nullptr) return link_.ready;
        (void)pump_fn_(link_.storage);
        return link_.ready;
    }

    /// Leave the client's pending list, if still on it. `abandoned`: the
    /// request will never be answered through this future.
    void detach(bool abandoned) {
        if (link_.list != nullptr && !link_.ready) (void)link_.list->unlink(&link_, abandoned);
        link_.list = nullptr;
    }

    /// Take over `other`'s state and list position; `other` becomes empty.
    void adopt(Future& other) {
        link_.buf = cached_buf_;
        link_.cap = sizeof(cached_buf_);
        link_.storage = other.link_.storage;
        link_.seq = other.link_.seq;
        link_.by_goal = other.link_.by_goal;
        for (int i = 0; i < 16; ++i)
            link_.goal_id[i] = other.link_.goal_id[i];
        link_.ready = other.link_.ready;
        link_.orphaned = other.link_.orphaned;
        link_.len = other.link_.len;
        link_.list = other.link_.list;
        link_.next = nullptr;
        if (link_.ready) {
            for (size_t i = 0; i < link_.len; ++i)
                cached_buf_[i] = other.cached_buf_[i];
        }
        if (link_.list != nullptr && !link_.ready) link_.list->relink(&other.link_, &link_);
        other.link_.list = nullptr;
        other.link_.ready = false;
        other.link_.next = nullptr;
        other.slot_ = -1;
    }

    TryRecvFn take_fn_;
    ::rclcpp::detail::TakeSeqFn take_seq_fn_;
    ::rclcpp::detail::PumpFn pump_fn_;
    void* executor_;
    int slot_;
    /// Issue 2009 — the entry the client's pending list links, plus the client
    /// storage this future takes from.
    ::rclcpp::detail::PendingLink link_;
    uint8_t cached_buf_[Cap];
};

} // namespace rclcpp
#endif // NROS_RCLCPP_FUTURE_HPP
