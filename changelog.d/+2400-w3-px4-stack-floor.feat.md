A PX4 module's threads are now checked against the stack their own code needs.

- **When the archive is built.** `scripts/build/cargo-out-dir-headers.py --stack-floor px4-uorb` walks the nano-ros archive it builds. It writes the stack floor of the session open and of one executor pass into the placed C++ config header.
- **At configure.** `nros_px4_add_module` checks the module's `STACK_MAIN` and each work queue's `CONFIG_WQ_<Q>_STACKSIZE` against those floors. A measured floor is refused; an unmeasured one is reported as a warning.
- **The fix it names.** A refusal now names the configured value to write, already adjusted for how PX4 scales stacks on POSIX.
- **When it re-runs.** The configure runs again when a newly built archive moves a floor.
