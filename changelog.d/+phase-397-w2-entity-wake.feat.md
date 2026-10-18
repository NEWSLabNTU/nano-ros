On zenoh and Cyclone DDS, an arrival on an executor's subscription, service
or client now wakes only the executor that owns that entity, and marks the
entity's slot in a ready word the executor will read instead of scanning
(phase-397 W3). Executors sharing one session, such as RTOS tier tasks, are
no longer woken by each other's traffic.
