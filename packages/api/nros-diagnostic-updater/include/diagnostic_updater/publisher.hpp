// SPDX-License-Identifier: Apache-2.0
//
// publisher.hpp — issue 2053.
//
// Upstream's `<diagnostic_updater/publisher.hpp>`: the topic monitors a driver
// uses to report how often, and how late, it publishes.
//
//   * `HeaderlessTopicDiagnostic` — a `FrequencyStatus`, reported as
//     "<name> topic status" and added to the updater by its constructor.
//   * `TopicDiagnostic` — the same plus a `TimeStampStatus`, ticked with each
//     message's stamp.
//
// Upstream's third class, `DiagnosedPublisher<M>`, is NOT here. It stores a
// copy of the publisher's `SharedPtr`, and nano-ros's
// `rclcpp::Publisher<M>::SharedPtr` is an `rclcpp::Owned` handle, which moves
// and does not copy, so the class cannot be written with upstream's
// constructor. A node that wants one keeps its publisher and a
// `TopicDiagnostic`, and calls `tick(stamp)` beside `publish(msg)`; that is
// all `DiagnosedPublisher::publish` does.

#ifndef NROS_DIAGNOSTIC_UPDATER_PUBLISHER_HPP
#define NROS_DIAGNOSTIC_UPDATER_PUBLISHER_HPP

#include <string>

#include <rclcpp/rclcpp.hpp>

#include "diagnostic_updater/diagnostic_updater.hpp"
#include "diagnostic_updater/update_functions.hpp"

namespace diagnostic_updater {

/// Upstream's `HeaderlessTopicDiagnostic`: a topic's publish rate, for a
/// message type with no header. The constructor adds the task to `diag`, so
/// `diag` holds a reference to it — see `DiagnosticTask`.
class HeaderlessTopicDiagnostic : public CompositeDiagnosticTask {
  public:
    HeaderlessTopicDiagnostic(std::string name, Updater& diag, const FrequencyStatusParam& freq,
                              const rclcpp::Clock* clock = nullptr)
        : CompositeDiagnosticTask(name + " topic status"), freq_(freq, clock) {
        addTask(&freq_);
        diag.add(*this);
    }

    ~HeaderlessTopicDiagnostic() override {}

    /// Record one publication.
    virtual void tick() { freq_.tick(); }

    /// Restart the rate window.
    virtual void clear_window() { freq_.clear(); }

  private:
    FrequencyStatus freq_;
};

/// Upstream's `TopicDiagnostic`: a topic's rate and its stamps' delay.
class TopicDiagnostic : public HeaderlessTopicDiagnostic {
  public:
    TopicDiagnostic(std::string name, Updater& diag, const FrequencyStatusParam& freq,
                    const TimeStampStatusParam& stamp, const rclcpp::Clock* clock = nullptr)
        : HeaderlessTopicDiagnostic(name, diag, freq, clock), stamp_(stamp, clock) {
        addTask(&stamp_);
    }

    ~TopicDiagnostic() override {}

    /// Upstream logs FATAL and records nothing: a stamped topic is ticked with
    /// its stamp.
    void tick() override {
        RCLCPP_FATAL(rclcpp::get_logger("TopicDiagnostic_error_logger"), "%s",
                     "tick(void) has been called on a TopicDiagnostic. This is never correct. "
                     "Use tick(rclcpp::Time &) instead.");
    }

    /// Record one publication stamped `stamp`.
    virtual void tick(const rclcpp::Time& stamp) {
        stamp_.tick(stamp);
        HeaderlessTopicDiagnostic::tick();
    }

  private:
    TimeStampStatus stamp_;
};

} // namespace diagnostic_updater

#endif // NROS_DIAGNOSTIC_UPDATER_PUBLISHER_HPP
