A package that is built as an image (its registration names a `DEPLOY`) and
registers a second component now fails at configure, naming both components.
Previously the second was built and silently left out of the firmware, which
shipped with the first component only. Put each component in its own package.
