# phase-575 W1 -- the C and C++ half of `check-r5f-hard-float-abi`: built with
# cmake/toolchain/arm-freertos-armcr5f.cmake, so the objects carry exactly the
# flags that toolchain file gives an image.
cmake_minimum_required(VERSION 3.22)
project(r5f_hard_float_abi C CXX)
add_library(r5f_abi_c STATIC abi.c abi.cpp)
