// SPDX-License-Identifier: Apache-2.0
//
// update_functions.hpp — phase-488.
//
// Upstream's `<diagnostic_updater/update_functions.hpp>` declares the
// statistics tasks `FrequencyStatus`, `TimeStampStatus` and `Heartbeat`, each a
// `DiagnosticTask` subclass. nano-ros's `Updater` takes plain callbacks and has
// no `DiagnosticTask` hierarchy yet, so those classes are not here, and code
// that names one fails to compile naming it.
//
// The header exists because code includes it without using it: Autoware's
// `autoware_external_cmd_selector` (0.45.0) includes it beside
// `<diagnostic_updater/diagnostic_updater.hpp>` and uses only `Updater`. A
// missing header stopped that port at the include line.

#ifndef NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP
#define NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP

#include "diagnostic_updater/diagnostic_updater.hpp"

#endif // NROS_DIAGNOSTIC_UPDATER_UPDATE_FUNCTIONS_HPP
