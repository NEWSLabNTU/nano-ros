C and C++ entries now bake a contract's operational modes too. Each mode becomes
a variant of every executor's monitor tables, and the entry defines
`NROS_MODE_<NAME>` for each one. Call `nros_cpp_request_mode(NROS_MODE_<NAME>)`
to switch, or `nros_cpp_request_mode(0)` to return to the base contract.
