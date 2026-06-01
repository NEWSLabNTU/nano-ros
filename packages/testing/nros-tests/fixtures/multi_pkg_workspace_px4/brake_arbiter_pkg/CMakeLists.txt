# Phase 212.H.7 fixture — brake_arbiter component pkg.
cmake_minimum_required(VERSION 3.16)
project(brake_arbiter_pkg CXX)
add_library(brake_arbiter_pkg STATIC src/brake_arbiter.cpp)
target_include_directories(brake_arbiter_pkg PUBLIC src)
