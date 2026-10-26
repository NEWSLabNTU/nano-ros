`wait_for_service` called from inside a callback no longer starts a second executor pass; it polls the graph without spinning, so a multi-threaded backend still sees the server arrive (issue 2055).
