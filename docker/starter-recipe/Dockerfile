# A team's image on top of the nano-ros starter -- RFC-0127 D5, the recipe
# option B rules. Copy this file and change the two marked lines.
#
# What the starter guarantees a recipe like this one (RFC-0127 D5):
#   1. a version tag is never re-pushed, so `FROM` a version is reproducible;
#   2. the baked store is what that release's `nros setup` resolves, so
#      `nros setup` here only ADDS what this recipe asks for;
#   3. NROS_HOME (/opt/nros) and the working directory /work do not move
#      within a major version;
#   4. this recipe is built against every release by a probe
#      (scripts/probe/verify-starter-recipe.sh).
#
# Until the starter is published (RFC-0127 Q1), STARTER names a locally
# built tag: scripts/ci/starter-image.sh --tag nano-ros-starter:local
ARG STARTER=nano-ros-starter:local
FROM ${STARTER}

# (1) A board the starter does not carry. `nros setup` reuses everything the
#     starter already baked and fetches only what this board adds.
ARG TEAM_BOARD=mps3-an536-freertos
RUN nros setup "${TEAM_BOARD}" --rmw zenoh

# (2) A package the team's nodes need that the starter does not ship, from
#     the image's own ROS 2 (Humble).
#     The starter runs as the user `nros`; installing a system package is
#     the one step that needs root, and the image goes back to `nros` after.
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends ros-humble-nav-msgs \
    && rm -rf /var/lib/apt/lists/*
USER nros
