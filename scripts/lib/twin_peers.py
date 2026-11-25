"""Host services a stack-floor twin run needs besides the router (issue 2963).

A row in `config/stack-floors.toml` names them in `twin_peers`, one
`<kind>=<port>` each, for example `["sntp=12399", "dns=15353"]`. The twin lane
(`scripts/stack-floor-twin.py`) starts them on 127.0.0.1 for the length of
the run. A QEMU guest reaches them as 10.0.2.2, the user-mode network's
gateway; that is the address an image's knobs and DNS settings name.

- `sntp`: answers every SNTP request with the host's time, the reply
  `SntpServer` in `packages/testing/nros-tests/tests/zephyr_clock_resolve.rs`
  gives (mode 4, stratum 1, originate = the request's transmit). That is what
  Zephyr's `sntp_simple` checks.
- `dns`: answers every A query with 10.0.2.2 and every other type with an
  empty NOERROR, so an image that names its server by hostname runs Zephyr's
  DNS resolver and reaches the `sntp` peer.

Each peer counts what it answered, so a run can show it reached the path it
was meant to measure.
"""

from __future__ import annotations

import socket
import struct
import threading
import time

KINDS = ("sntp", "dns")

# NTP's epoch (1900) to the Unix epoch (1970), in seconds.
NTP_UNIX_DELTA_S = 2_208_988_800
# The guest's route to the host on QEMU's user-mode network.
GUEST_HOST_ADDR = bytes([10, 0, 2, 2])


def parse(specs) -> list[tuple[str, int]]:
    """`[(kind, port)]` from a row's `twin_peers`, or ValueError naming the bad
    entry."""
    if specs is None:
        return []
    if not isinstance(specs, list):
        raise ValueError("`twin_peers` must be a list of `<kind>=<port>` strings")
    out = []
    for s in specs:
        kind, _, port = str(s).partition("=")
        if kind not in KINDS:
            raise ValueError(f"`twin_peers` entry `{s}`: kind must be one of {KINDS}")
        if not port.isdigit() or not 0 < int(port) < 65536:
            raise ValueError(f"`twin_peers` entry `{s}`: `{port}` is not a port")
        out.append((kind, int(port)))
    return out


def sntp_reply(req: bytes, now: float) -> bytes | None:
    """The reply to one SNTP request at host time `now`, or None for a request
    too short to be one."""
    if len(req) < 48:
        return None
    secs = int(now) + NTP_UNIX_DELTA_S
    frac = int((now - int(now)) * (1 << 32)) & 0xFFFFFFFF
    stamp = struct.pack(">II", secs & 0xFFFFFFFF, frac)
    rep = bytearray(48)
    rep[0], rep[1], rep[2], rep[3] = 0x24, 1, 4, 0xEC  # LI 0, v4, mode 4; stratum 1
    rep[12:16] = b"LOCL"
    rep[16:24] = stamp  # reference
    rep[24:32] = req[40:48]  # originate = their transmit
    rep[32:40] = stamp  # receive
    rep[40:48] = stamp  # transmit
    return bytes(rep)


def dns_reply(q: bytes) -> bytes | None:
    """The reply to one DNS query, or None for one too short or malformed."""
    if len(q) < 12:
        return None
    tid = struct.unpack(">H", q[:2])[0]
    i = 12
    while i < len(q) and q[i] != 0:
        i += q[i] + 1
    end = i + 5  # the name's terminating 0, then QTYPE and QCLASS
    if end > len(q):
        return None
    qtype = struct.unpack(">H", q[i + 1:i + 3])[0]
    question = q[12:end]
    if qtype == 1:
        answer = b"\xc0\x0c" + struct.pack(">HHIH", 1, 1, 60, 4) + GUEST_HOST_ADDR
        an = 1
    else:
        answer, an = b"", 0
    return struct.pack(">HHHHHH", tid, 0x8180, 1, an, 0, 0) + question + answer


class Peers:
    """The row's peers, started on enter and stopped on exit. `answered` maps
    each kind to how many requests it answered."""

    def __init__(self, specs):
        self.specs = parse(specs)
        self.answered = {k: 0 for k, _ in self.specs}
        self._stop = threading.Event()
        self._threads: list[threading.Thread] = []
        self._socks: list[socket.socket] = []

    def __enter__(self) -> "Peers":
        for kind, port in self.specs:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.bind(("127.0.0.1", port))
            s.settimeout(0.1)
            self._socks.append(s)
            t = threading.Thread(target=self._serve, args=(kind, s), daemon=True)
            t.start()
            self._threads.append(t)
        return self

    def _serve(self, kind: str, s: socket.socket) -> None:
        while not self._stop.is_set():
            try:
                req, peer = s.recvfrom(512)
            except (socket.timeout, OSError):
                continue
            rep = sntp_reply(req, time.time()) if kind == "sntp" else dns_reply(req)
            if rep is not None:
                s.sendto(rep, peer)
                self.answered[kind] += 1

    def __exit__(self, *_exc) -> None:
        self._stop.set()
        for t in self._threads:
            t.join(timeout=2)
        for s in self._socks:
            s.close()


def self_test() -> list[str]:
    """Negative controls for the parser and both replies; returns failures."""
    bad = []
    try:
        parse(["ntp=1"])
        bad.append("parse accepted an unknown kind")
    except ValueError:
        pass
    try:
        parse(["sntp=0"])
        bad.append("parse accepted port 0")
    except ValueError:
        pass
    if parse(["sntp=12399", "dns=15353"]) != [("sntp", 12399), ("dns", 15353)]:
        bad.append("parse misread a valid list")
    req = bytes(40) + b"ORIGINAL"
    rep = sntp_reply(req, 1_700_000_000.5)
    if rep is None or rep[0] != 0x24 or rep[24:32] != b"ORIGINAL":
        bad.append("the SNTP reply is not mode 4 or does not echo the transmit stamp")
    if sntp_reply(bytes(47), 0.0) is not None:
        bad.append("a 47-byte SNTP request was answered")
    q = struct.pack(">HHHHHH", 0x1234, 0x0100, 1, 0, 0, 0) + b"\x03ntp\x04test\x00" + \
        struct.pack(">HH", 1, 1)
    a = dns_reply(q)
    if a is None or a[:2] != b"\x12\x34" or not a.endswith(GUEST_HOST_ADDR):
        bad.append("the DNS A reply does not answer 10.0.2.2 under the query's id")
    aaaa = dns_reply(q[:-4] + struct.pack(">HH", 28, 1))
    if aaaa is None or struct.unpack(">H", aaaa[6:8])[0] != 0:
        bad.append("a DNS AAAA query got an answer record")
    if dns_reply(q[:14]) is not None:
        bad.append("a truncated DNS query was answered")
    return bad
