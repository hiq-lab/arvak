# SPDX-License-Identifier: Apache-2.0
#
# Fetch mqt-core and delegate the DD QDMI device shared-library build to
# upstream.
#
# Since mqt-core v3.6.0 the DD QDMI device target ships as a shared library
# directly (`add_mqt_core_library(... FORCE_SHARED HIDDEN_VISIBILITY ...)`),
# with the 18 `MQT_DDSIM_QDMI_device_*` symbols exported through the
# QDMI_EXPORT macro from qdmi/export.h. The previous shim wrapped a static
# archive with `--whole-archive` — that machinery is obsolete now.
#
# Output:  libmqt-core-qdmi-ddsim-device.so  (built by upstream)
# Target:  mqt-core-qdmi-ddsim-device
#
# Usage:
#   cmake -S crates/arvak-qdmi/ci/ddsim_shim -B build-ddsim -G Ninja \
#         -DCMAKE_BUILD_TYPE=Release
#   cmake --build build-ddsim --target mqt-core-qdmi-ddsim-device
#
# To build against mqt-core main instead of the pinned tag:
#   cmake -S ... -B ... -DMQT_CORE_VERSION=main

cmake_minimum_required(VERSION 3.24)
project(arvak-ddsim-fetch LANGUAGES C CXX)

# Which mqt-core version to build against. Nightly CI overrides this via
# workflow input; local builds default to the pinned release tag.
set(MQT_CORE_VERSION "v3.7.0" CACHE STRING "mqt-core git tag or branch")

message(STATUS "Fetching mqt-core ${MQT_CORE_VERSION}")

include(FetchContent)

# Disable parts of mqt-core we don't need (saves significant build time).
set(BUILD_MQT_CORE_TESTS      OFF CACHE BOOL "" FORCE)
set(BUILD_MQT_CORE_BINDINGS   OFF CACHE BOOL "" FORCE)
set(BUILD_MQT_CORE_BENCHMARKS OFF CACHE BOOL "" FORCE)

# mqt-core v3.7.0 self-fetches nlohmann_json, spdlog, QDMI (v1.3.2), and Boost.
# No manual pre-fetching needed.
FetchContent_Declare(
  mqt-core
  GIT_REPOSITORY https://github.com/munich-quantum-toolkit/core.git
  GIT_TAG        ${MQT_CORE_VERSION}
)
FetchContent_MakeAvailable(mqt-core)
