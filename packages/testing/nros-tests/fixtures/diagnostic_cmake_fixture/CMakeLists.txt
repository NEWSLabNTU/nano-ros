# Phase 212 §Acceptance — deliberate CMake find_package failure.
#
# The "Could not find a package configuration file provided by ..."
# diagnostic emitted here MUST reach the user's terminal verbatim per
# the §Non-Goals contract. `phase212_diagnostic_verbatim::
# cmake_diagnostic_verbatim` invokes `cmake -B build` against this
# fixture and greps stderr for the exact error prefix.
cmake_minimum_required(VERSION 3.20)
project(diagnostic_cmake_fixture LANGUAGES NONE)
find_package(NoSuchPackageForPhase212VerbatimTest REQUIRED)
