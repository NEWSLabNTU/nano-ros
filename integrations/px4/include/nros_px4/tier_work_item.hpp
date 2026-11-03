// phase-521 W8 (b), RFC-0113 D9 -- a PX4 tier is a work item.
//
// The generated PX4 entry (`nros dev codegen entry --lang cpp` for a PX4 module)
// emits one `TierWorkItem` subclass per tier, constructed on the work queue
// `nros_orchestration_ir::px4::px4_work_queues` picks for the tier's rank. This
// header is the runtime half: it opens the tier's executor, installs the
// tier's park slot, and drives one executor pass per `Run()`.
//
// A tier never parks a thread. Its wait is the work item's schedule:
//
//   * a DEADLINE -- the executor calls the park slot with the bound it would
//     have slept (timers, the session's next internal event). The slot arms
//     `ScheduleDelayed(bound)`, or `ScheduleClear()` when the executor has no
//     deadline inside its idle budget, and answers
//     `NROS_PLATFORM_PARK_YIELDED`, so `spin_once` returns to the queue
//     (maintainer ruling 2026-11-03; no executor API widens, D7 stands).
//   * an ARRIVAL -- the executor's wake hook (`nros_cpp_executor_set_wake_hook`,
//     issue 2225) calls `notify()`, i.e. `ScheduleNow()`, after every signal
//     of the executor's wake object. Every backend reaches that object: uORB
//     from the publisher's context through its per-entity callback (issue
//     2218), zenoh and XRCE from their transport threads. An attributed
//     arrival signals only the owning tier's object, so only that tier runs.
//
// `Run()` is therefore: one pass that does not wait (the slot answers EXPIRED
// to the first park of a run, so the pass drains and dispatches without
// blocking), then passes until the slot yields, which is the moment the next
// wake is armed. A run always ends armed or cleared, never with work dropped.
//
// Written to PX4 convention (tab indent) because it compiles inside a PX4
// module; nothing here is reachable from a non-PX4 build.

#pragma once

// nano-ros first: PX4's `defines.h` defines `ERROR` as a macro, which
// breaks every later declaration that names it (`rclcpp_lifecycle`'s
// `ERROR` transition, for one).
#include <nros/platform.h>
#include <rclcpp/detail/nros_cpp_config_generated.h>
#include <rclcpp/rclcpp.hpp>

#include <px4_platform_common/atomic.h>
#include <px4_platform_common/log.h>
#include <px4_platform_common/px4_work_queue/ScheduledWorkItem.hpp>

#include <stddef.h>
#include <stdint.h>

#include <chrono>

namespace nros_px4
{

/// One tier of a generated PX4 entry, in the shape the other runners' tier
/// tables have (`NativeTierSpec`): the groups are FLAT `(node name, node
/// namespace, group)` triples, `n_groups` counts triples, and an empty array is
/// the wildcard (issue 1172).
struct TierSpec {
	const char *name;
	const char **groups;
	size_t n_groups;
	int32_t (*setup)(void *executor);
};

/// The executor storage a borrowed tier opens into: this build's own
/// `NROS_CPP_EXECUTOR_STORAGE_SIZE`, rounded to 8.
struct alignas(8) TierStorage {
	uint64_t words[(NROS_CPP_EXECUTOR_STORAGE_SIZE + 7u) / 8u];
};

/// What the tiers of one image share: the stop request and how many tiers are
/// still running, so the last one to stop is the one that cleans up.
struct ImageState {
	px4::atomic<bool> stopping{false};
	px4::atomic<int> running{0};
};

class TierWorkItem : public px4::ScheduledWorkItem
{
public:
	/// Open this tier's executor and run its setup.
	///
	/// The BOOT tier (`session == nullptr`) opens the image's session through
	/// `rclcpp::init_in`, so its executor is the global one; every other tier
	/// borrows that session into `storage` (`nros_cpp_executor_open_over_session`),
	/// exactly as the RTOS tier runners do. Setups run one after another, never
	/// overlapping (issue #144): the caller opens the tiers in table order.
	int32_t open(const TierSpec &spec, ImageState &image, TierStorage *storage, void *session,
		     const char *session_name, const char *node_namespace)
	{
		_spec = &spec;
		_image = &image;

		if (session == nullptr) {
			const ::rclcpp::Result r = ::rclcpp::init_in(nullptr, 0, session_name, node_namespace);

			if (!r.ok()) {
				return static_cast<int32_t>(r.raw());
			}

			_executor = ::rclcpp::global_handle();

		} else {
			if (storage == nullptr) {
				return NROS_CPP_RET_INVALID_ARGUMENT;
			}

			const nros_cpp_ret_t rc = nros_cpp_executor_open_over_session(session, "tier_node", 0, storage);

			if (rc != NROS_CPP_RET_OK) {
				return rc;
			}

			_executor = storage;
			_borrowed = true;
		}

		if (_executor == nullptr) {
			return NROS_CPP_RET_NOT_INIT;
		}

		if (spec.n_groups > 0 && spec.groups != nullptr) {
			const nros_cpp_ret_t rc = nros_cpp_executor_set_active_groups(_executor, spec.groups, spec.n_groups);

			if (rc != NROS_CPP_RET_OK) {
				return rc;
			}
		}

		// The executor's bound is in microseconds and so is `ScheduleDelayed`:
		// a granularity of 1 states that nothing between them rounds.
		const nros_cpp_ret_t prc = nros_cpp_executor_set_park_primitive(_executor, &TierWorkItem::park, this, 1);

		if (prc != NROS_CPP_RET_OK) {
			return prc;
		}

		// Issue 2225 -- every arrival for this tier, from any backend,
		// schedules THIS work item: the wake half of the yielding park slot.
		const nros_cpp_ret_t hrc = nros_cpp_executor_set_wake_hook(_executor, &TierWorkItem::notify_trampoline, this);

		if (hrc != NROS_CPP_RET_OK) {
			return hrc;
		}

		if (spec.setup != nullptr) {
			const int32_t rc = spec.setup(_executor);

			if (rc != 0) {
				return rc;
			}
		}

		image.running.fetch_add(1);
		return 0;
	}

	/// The session the boot tier opened, for the tiers that borrow it.
	void *session() const { return _executor ? nros_cpp_executor_session_handle(_executor) : nullptr; }

	/// First run: the tier's first pass, which also drives the session's
	/// handshake.
	void start() { ScheduleNow(); }

	/// An arrival for this tier: run it. Every wake source of a PX4 tier
	/// comes through here rather than calling `ScheduleNow()` itself, because
	/// the park slot's `ScheduleClear()` also removes a run already queued,
	/// and `_notified` is how the slot learns one was (issue 2218 binds
	/// uORB's `SubscriptionCallback` to this).
	void notify()
	{
		// The first arrival since the last run stamps the wake path's start
		// on the REAL clock (`steady_clock`, not PX4's `hrt`: SITL runs
		// lockstep, where `hrt` is virtual time that moves in steps).
		bool was = false;
		if (_notified.compare_exchange(&was, true)) {
			_notify_ns.store(now_ns());
		}
		ScheduleNow();
	}

	/// `notify()` as a C callback, for a backend's arrival path.
	static void notify_trampoline(void *ctx) { static_cast<TierWorkItem *>(ctx)->notify(); }

	/// The image is stopping: run once more to see it.
	void wake_for_stop() { ScheduleNow(); }

	/// Close the executor. Called by the last tier to stop, borrowed tiers
	/// first and the boot tier last (it owns the session).
	void close()
	{
		if (_executor != nullptr) {
			(void)nros_cpp_executor_set_wake_hook(_executor, nullptr, nullptr);
		}

		if (_borrowed && _executor != nullptr) {
			(void)nros_cpp_fini(_executor);
		}

		_executor = nullptr;
	}

	bool is_boot() const { return !_borrowed; }

	/// How many times this tier's work item has run a pass (stop excluded).
	uint32_t runs() const { return _runs.load(); }

	/// The longest real time, in microseconds, from an arrival's `notify()`
	/// to the run that served it (RFC-0113 D2's arrival->dispatch, less the
	/// pass itself).
	uint64_t wake_max_us() const { return _wake_max_us.load(); }

protected:
	TierWorkItem(const char *item_name, const px4::wq_config_t &queue) : ScheduledWorkItem(item_name, queue) {}

	/// Called by the last tier to stop, once every tier has cleared its
	/// schedule. The generated module closes the tiers and calls
	/// `exit_and_cleanup()` here.
	virtual void on_image_stopped() = 0;

	void Run() override
	{
		if (_executor == nullptr) {
			ScheduleClear();
			return;
		}

		if (_image->stopping.load()) {
			ScheduleClear();

			if (!_stopped) {
				_stopped = true;

				if (_image->running.fetch_sub(1) == 1) {
					on_image_stopped();
				}
			}

			return;
		}

		// Every wake that reaches the executor, counted for the idle-wake
		// witness (phase-521 W8 (e), RFC-0113 D2): a tier woken only by its
		// deadlines and its arrivals runs no more often than they occur.
		_runs.fetch_add(1);
		const uint64_t woke_ns = _notify_ns.load();
		if (woke_ns != 0) {
			_notify_ns.store(0);
			const uint64_t us = (now_ns() - woke_ns) / 1000u;
			if (us > _wake_max_us.load()) {
				_wake_max_us.store(us);
			}
		}

		// Cleared BEFORE the pass, so an arrival from here on is either
		// dispatched by it or seen by the park slot below.
		_notified.store(false);

		// RFC-0113 D2 -- the executor's own entry budget: up to its idle budget
		// when every source of work can end the wait, else the poll slice.
		const int32_t budget_ms = nros_cpp_entry_spin_budget_ms(_executor);
		_budget_us = static_cast<uint64_t>(budget_ms) * 1000u;
		_idle_budget = budget_ms >= NROS_CPP_ENTRY_IDLE_BUDGET_MS;

		for (unsigned pass = 0; pass < kMaxPassesPerRun; ++pass) {
			_first_park = (pass == 0);
			_yielded = false;
			const nros_cpp_ret_t rc = nros_cpp_spin_once(_executor, budget_ms);

			if (rc != NROS_CPP_RET_OK && !_reported) {
				_reported = true;
				PX4_ERR("nros tier `%s`: spin_once failed (%d)", _spec->name, static_cast<int>(rc));
			}

			if (_yielded) {
				return;
			}
		}

		// The executor never reached a park: work was due on every pass. Give
		// the queue back to the lower-priority items it serves and run again.
		ScheduleNow();
	}

private:
	/// More passes than this in one run means the executor keeps finding work
	/// due, and the queue's other items deserve their turn.
	static constexpr unsigned kMaxPassesPerRun = 16;

	/// The tier's park slot (`ParkUntilFn`). `bound_us` is the executor's
	/// next wake bound, relative, already rounded to the granularity above.
	static int8_t park(void *ctx, uint64_t bound_us)
	{
		TierWorkItem *self = static_cast<TierWorkItem *>(ctx);

		// The run's first park: this run was scheduled because something is
		// due (an arrival, a deadline, the first start), so the pass must run.
		// EXPIRED is "the park ended", which drains without blocking.
		if (self->_first_park) {
			self->_first_park = false;
			return NROS_PLATFORM_PARK_EXPIRED;
		}

		if (bound_us >= self->_budget_us && self->_idle_budget) {
			// No deadline inside the idle budget: only an arrival wakes this
			// tier, through `notify()`. `ScheduleClear()` also drops a run
			// queued since this one started, so one that `notify()` queued
			// is put back.
			self->ScheduleClear();

			if (self->_notified.load()) {
				self->ScheduleNow();
			}

		} else {
			self->ScheduleDelayed(bound_us > UINT32_MAX ? UINT32_MAX : static_cast<uint32_t>(bound_us));
		}

		self->_yielded = true;
		return NROS_PLATFORM_PARK_YIELDED;
	}

	const TierSpec *_spec{nullptr};
	ImageState *_image{nullptr};
	void *_executor{nullptr};
	bool _borrowed{false};
	bool _stopped{false};
	bool _reported{false};
	uint64_t _budget_us{0};
	bool _idle_budget{false};
	bool _first_park{false};
	bool _yielded{false};
	px4::atomic<bool> _notified{false};
	px4::atomic<uint32_t> _runs{0};
	px4::atomic<uint64_t> _notify_ns{0};
	px4::atomic<uint64_t> _wake_max_us{0};

	static uint64_t now_ns()
	{
		return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(
						     std::chrono::steady_clock::now().time_since_epoch())
						     .count());
	}
};

} // namespace nros_px4
