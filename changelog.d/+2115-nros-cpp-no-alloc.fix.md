nros-cpp builds without `alloc` again, with the parameter store carved from
the executor backing (`param-store`, `NROS_PARAM_STORE=1`). In that build
`-p` / `--params-file` arguments are refused by name and scheduler levels poll,
because both need a heap. The Cortex-M staticlib references no allocator
symbol (issue 2115).
