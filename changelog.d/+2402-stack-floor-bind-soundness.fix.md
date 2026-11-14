The bound stack walk now charges Rust calls through C-supplied pointers, vtable slots copied out of their table, and vtables nested in larger constants; stack floors rise to cover them.
