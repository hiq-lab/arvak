# SPDX-License-Identifier: Apache-2.0
#
# Build a shared library (.so) wrapping the MQT Core DDSIM QDMI device.
#
# The upstream mqt-core builds the DD QDMI device as a static library only.
# This shim links that static library into a shared library so arvak-qdmi
# can dlopen() it at runtime, just like any other QDMI device.
#
# Usage:
#   cmake -S crates/arvak-qdmi/ci/ddsim_shim -B build-ddsim \
#         -G Ninja -DCMAKE_BUILD_TYPE=Release
#   cmake --build build-ddsim --target mqt_ddsim_qdmi_shared
#
# To build against mqt-core main (instead of pinned tag):
#   cmake -S ... -B ... -DMQT_CORE_VERSION=main

cmake_minimum_required(VERSION 3.24)
project(arvak-ddsim-shim LANGUAGES C CXX)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Which mqt-core version to build against. Default: pinned release tag.
# CI overrides this to "main" for the upstream-drift detection job.
set(MQT_CORE_VERSION "v3.4.1" CACHE STRING "mqt-core git tag or branch")

message(STATUS "Building DDSIM shim against mqt-core ${MQT_CORE_VERSION}")

# All code (including fetched dependencies) must be compiled with -fPIC
# because the static libraries end up linked into a shared library.
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

include(FetchContent)

# ---------------------------------------------------------------------------
# Pre-fetch QDMI so generate_prefixed_qdmi_headers() is defined before
# mqt-core's DD device CMakeLists needs it.  (Required since mqt-core v3.4.1
# which reorganised the QDMI device builds to use prefixed headers.)
# ---------------------------------------------------------------------------

set(QDMI_INSTALL OFF CACHE BOOL "" FORCE)
FetchContent_Declare(
  qdmi
  GIT_REPOSITORY https://github.com/Munich-Quantum-Software-Stack/qdmi.git
  GIT_TAG        70b815615475598c6194096a29c1b2340dd54a6c   # QDMI 1.2.1
)
FetchContent_MakeAvailable(qdmi)

# ---------------------------------------------------------------------------
# Pre-fetch nlohmann_json and spdlog (required by mqt-core >= v3.4.1)
# ---------------------------------------------------------------------------

FetchContent_Declare(
  nlohmann_json
  GIT_REPOSITORY https://github.com/nlohmann/json.git
  GIT_TAG        v3.11.3
)
set(JSON_BuildTests OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(nlohmann_json)

FetchContent_Declare(
  spdlog
  GIT_REPOSITORY https://github.com/gabime/spdlog.git
  GIT_TAG        v1.15.1
)
set(SPDLOG_BUILD_EXAMPLE OFF CACHE BOOL "" FORCE)
set(SPDLOG_BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(spdlog)

# ---------------------------------------------------------------------------
# Fetch mqt-core (reuses already-populated qdmi, nlohmann_json, spdlog)
# ---------------------------------------------------------------------------

FetchContent_Declare(
  mqt-core
  GIT_REPOSITORY https://github.com/cda-tum/mqt-core.git
  GIT_TAG        ${MQT_CORE_VERSION}
)

# Disable parts of mqt-core we don't need (saves significant build time)
set(BUILD_MQT_CORE_TESTS    OFF CACHE BOOL "" FORCE)
set(BUILD_MQT_CORE_BINDINGS OFF CACHE BOOL "" FORCE)
set(BUILD_MQT_CORE_BENCHMARKS OFF CACHE BOOL "" FORCE)

FetchContent_MakeAvailable(mqt-core)

# ---------------------------------------------------------------------------
# Create the shared library
# ---------------------------------------------------------------------------

add_library(mqt_ddsim_qdmi_shared SHARED ddsim_shared.cpp)

# The DD device static library target created by mqt-core
# Target name: mqt-core-qdmi-ddsim-device
# CMake alias:  MQT::CoreQDMI_DDSIM_Device (via add_mqt_core_library)
target_link_libraries(mqt_ddsim_qdmi_shared
  PRIVATE
    mqt-core-qdmi-ddsim-device
)

# Force all symbols from the static library into the shared library.
# Without --whole-archive the linker would discard "unreferenced" symbols.
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
  target_link_options(mqt_ddsim_qdmi_shared PRIVATE
    "LINKER:--whole-archive"
    "$<TARGET_FILE:mqt-core-qdmi-ddsim-device>"
    "LINKER:--no-whole-archive"
  )
endif()

# Ensure symbols are visible in the dynamic symbol table
set_target_properties(mqt_ddsim_qdmi_shared PROPERTIES
  OUTPUT_NAME                "mqt_ddsim_qdmi_device"
  CXX_VISIBILITY_PRESET      default
  C_VISIBILITY_PRESET        default
  VISIBILITY_INLINES_HIDDEN  OFF
  POSITION_INDEPENDENT_CODE  ON
)

target_compile_features(mqt_ddsim_qdmi_shared PRIVATE cxx_std_20)
