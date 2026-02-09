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
// archive by casting their addresses into a volatile void* array. The
// volatile qualifier prevents the compiler from optimising these away.
// We use void* casts because these are extern "C" function symbols and
// auto* cannot deduce from C function declarations.
namespace {

[[maybe_unused]] volatile void* refs[] = {
    // Device lifecycle (2)
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_initialize),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_finalize),

    // Session lifecycle (4)
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_session_alloc),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_session_set_parameter),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_session_init),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_session_free),

    // Query interface (3)
    reinterpret_cast<void*>(
        &MQT_DDSIM_QDMI_device_session_query_device_property),
    reinterpret_cast<void*>(
        &MQT_DDSIM_QDMI_device_session_query_site_property),
    reinterpret_cast<void*>(
        &MQT_DDSIM_QDMI_device_session_query_operation_property),

    // Job interface (9)
    reinterpret_cast<void*>(
        &MQT_DDSIM_QDMI_device_session_create_device_job),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_set_parameter),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_query_property),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_submit),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_cancel),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_check),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_wait),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_get_results),
    reinterpret_cast<void*>(&MQT_DDSIM_QDMI_device_job_free),
};

} // namespace
