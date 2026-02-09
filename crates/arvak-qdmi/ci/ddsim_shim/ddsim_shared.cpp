// SPDX-License-Identifier: Apache-2.0
//
// Thin shim to produce a shared library (.so) from the mqt-core DD QDMI
// static device library.
//
// All 18 MQT_DDSIM_QDMI_* symbols are defined with extern "C" linkage in
// the static library (via the generated header mqt_ddsim_qdmi/device.h).
// The --whole-archive linker flag in CMakeLists.txt ensures they end up in
// the dynamic symbol table. This source file provides a backup mechanism:
// explicit references to every symbol, so even linkers that don't fully
// honour --whole-archive on mixed C/C++ static archives will still pull
// them in.
//
// This file contains no logic — it is purely a linker hint.

#include "mqt_ddsim_qdmi/device.h"

// Force the linker to retain all 18 QDMI device symbols from the static
// archive by taking their addresses. The volatile qualifier prevents the
// compiler from optimising these references away.
namespace {

// Device lifecycle (2)
[[maybe_unused]] volatile auto* ref_01 = &MQT_DDSIM_QDMI_device_initialize;
[[maybe_unused]] volatile auto* ref_02 = &MQT_DDSIM_QDMI_device_finalize;

// Session lifecycle (4)
[[maybe_unused]] volatile auto* ref_03 = &MQT_DDSIM_QDMI_device_session_alloc;
[[maybe_unused]] volatile auto* ref_04 = &MQT_DDSIM_QDMI_device_session_set_parameter;
[[maybe_unused]] volatile auto* ref_05 = &MQT_DDSIM_QDMI_device_session_init;
[[maybe_unused]] volatile auto* ref_06 = &MQT_DDSIM_QDMI_device_session_free;

// Query interface (3)
[[maybe_unused]] volatile auto* ref_07 =
    &MQT_DDSIM_QDMI_device_session_query_device_property;
[[maybe_unused]] volatile auto* ref_08 =
    &MQT_DDSIM_QDMI_device_session_query_site_property;
[[maybe_unused]] volatile auto* ref_09 =
    &MQT_DDSIM_QDMI_device_session_query_operation_property;

// Job interface (9)
[[maybe_unused]] volatile auto* ref_10 =
    &MQT_DDSIM_QDMI_device_session_create_device_job;
[[maybe_unused]] volatile auto* ref_11 = &MQT_DDSIM_QDMI_device_job_set_parameter;
[[maybe_unused]] volatile auto* ref_12 = &MQT_DDSIM_QDMI_device_job_query_property;
[[maybe_unused]] volatile auto* ref_13 = &MQT_DDSIM_QDMI_device_job_submit;
[[maybe_unused]] volatile auto* ref_14 = &MQT_DDSIM_QDMI_device_job_cancel;
[[maybe_unused]] volatile auto* ref_15 = &MQT_DDSIM_QDMI_device_job_check;
[[maybe_unused]] volatile auto* ref_16 = &MQT_DDSIM_QDMI_device_job_wait;
[[maybe_unused]] volatile auto* ref_17 = &MQT_DDSIM_QDMI_device_job_get_results;
[[maybe_unused]] volatile auto* ref_18 = &MQT_DDSIM_QDMI_device_job_free;

} // namespace
