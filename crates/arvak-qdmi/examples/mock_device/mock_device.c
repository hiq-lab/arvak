/**
 * SPDX-License-Identifier: Apache-2.0
 *
 * Mock QDMI v1.2.1 device implementation for testing arvak-qdmi.
 *
 * Prefix: MOCK
 * Simulates: 5-qubit linear topology with realistic properties.
 *
 * Implements all 18 device interface functions per the QDMI v1.2.1 spec:
 *   - 2 device lifecycle (initialize, finalize)
 *   - 4 session lifecycle (alloc, set_parameter, init, free)
 *   - 3 query interface (device, site, operation)
 *   - 9 job interface (create, set_parameter, query_property, submit,
 *                       cancel, check, wait, get_results, free)
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

/* -----------------------------------------------------------------------
 * QDMI v1.2.1 status codes (must match ffi.rs)
 * ----------------------------------------------------------------------- */

#define QDMI_SUCCESS                0
#define QDMI_WARN_GENERAL           1
#define QDMI_ERROR_FATAL           -1
#define QDMI_ERROR_OUTOFMEM        -2
#define QDMI_ERROR_NOTIMPLEMENTED  -3
#define QDMI_ERROR_LIBNOTFOUND     -4
#define QDMI_ERROR_NOTFOUND        -5
#define QDMI_ERROR_OUTOFRANGE      -6
#define QDMI_ERROR_INVALIDARGUMENT -7
#define QDMI_ERROR_PERMISSIONDENIED -8
#define QDMI_ERROR_NOTSUPPORTED    -9
#define QDMI_ERROR_BADSTATE       -10
#define QDMI_ERROR_TIMEOUT        -11

/* -----------------------------------------------------------------------
 * QDMI v1.2.1 property keys (must match ffi.rs)
 * ----------------------------------------------------------------------- */

/* Device properties */
#define QDMI_DEVICE_PROPERTY_NAME                   0
#define QDMI_DEVICE_PROPERTY_VERSION                1
#define QDMI_DEVICE_PROPERTY_STATUS                 2
#define QDMI_DEVICE_PROPERTY_LIBRARYVERSION         3
#define QDMI_DEVICE_PROPERTY_QUBITSNUM              4
#define QDMI_DEVICE_PROPERTY_SITES                  5
#define QDMI_DEVICE_PROPERTY_OPERATIONS             6
#define QDMI_DEVICE_PROPERTY_COUPLINGMAP            7
#define QDMI_DEVICE_PROPERTY_DURATIONUNIT          12
#define QDMI_DEVICE_PROPERTY_DURATIONSCALEFACTOR   13
#define QDMI_DEVICE_PROPERTY_SUPPORTEDPROGRAMFORMATS 15

/* Site properties */
#define QDMI_SITE_PROPERTY_INDEX  0
#define QDMI_SITE_PROPERTY_T1     1
#define QDMI_SITE_PROPERTY_T2     2
#define QDMI_SITE_PROPERTY_NAME   3

/* Operation properties */
#define QDMI_OPERATION_PROPERTY_NAME          0
#define QDMI_OPERATION_PROPERTY_QUBITSNUM     1
#define QDMI_OPERATION_PROPERTY_PARAMETERSNUM 2
#define QDMI_OPERATION_PROPERTY_DURATION      3
#define QDMI_OPERATION_PROPERTY_FIDELITY      4

/* Device status */
#define QDMI_DEVICE_STATUS_IDLE  1

/* Job status */
#define QDMI_JOB_STATUS_CREATED   0
#define QDMI_JOB_STATUS_SUBMITTED 1
#define QDMI_JOB_STATUS_DONE      4

/* Program formats */
#define QDMI_PROGRAM_FORMAT_QASM2  0
#define QDMI_PROGRAM_FORMAT_QASM3  1

/* Device session parameters */
#define QDMI_DEVICE_SESSION_PARAMETER_BASEURL  0
#define QDMI_DEVICE_SESSION_PARAMETER_TOKEN    1

/* Device job parameters */
#define QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT 0
#define QDMI_DEVICE_JOB_PARAMETER_PROGRAM       1
#define QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM      2

/* Device job properties */
#define QDMI_DEVICE_JOB_PROPERTY_ID             0

/* Job result types */
#define QDMI_JOB_RESULT_HISTKEYS   1
#define QDMI_JOB_RESULT_HISTVALUES 2

/* -----------------------------------------------------------------------
 * Mock device data
 * ----------------------------------------------------------------------- */

#define NUM_QUBITS 5
#define NUM_OPERATIONS 3  /* H, CX, RZ */

/* Sentinel values as opaque "site" pointers. */
static const uintptr_t SITES[NUM_QUBITS] = {
    0x1000, 0x1001, 0x1002, 0x1003, 0x1004
};

/* Linear coupling: 0-1, 1-2, 2-3, 3-4 (8 directed edges) */
static const uintptr_t COUPLING_MAP[] = {
    0x1000, 0x1001,  /* 0 -> 1 */
    0x1001, 0x1000,  /* 1 -> 0 */
    0x1001, 0x1002,  /* 1 -> 2 */
    0x1002, 0x1001,  /* 2 -> 1 */
    0x1002, 0x1003,  /* 2 -> 3 */
    0x1003, 0x1002,  /* 3 -> 2 */
    0x1003, 0x1004,  /* 3 -> 4 */
    0x1004, 0x1003,  /* 4 -> 3 */
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
static const size_t  OP_PARAMS[NUM_OPERATIONS] = { 0, 0, 1 };
static const double  OP_FIDELITIES[NUM_OPERATIONS] = { 0.999, 0.98, 0.9995 };
/* Durations as uint64_t (nanoseconds); scale factor = 1e-9 makes them seconds */
static const uint64_t OP_DURATIONS[NUM_OPERATIONS] = { 30, 300, 20 };

/* Per-qubit T1, T2 as uint64_t (nanoseconds); scale factor = 1e-9 */
static const uint64_t SITE_T1[NUM_QUBITS] = {
    100000, 95000, 110000, 90000, 105000
};
static const uint64_t SITE_T2[NUM_QUBITS] = {
    50000, 48000, 55000, 45000, 52000
};

/* Supported formats */
static const int32_t SUPPORTED_FORMATS[] = {
    QDMI_PROGRAM_FORMAT_QASM2,
    QDMI_PROGRAM_FORMAT_QASM3,
};

/* Duration scale factor: 1e-9 (raw values in nanoseconds) */
static const double DURATION_SCALE_FACTOR = 1e-9;

/* Device initialization reference count (supports concurrent test loads) */
static int device_init_refcount = 0;

/* -----------------------------------------------------------------------
 * Session struct
 * ----------------------------------------------------------------------- */

typedef struct {
    int active;
    char token[256];
    char baseurl[256];
} MockSession;

/* -----------------------------------------------------------------------
 * Job struct
 * ----------------------------------------------------------------------- */

typedef struct {
    int status;
    int program_format;
    char* program;
    size_t shots;
} MockJob;

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

/* =======================================================================
 * Device lifecycle (2 functions)
 * ======================================================================= */

int MOCK_QDMI_device_initialize(void) {
    device_init_refcount++;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_finalize(void) {
    if (device_init_refcount > 0) device_init_refcount--;
    return QDMI_SUCCESS;
}

/* =======================================================================
 * Session lifecycle (4 functions)
 * ======================================================================= */

int MOCK_QDMI_device_session_alloc(void **session_out) {
    if (!session_out) return QDMI_ERROR_INVALIDARGUMENT;
    if (device_init_refcount <= 0) return QDMI_ERROR_BADSTATE;

    MockSession *s = (MockSession*)calloc(1, sizeof(MockSession));
    if (!s) return QDMI_ERROR_OUTOFMEM;

    *session_out = s;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_session_set_parameter(
    void *session, int param, size_t size, const void *value
) {
    if (!session) return QDMI_ERROR_INVALIDARGUMENT;
    MockSession *s = (MockSession*)session;

    switch (param) {
    case QDMI_DEVICE_SESSION_PARAMETER_TOKEN:
        if (value && size > 0 && size < sizeof(s->token)) {
            memcpy(s->token, value, size);
            s->token[size] = '\0';
        }
        return QDMI_SUCCESS;
    case QDMI_DEVICE_SESSION_PARAMETER_BASEURL:
        if (value && size > 0 && size < sizeof(s->baseurl)) {
            memcpy(s->baseurl, value, size);
            s->baseurl[size] = '\0';
        }
        return QDMI_SUCCESS;
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

int MOCK_QDMI_device_session_init(void *session) {
    if (!session) return QDMI_ERROR_INVALIDARGUMENT;
    MockSession *s = (MockSession*)session;
    s->active = 1;
    return QDMI_SUCCESS;
}

void MOCK_QDMI_device_session_free(void *session) {
    if (session) {
        free(session);
    }
}

/* =======================================================================
 * Query interface: device level
 * ======================================================================= */

int MOCK_QDMI_device_session_query_device_property(
    void *session,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (!session) return QDMI_ERROR_INVALIDARGUMENT;
    MockSession *s = (MockSession*)session;
    if (!s->active) return QDMI_ERROR_BADSTATE;

    switch (prop) {
    case QDMI_DEVICE_PROPERTY_NAME: {
        const char *name = "Arvak Mock Device (5Q Linear)";
        return write_property(name, strlen(name) + 1, size, value, size_ret);
    }
    case QDMI_DEVICE_PROPERTY_VERSION: {
        const char *ver = "0.1.0";
        return write_property(ver, strlen(ver) + 1, size, value, size_ret);
    }
    case QDMI_DEVICE_PROPERTY_STATUS: {
        int32_t status = QDMI_DEVICE_STATUS_IDLE;
        return write_property(&status, sizeof(status), size, value, size_ret);
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
    case QDMI_DEVICE_PROPERTY_DURATIONSCALEFACTOR: {
        return write_property(
            &DURATION_SCALE_FACTOR, sizeof(double), size, value, size_ret
        );
    }
    case QDMI_DEVICE_PROPERTY_SUPPORTEDPROGRAMFORMATS: {
        return write_property(
            SUPPORTED_FORMATS, sizeof(SUPPORTED_FORMATS), size, value, size_ret
        );
    }
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

/* =======================================================================
 * Query interface: site level
 * ======================================================================= */

int MOCK_QDMI_device_session_query_site_property(
    void *session,
    void *site,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (!session) return QDMI_ERROR_INVALIDARGUMENT;

    int idx = site_index((uintptr_t)site);
    if (idx < 0) return QDMI_ERROR_INVALIDARGUMENT;

    switch (prop) {
    case QDMI_SITE_PROPERTY_INDEX: {
        size_t index = (size_t)idx;
        return write_property(&index, sizeof(size_t), size, value, size_ret);
    }
    case QDMI_SITE_PROPERTY_T1:
        return write_property(&SITE_T1[idx], sizeof(uint64_t), size, value, size_ret);
    case QDMI_SITE_PROPERTY_T2:
        return write_property(&SITE_T2[idx], sizeof(uint64_t), size, value, size_ret);
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

/* =======================================================================
 * Query interface: operation level
 * ======================================================================= */

int MOCK_QDMI_device_session_query_operation_property(
    void *session,
    void *operation,
    size_t num_sites,
    const void *sites,
    size_t num_params,
    const double *params,
    int prop,
    size_t size,
    void *value,
    size_t *size_ret
) {
    if (!session) return QDMI_ERROR_INVALIDARGUMENT;

    /* Suppress unused parameter warnings */
    (void)num_sites;
    (void)sites;
    (void)num_params;
    (void)params;

    int idx = op_index((uintptr_t)operation);
    if (idx < 0) return QDMI_ERROR_INVALIDARGUMENT;

    switch (prop) {
    case QDMI_OPERATION_PROPERTY_NAME: {
        const char *name = OP_NAMES[idx];
        return write_property(name, strlen(name) + 1, size, value, size_ret);
    }
    case QDMI_OPERATION_PROPERTY_QUBITSNUM:
        return write_property(&OP_QUBITS[idx], sizeof(size_t), size, value, size_ret);
    case QDMI_OPERATION_PROPERTY_PARAMETERSNUM:
        return write_property(&OP_PARAMS[idx], sizeof(size_t), size, value, size_ret);
    case QDMI_OPERATION_PROPERTY_DURATION:
        return write_property(&OP_DURATIONS[idx], sizeof(uint64_t), size, value, size_ret);
    case QDMI_OPERATION_PROPERTY_FIDELITY:
        return write_property(&OP_FIDELITIES[idx], sizeof(double), size, value, size_ret);
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

/* =======================================================================
 * Job interface (9 functions)
 * ======================================================================= */

int MOCK_QDMI_device_session_create_device_job(void *session, void **job_out) {
    if (!session || !job_out) return QDMI_ERROR_INVALIDARGUMENT;

    MockJob *j = (MockJob*)calloc(1, sizeof(MockJob));
    if (!j) return QDMI_ERROR_OUTOFMEM;

    j->status = QDMI_JOB_STATUS_CREATED;
    j->shots = 1024;
    *job_out = j;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_job_set_parameter(
    void *job, int param, size_t size, const void *value
) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    MockJob *j = (MockJob*)job;

    switch (param) {
    case QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT:
        if (value && size >= sizeof(int32_t)) {
            j->program_format = *(const int32_t*)value;
        }
        return QDMI_SUCCESS;
    case QDMI_DEVICE_JOB_PARAMETER_PROGRAM:
        if (j->program) free(j->program);
        j->program = (char*)malloc(size + 1);
        if (!j->program) return QDMI_ERROR_OUTOFMEM;
        memcpy(j->program, value, size);
        j->program[size] = '\0';
        return QDMI_SUCCESS;
    case QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM:
        if (value && size >= sizeof(size_t)) {
            j->shots = *(const size_t*)value;
        }
        return QDMI_SUCCESS;
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

int MOCK_QDMI_device_job_query_property(
    void *job, int prop, size_t size, void *value, size_t *size_ret
) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    MockJob *j = (MockJob*)job;

    switch (prop) {
    case QDMI_DEVICE_JOB_PROPERTY_ID: {
        const char *id = "mock-job-001";
        return write_property(id, strlen(id) + 1, size, value, size_ret);
    }
    default:
        (void)j;
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

int MOCK_QDMI_device_job_submit(void *job) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    MockJob *j = (MockJob*)job;
    j->status = QDMI_JOB_STATUS_SUBMITTED;
    /* Mock: immediately transition to DONE */
    j->status = QDMI_JOB_STATUS_DONE;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_job_cancel(void *job) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    /* Mock: just succeed */
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_job_check(void *job, int *status) {
    if (!job || !status) return QDMI_ERROR_INVALIDARGUMENT;
    MockJob *j = (MockJob*)job;
    *status = j->status;
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_job_wait(void *job, size_t timeout_ms) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    (void)timeout_ms;
    /* Mock: already done, just return success */
    return QDMI_SUCCESS;
}

int MOCK_QDMI_device_job_get_results(
    void *job, int result_type, size_t size, void *value, size_t *size_ret
) {
    if (!job) return QDMI_ERROR_INVALIDARGUMENT;
    (void)job;

    switch (result_type) {
    case QDMI_JOB_RESULT_HISTKEYS: {
        /* Mock: return "00000" and "11111" as null-separated keys */
        const char *keys = "00000\011111\0";
        size_t keys_len = 12; /* two 6-byte null-terminated strings */
        return write_property(keys, keys_len, size, value, size_ret);
    }
    case QDMI_JOB_RESULT_HISTVALUES: {
        /* Mock: return counts [512, 512] */
        size_t counts[2] = { 512, 512 };
        return write_property(counts, sizeof(counts), size, value, size_ret);
    }
    default:
        return QDMI_ERROR_NOTSUPPORTED;
    }
}

void MOCK_QDMI_device_job_free(void *job) {
    if (job) {
        MockJob *j = (MockJob*)job;
        if (j->program) free(j->program);
        free(job);
    }
}
