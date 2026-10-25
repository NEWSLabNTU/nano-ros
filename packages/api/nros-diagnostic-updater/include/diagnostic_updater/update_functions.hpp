// SPDX-License-Identifier: Apache-2.0
//
// update_functions.hpp — phase-488; the tasks, issue 2053.
//
// Upstream's `<diagnostic_updater/update_functions.hpp>`: the statistics tasks
// `FrequencyStatus`, `TimeStampStatus` and `Heartbeat`,
// each a `DiagnosticTask` an `Updater` adds as an object. Their levels,
// messages and key names are upstream's, so a monitor reading `/diagnostics`
// sees the same report from a ported node.
//
// No heap allocation per `tick()`: `FrequencyStatus` sizes its window once, in
// its constructor, and `tick()` only counts.
//
// ADOPT-BOUNDED on one point. Upstream guards each task with a `std::mutex`,
// so `tick()` may run on one thread while the updater's timer runs `run()` on
// another. These tasks take no lock: `<mutex>` is not available on every
// target this header compiles for (the FreeRTOS `rclcpp-compat-smoke` image is
// one), and nano-ros's executor dispatches a node's timers and subscriptions
// from one thread. A node that ticks from a thread of its own must serialise
// that call with the updater itself.
//
// The clock is upstream's default, a system clock, unless the constructor is
// handed one. Upstream's parameter is a `rclcpp::Clock::SharedPtr`; here it is
// a `const rclcpp::Clock*`, because `node->get_clock()` returns one (there is no
// allocator to hand a shared pointer from, RFC-0022), so `node->get_clock()`
// still passes the node's clock unchanged. The clock must outlive the task.

#ifndef NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP
#define NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP

#include <cmath>
#include <limits>
#include <string>
#include <vector>

#include <rclcpp/rclcpp.hpp>

#include "diagnostic_updater/diagnostic_updater.hpp"

namespace diagnostic_updater {

namespace detail {

/// The clock a task reads: the caller's, or one the task owns. A task holding
/// a pointer to its own member must not be copied, which upstream's are not
/// either (each holds a mutex).
class TaskClock {
  public:
    explicit TaskClock(const rclcpp::Clock* clock)
        : own_(RCL_SYSTEM_TIME), clock_(clock != nullptr ? clock : &own_) {}
    TaskClock(const TaskClock&) = delete;
    TaskClock& operator=(const TaskClock&) = delete;

    rclcpp::Time now() const { return clock_->now(); }

  private:
    rclcpp::Clock own_;
    const rclcpp::Clock* clock_;
};

} // namespace detail

/// Upstream's `FrequencyStatusParam`. The two bounds are POINTERS, as
/// upstream's are, so the caller can move the band after construction; both
/// must outlive the task.
struct FrequencyStatusParam {
    FrequencyStatusParam(double* min_freq, double* max_freq, double tolerance = 0.1,
                         int window_size = 5)
        : min_freq_(min_freq), max_freq_(max_freq), tolerance_(tolerance),
          window_size_(window_size) {}

    double* min_freq_;
    double* max_freq_;
    /// Fraction of the band accepted outside it: a rate passes when it lies in
    /// `[min * (1 - tolerance), max * (1 + tolerance)]`.
    double tolerance_;
    /// Number of updater runs the rate is averaged over.
    int window_size_;
};

/// Upstream's `FrequencyStatus`: counts `tick()` calls and, on each run,
/// reports the rate over the last `window_size` runs against the band.
///
/// ERROR "No events recorded." when nothing ticked across the window, WARN
/// "Frequency too low." / "Frequency too high." outside the band, OK "Desired
/// frequency met" inside it.
class FrequencyStatus : public DiagnosticTask {
  public:
    FrequencyStatus(const FrequencyStatusParam& params, std::string name,
                    const rclcpp::Clock* clock = nullptr)
        : DiagnosticTask(name), params_(params), clock_(clock),
          times_(static_cast<size_t>(params.window_size_ > 0 ? params.window_size_ : 1)),
          seq_nums_(times_.size()) {
        clear();
    }

    explicit FrequencyStatus(const FrequencyStatusParam& params,
                             const rclcpp::Clock* clock = nullptr)
        : FrequencyStatus(params, "Frequency Status", clock) {}

    /// Restart the window from now.
    void clear() {
        const rclcpp::Time now = clock_.now();
        count_ = 0;
        for (size_t i = 0; i < times_.size(); ++i) {
            times_[i] = now;
            seq_nums_[i] = 0;
        }
        hist_indx_ = 0;
    }

    /// Record one event.
    void tick() { ++count_; }

    void run(DiagnosticStatusWrapper& stat) override {
        const rclcpp::Time curtime = clock_.now();
        const int curseq = count_;
        const int events = curseq - seq_nums_[hist_indx_];
        const double window = curtime.seconds() - times_[hist_indx_].seconds();
        const double freq = events / window;
        seq_nums_[hist_indx_] = curseq;
        times_[hist_indx_] = curtime;
        hist_indx_ = (hist_indx_ + 1) % times_.size();

        if (events == 0) {
            stat.summary(ERROR, "No events recorded.");
        } else if (freq < *params_.min_freq_ * (1 - params_.tolerance_)) {
            stat.summary(WARN, "Frequency too low.");
        } else if (freq > *params_.max_freq_ * (1 + params_.tolerance_)) {
            stat.summary(WARN, "Frequency too high.");
        } else {
            stat.summary(OK, "Desired frequency met");
        }

        stat.addf("Events in window", "%d", events);
        stat.addf("Events since startup", "%d", count_);
        stat.addf("Duration of window (s)", "%f", window);
        stat.addf("Actual frequency (Hz)", "%f", freq);
        if (*params_.min_freq_ == *params_.max_freq_) {
            stat.addf("Target frequency (Hz)", "%f", *params_.min_freq_);
        }
        if (*params_.min_freq_ > 0) {
            stat.addf("Minimum acceptable frequency (Hz)", "%f",
                      *params_.min_freq_ * (1 - params_.tolerance_));
        }
        if (std::isfinite(*params_.max_freq_)) {
            stat.addf("Maximum acceptable frequency (Hz)", "%f",
                      *params_.max_freq_ * (1 + params_.tolerance_));
        }
    }

  private:
    const FrequencyStatusParam params_;
    detail::TaskClock clock_;
    int count_ = 0;
    std::vector<rclcpp::Time> times_;
    std::vector<int> seq_nums_;
    size_t hist_indx_ = 0;
};

/// Upstream's `TimeStampStatusParam`: the accepted delay of a stamp behind now,
/// in seconds. A negative delay is a stamp in the future.
struct TimeStampStatusParam {
    TimeStampStatusParam(const double min_acceptable = -1, const double max_acceptable = 5)
        : max_acceptable_(max_acceptable), min_acceptable_(min_acceptable) {}

    double max_acceptable_;
    double min_acceptable_;
};

/// Upstream's `TimeStampStatus`: on each `tick(stamp)` records how far the
/// stamp lies behind now, and on each run reports the extremes since the last.
///
/// WARN "No data since last update." with nothing ticked; ERROR "Timestamps
/// too far in future seen." / "Timestamps too far in past seen." / "Zero
/// timestamp seen." for a delay outside the band or a zero stamp; otherwise OK
/// "Timestamps are reasonable.".
class TimeStampStatus : public DiagnosticTask {
  public:
    TimeStampStatus(const TimeStampStatusParam& params, std::string name,
                    const rclcpp::Clock* clock = nullptr)
        : DiagnosticTask(name), params_(params), clock_(clock) {}

    explicit TimeStampStatus(const TimeStampStatusParam& params,
                             const rclcpp::Clock* clock = nullptr)
        : TimeStampStatus(params, "Timestamp Status", clock) {}

    explicit TimeStampStatus(const rclcpp::Clock* clock = nullptr)
        : TimeStampStatus(TimeStampStatusParam(), clock) {}

    /// Record a stamp, in seconds since the clock's epoch. `0` is upstream's
    /// "no stamp" and is reported as such.
    void tick(double stamp) {
        if (stamp == 0) {
            zero_seen_ = true;
        } else {
            const double delta = clock_.now().seconds() - stamp;
            if (!deltas_valid_ || delta > max_delta_) {
                max_delta_ = delta;
            }
            if (!deltas_valid_ || delta < min_delta_) {
                min_delta_ = delta;
            }
            deltas_valid_ = true;
        }
    }

    void tick(const rclcpp::Time& t) { tick(t.seconds()); }

    void run(DiagnosticStatusWrapper& stat) override {
        stat.summary(OK, "Timestamps are reasonable.");
        if (!deltas_valid_) {
            stat.summary(WARN, "No data since last update.");
        } else {
            if (min_delta_ < params_.min_acceptable_) {
                stat.summary(ERROR, "Timestamps too far in future seen.");
                early_count_++;
            }
            if (max_delta_ > params_.max_acceptable_) {
                stat.summary(ERROR, "Timestamps too far in past seen.");
                late_count_++;
            }
            if (zero_seen_) {
                stat.summary(ERROR, "Zero timestamp seen.");
                zero_count_++;
            }
        }

        stat.addf("Earliest timestamp delay:", "%f", min_delta_);
        stat.addf("Latest timestamp delay:", "%f", max_delta_);
        stat.addf("Earliest acceptable timestamp delay:", "%f", params_.min_acceptable_);
        stat.addf("Latest acceptable timestamp delay:", "%f", params_.max_acceptable_);
        stat.add("Late diagnostic update count:", late_count_);
        stat.add("Early diagnostic update count:", early_count_);
        stat.add("Zero seen diagnostic update count:", zero_count_);

        deltas_valid_ = false;
        min_delta_ = 0;
        max_delta_ = 0;
        zero_seen_ = false;
    }

  private:
    TimeStampStatusParam params_;
    detail::TaskClock clock_;
    int early_count_ = 0;
    int late_count_ = 0;
    int zero_count_ = 0;
    bool zero_seen_ = false;
    double max_delta_ = 0;
    double min_delta_ = 0;
    bool deltas_valid_ = false;
};

/// Upstream's `Heartbeat`: always OK "Alive".
class Heartbeat : public DiagnosticTask {
  public:
    Heartbeat() : DiagnosticTask("Heartbeat") {}

    void run(DiagnosticStatusWrapper& stat) override { stat.summary(OK, "Alive"); }
};

} // namespace diagnostic_updater

#endif // NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP
