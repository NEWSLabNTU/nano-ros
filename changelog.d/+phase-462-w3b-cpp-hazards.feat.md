C and C++ images now observe the launch contract's `hazards` on target too, as
Rust images do: the generated entry bakes each hazard its monitors can see and
installs it through the new `nros_cpp_install_hazards`, and the executor
reports `hazard-detected`, `hazard-reaction` and `hazard-recovered`.
