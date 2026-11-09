`nros run` no longer returns while a runner's process chain (`west`, `fvp`,
`renode`, `command`) is still stopping. The runner names the chain's process
group on stderr. After a stop, the verb waits up to 2.5 s for that group to
empty, then SIGKILLs whatever is left and says so (issue 2307).
