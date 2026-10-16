A detected hazard now starts its safe state. When the contract's hazard reacts
through a path that states a `safe_state`, the executor calls the reaction hook
(`nros_app_reaction_hook` in a C or C++ entry, `Executor::set_reaction_hook` in
Rust) with that path's reaction id the moment the hazard is detected, once per
detection. Reaction ids are now numbered across the whole model, so one id
names one path wherever it is reported.
