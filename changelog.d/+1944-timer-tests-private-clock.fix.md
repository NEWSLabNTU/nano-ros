nros-node's timer unit tests no longer fire a timer one step early on a loaded
host. The shared test helper that SLEPT each step on the wall clock is gone;
every test that steps time runs on its own private clock, so a "50 ms" step is
exactly 50 ms and the timer assertions are equalities (issue 1944).
