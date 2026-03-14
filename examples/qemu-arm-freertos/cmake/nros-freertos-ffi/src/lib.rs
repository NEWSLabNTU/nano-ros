//! FFI bundle for FreeRTOS C/C++ examples.
//!
//! This thin crate re-exports nros-c and nros-cpp-ffi symbols and provides
//! the panic handler + global allocator needed for a no_std staticlib.

#![no_std]

// Panic handler for bare-metal
use panic_halt as _;

// Force-link so all FFI symbols are available to the C/C++ linker
extern crate nros_c;
extern crate nros_cpp_ffi;
