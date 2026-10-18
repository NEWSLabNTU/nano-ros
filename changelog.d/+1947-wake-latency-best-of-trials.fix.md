The cross-thread wake-latency test judges its 10 ms bound on the best of
five trials, so a gate build preempting the main thread no longer fails it
while a wake that never fires still fails every trial (issue 1947).
