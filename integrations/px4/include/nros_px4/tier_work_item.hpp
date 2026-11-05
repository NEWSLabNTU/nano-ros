/****************************************************************************
 * Deprecated forwarding header: renamed <nros_px4/sched_task_work_item.hpp>.
 *
 * Phase 536 W2 (nano-ros RFC-0052 "Names"): a scheduling tier is a sched
 * task, so `TierWorkItem` is `SchedTaskWorkItem`. This header and the alias
 * stay one release so an entry generated before the rename still builds,
 * with a warning naming the new spelling; define
 * `NROS_NO_DEPRECATED_TIER_NAMES` to drop the alias early.
 ****************************************************************************/

#pragma once

#if defined(__GNUC__) || defined(__clang__)
#warning "<nros_px4/tier_work_item.hpp> is renamed <nros_px4/sched_task_work_item.hpp> (phase 536)"
#endif

#include <nros_px4/sched_task_work_item.hpp>

#ifndef NROS_NO_DEPRECATED_TIER_NAMES
namespace nros_px4
{
using TierWorkItem [[deprecated("renamed SchedTaskWorkItem (phase 536)")]] = SchedTaskWorkItem;
} // namespace nros_px4
#endif
