Setting a parameter no longer copies its value through every frame of the set
path. `ParameterServer::apply_ref` / `set_ref` and
`Executor::set_parameter_on_ref` borrow it down to its slot, the C++ setters
lend their one value, a range attach no longer clones the stored value, and
the argv-override branch of a declaration no longer clones the declared one.
Listing overrides (`for_each_argv_param_override_on`) hands each typed value
to the callback by reference (issue 2104).
