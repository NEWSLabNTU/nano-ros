A Zephyr image configure that finds its metadata-probe sidecars stale now still
lists them as configure inputs. Before, it listed none, so the `nros configure`
that made them current never re-ran the image's configure, and the image stayed
sized from the closure fallback (measured on the safety island as a DTCM
overflow) until a fresh configure was forced.
