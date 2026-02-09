/**
 * SPDX-License-Identifier: Apache-2.0
 *
 * Mock QDMI device implementation for testing arvak-qdmi.
 *
 * Prefix: MOCK
 * Simulates: 5-qubit linear topology with realistic-ish properties.
 *
 * This implements the QDMI device interface functions that arvak-qdmi's
 * device_loader resolves via dlsym with the "MOCK" prefix.
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

/* -----------------------------------------------------------------------
 * QDMI error codes (must match ffi.rs)
 * ----------------------------------------------------------------------- */

#define QDMI_SUCCESS               0
#define QDMI_ERROR_INVALIDARGUMENT 1
#define QDMI_ERROR_NOTSUPPORTED    2
#define QDMI_ERROR_OUTOFMEMORY     3

/* -----------------------------------------------------------------------
 * QDMI property keys (must match ffi.rs)
 * ----------------------------------------------------------------------- */

/* Device properties */
#define QDMI_DEVICE_PROPERTY_NAME          0
#define QDMI_DEVICE_PROPERTY_VERSION       1
#define QDMI_DEVICE_PROPERTY_LIBRARYVERSION 2
#define QDMI_DEVICE_PROPERTY_QUBITSNUM     3
#define QDMI_DEVICE_PROPERTY_SITES         4
#define QDMI_DEVICE_PROPERTY_COUPLINGMAP   5
#define QDMI_DEVICE_PROPERTY_OPERATIONS    6

/* Site properties */
#define QDMI_SITE_PROPERTY_T1              0
#define QDMI_SITE_PROPERTY_T2              1
#define QDMI_SITE_PROPERTY_READOUTERROR    2
#define QDMI_SITE_PROPERTY_READOUTDURATION 3
#define QDMI_SITE_PROPERTY_FREQUENCY       4

/* Operation properties */
#define QDMI_OPERATION_PROPERTY_NAME       0
#define QDMI_OPERATION_PROPERTY_DURATION   1
#define QDMI_OPERATION_PROPERTY_FIDELITY   2
#define QDMI_OPERATION_PROPERTY_QUBITSNUM  3
#define QDMI_OPERATION_PROPERTY_SITES      4

/* -----------------------------------------------------------------------
 * Mock device data
 * ----------------------------------------------------------------------- */

#define NUM_QUBITS 5
#define NUM_OPERATIONS 3  /* H, CX, RZ */

/* Use integer sentinel values as opaque "site" pointers.
 * We use addresses 0x1000 + i so they're non-null and distinct. */
static const uintptr_t SITES[NUM_QUBITS] = {
    0x1000, 0x1001, 0x1002, 0x1003, 0x1004
};

/* Linear coupling: 0↔1, 1↔2, 2↔3, 3↔4 (8 directed edges) */
static const uintptr_t COUPLING_MAP[] = {
    0x1000, 0x1001,  /* 0 → 1 */
    0x1001, 0x1000,  /* 1 → 0 */
    0x1001, 0x1002,  /* 1 → 2 */
    0x1002, 0x1001,  /* 2 → 1 */
    0x1002, 0x1003,  /* 2 → 3 */
    0x1003, 0x1002,  /* 3 → 2 */
    0x1003, 0x1004,  /* 3 → 4 */
    0x1004, 0x1003,  /* 4 → 3 */
};
#define NUM_COUPLING_PAIRS 8

/* Operations as sentinel pointers */
static const uintptr_t OPERATIONS[NUM_OPERATIONS] = {
    0x2000,  /* H gate */
    0x2001,  /* CX gate */
    0x2002,  /* RZ gate */
};

static const char* OP_NAMES[NUM_OPERATIONS] = { "h", "cx", "rz" };
static const size_t  OP_QUBITS[NUM_OPERATIONS] = { 1, 2, 1 };
static const double  OP_FIDELITIES[NUM_OPERATIONS] = { 0.999, 0.98, 0.9995 };
static const double  OP_DURATIONS[NUM_OPERATIONS] = { 30e-9, 300e-9, 20e-9 };

/* Per-qubit T1, T2 (seconds), readout error, frequency (Hz) */
static const double SITE_T1[NUM_QUBITS] = {
    100e-6, 95e-6, 110e-6, 90e-6, 105e-6
};
static const double SITE_T2[NUM_QUBITS] = {
    50e-6, 48e-6, 55e-6, 45e-6, 52e-6
};
static const double SITE_READOUT_ERR[NUM_QUBITS] = {
    0.02, 0.025, 0.015, 0.03, 0.018
};
static const double SITE_FREQUENCY[NUM_QUBITS] = {
    5.1e9, 5.2e9, 5.05e9, 5.15e9, 5.25e9
};

/* Session is just a dummy non-null pointer. */
typedef void* MOCK_QDMI_Device_Session;
static int session_active = 0;

/* -----------------------------------------------------------------------
 * Helper: find the index of a site/operation by its sentinel value
 * ----------------------------------------------------------------------- */

static int site_index(uintptr_t site) {
    for (int i = 0; i < NUM_QUBITS; i++) {
        if (SITES[i] == site) return i;
    }
    return -1;
}

static int op_index(uintptr_t op) {
    for (int i = 0; i < NUM_OPERATIONS; i++) {
        if (OPERATIONS[i] == op) return i;
    }
    return -1;
}

/* -----------------------------------------------------------------------
 * Helper: write a value into the QDMI two-phase query buffer
 * ----------------------------------------------------------------------- */

static int write_property(
    const void *src, size_t src_size,
    size_t size, void *value, size_t *size_ret
) {
    if (size_ret) *size_ret = src_size;
    if (size == 0 || value == NULL) {
        /* Phase 1: just report the size */
        return QDMI_SUCCESS;
    }
    if (size < src_size) return QDMI_ERROR_INVALIDARGUMENT;
    memcpy(value, src, src_size);
    return QDMI_SUCCESS;
}

/* -----------------------------------------------------------------------
 * Session interface
 * ----------------------------------------------------------------------- */

int MOCK_QDMI_device_session_init(MOCK_QDMI_Device_Session *session_out) {
    if (!session_out) return QDMI_ERROR_INVALIDARGUMENT;
    /* Return a non-null sentinel as the "session handle". */
    *session_out = (void*)0xDEAD;
    session_active = 1;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_session_deinit(MOCK_QDMI_Device_Session session) {
    if (session != (void*)0xDEAD) return QDMI_ERROR_INVALIDARGUMENT;
    session_active = 0;
    return QDMI_SUCCESS;
}

/* -----------------------------------------------------------------------
 * Query interface: device level
 * ----------------------------------------------------------------------- */

int MOCK_QDMI_device_session_query_device_property(
    MOCK_QDMI_Device_Session session,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (session != (void*)0xDEAD) return QDMI_ERROR_INVALIDARGUMENT;

    switch (prop) {
    case QDMI_DEVICE_PROPERTY_NAME: {
        const char *name = "Arvak Mock Device (5Q Linear)";
        /* Include the null terminator */
        return write_property(name, strlen(name) + 1, size, value, size_ret);
    }
    case QDMI_DEVICE_PROPERTY_VERSION: {
        const char *ver = "0.1.0";
        return write_property(ver, strlen(ver) + 1, size, value, size_ret);
    }
    case QDMI_DEVICE_PROPERTY_QUBITSNUM: {
        size_t n = NUM_QUBITS;
        return write_property(&n, sizeof(n), size, value, size_ret);
    }
    case QDMI_DEVICE_PROPERTY_SITES: {
        return write_property(
            SITES, sizeof(SITES), size, value, size_ret
        );
    }
    case QDMI_DEVICE_PROPERTY_COUPLINGMAP: {
        return write_property(
            COUPLING_MAP,
            sizeof(uintptr_t) * NUM_COUPLING_PAIRS * 2,
            size, value, size_ret
        );
    }
    case QDMI_DEVICE_PROPERTY_OPERATIONS: {
        return write_property(
            OPERATIONS, sizeof(OPERATIONS), size, value, size_ret
        );
    }
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

/* -----------------------------------------------------------------------
 * Query interface: site level
 * ----------------------------------------------------------------------- */

int MOCK_QDMI_device_session_query_site_property(
    MOCK_QDMI_Device_Session session,
    void *site,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (session != (void*)0xDEAD) return QDMI_ERROR_INVALIDARGUMENT;

    int idx = site_index((uintptr_t)site);
    if (idx < 0) return QDMI_ERROR_INVALIDARGUMENT;

    switch (prop) {
    case QDMI_SITE_PROPERTY_T1:
        return write_property(&SITE_T1[idx], sizeof(double), size, value, size_ret);
    case QDMI_SITE_PROPERTY_T2:
        return write_property(&SITE_T2[idx], sizeof(double), size, value, size_ret);
    case QDMI_SITE_PROPERTY_READOUTERROR:
        return write_property(&SITE_READOUT_ERR[idx], sizeof(double), size, value, size_ret);
    case QDMI_SITE_PROPERTY_FREQUENCY:
        return write_property(&SITE_FREQUENCY[idx], sizeof(double), size, value, size_ret);
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

/* -----------------------------------------------------------------------
 * Query interface: operation level
 * ----------------------------------------------------------------------- */

int MOCK_QDMI_device_session_query_operation_property(
    MOCK_QDMI_Device_Session session,
    void *operation,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (session != (void*)0xDEAD) return QDMI_ERROR_INVALIDARGUMENT;

    int idx = op_index((uintptr_t)operation);
    if (idx < 0) return QDMI_ERROR_INVALIDARGUMENT;

    switch (prop) {
    case QDMI_OPERATION_PROPERTY_NAME: {
        const char *name = OP_NAMES[idx];
        return write_property(name, strlen(name) + 1, size, value, size_ret);
    }
    case QDMI_OPERATION_PROPERTY_DURATION:
        return write_property(&OP_DURATIONS[idx], sizeof(double), size, value, size_ret);
    case QDMI_OPERATION_PROPERTY_FIDELITY:
        return write_property(&OP_FIDELITIES[idx], sizeof(double), size, value, size_ret);
    case QDMI_OPERATION_PROPERTY_QUBITSNUM:
        return write_property(&OP_QUBITS[idx], sizeof(size_t), size, value, size_ret);
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}
