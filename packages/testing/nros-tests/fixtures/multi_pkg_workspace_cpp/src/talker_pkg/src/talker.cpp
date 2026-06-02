// Phase 212.M.10 fixture — minimal Component pkg class. Link-correctness
// only; no nano-ros API surface is exercised here (the test only
// configures + compiles).
namespace talker_pkg {
class Talker {
public:
    Talker() = default;
    int tick() { return 0; }
};
} // namespace talker_pkg

// Force a non-empty TU so the STATIC lib is not elided at link time.
extern "C" int nros_fixture_talker_marker() { return 0; }
