/**
 * SPDX-License-Identifier: Apache-2.0
 *
 * Test harness for the QDMI device interface pattern.
 *
 * This C program mirrors exactly what arvak-qdmi's device_loader.rs does:
 *   1. dlopen the device .so
 *   2. dlsym with prefix-shifted names ("MOCK_QDMI_device_session_*")
 *   3. Open a session
 *   4. Query all device/site/operation properties
 *   5. Validate results
 *
 * Compile & run:
 *   gcc -o test_harness test_harness.c -ldl -Wall -Wextra
 *   ./test_harness ./libmock_qdmi_device.so
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include <stdint.h>

/* ------- QDMI constants (must match mock_device.c and ffi.rs) ------- */

#define QDMI_SUCCESS               0
#define QDMI_ERROR_INVALIDARGUMENT 1
#define QDMI_ERROR_NOTSUPPORTED    2

#define QDMI_DEVICE_PROPERTY_NAME          0
#define QDMI_DEVICE_PROPERTY_VERSION       1
#define QDMI_DEVICE_PROPERTY_QUBITSNUM     3
#define QDMI_DEVICE_PROPERTY_SITES         4
#define QDMI_DEVICE_PROPERTY_COUPLINGMAP   5
#define QDMI_DEVICE_PROPERTY_OPERATIONS    6

#define QDMI_SITE_PROPERTY_T1              0
#define QDMI_SITE_PROPERTY_T2              1
#define QDMI_SITE_PROPERTY_READOUTERROR    2
#define QDMI_SITE_PROPERTY_FREQUENCY       4

#define QDMI_OPERATION_PROPERTY_NAME       0
#define QDMI_OPERATION_PROPERTY_DURATION   1
#define QDMI_OPERATION_PROPERTY_FIDELITY   2
#define QDMI_OPERATION_PROPERTY_QUBITSNUM  3

/* ------- Function pointer types ------- */

typedef int (*fn_session_init)(void **session_out);
typedef int (*fn_session_deinit)(void *session);
typedef int (*fn_query_device_prop)(void *session, int prop, size_t size, void *value, size_t *size_ret);
typedef int (*fn_query_site_prop)(void *session, void *site, int prop, size_t size, void *value, size_t *size_ret);
typedef int (*fn_query_op_prop)(void *session, void *op, int prop, size_t size, void *value, size_t *size_ret);

/* ------- Test infrastructure ------- */

static int tests_run = 0;
static int tests_passed = 0;
static int tests_failed = 0;

#define ASSERT(cond, msg) do { \
    tests_run++; \
    if (!(cond)) { \
        fprintf(stderr, "  FAIL: %s (line %d)\n", msg, __LINE__); \
        tests_failed++; \
    } else { \
        tests_passed++; \
    } \
} while(0)

#define ASSERT_EQ_INT(a, b, msg) ASSERT((a) == (b), msg)
#define ASSERT_EQ_STR(a, b, msg) ASSERT(strcmp((a), (b)) == 0, msg)

/* ------- Prefix-aware symbol resolution (the core pattern) ------- */

static void *resolve_symbol(void *handle, const char *prefix, const char *base_name) {
    char sym[256];
    snprintf(sym, sizeof(sym), "%s_%s", prefix, base_name);
    void *fn = dlsym(handle, sym);
    if (!fn) {
        fprintf(stderr, "  dlsym failed for '%s': %s\n", sym, dlerror());
    }
    return fn;
}

/* ------- Two-phase query helper (matches session.rs logic) ------- */

static int query_device_prop_buf(
    fn_query_device_prop query_fn, void *session,
    int prop, void *buf, size_t buf_size, size_t *actual_size
) {
    /* Phase 1: size probe */
    size_t needed = 0;
    int ret = query_fn(session, prop, 0, NULL, &needed);
    if (ret != QDMI_SUCCESS) return ret;

    if (actual_size) *actual_size = needed;

    /* Phase 2: data read */
    if (buf && buf_size >= needed) {
        ret = query_fn(session, prop, needed, buf, NULL);
    }
    return ret;
}

/* ------- Main test ------- */

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <path-to-libmock_qdmi_device.so>\n", argv[0]);
        return 1;
    }

    const char *lib_path = argv[1];
    const char *prefix = "MOCK";

    printf("=== QDMI Device Interface Test Harness ===\n");
    printf("Library: %s\n", lib_path);
    printf("Prefix:  %s\n\n", prefix);

    /* ---- Load library ------------------------------------------------- */

    void *handle = dlopen(lib_path, RTLD_NOW);
    if (!handle) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }
    printf("[OK] Library loaded\n");

    /* ---- Resolve symbols with prefix ---------------------------------- */

    fn_session_init      init_fn    = (fn_session_init)     resolve_symbol(handle, prefix, "QDMI_device_session_init");
    fn_session_deinit    deinit_fn  = (fn_session_deinit)   resolve_symbol(handle, prefix, "QDMI_device_session_deinit");
    fn_query_device_prop dev_fn     = (fn_query_device_prop)resolve_symbol(handle, prefix, "QDMI_device_session_query_device_property");
    fn_query_site_prop   site_fn    = (fn_query_site_prop)  resolve_symbol(handle, prefix, "QDMI_device_session_query_site_property");
    fn_query_op_prop     op_fn      = (fn_query_op_prop)    resolve_symbol(handle, prefix, "QDMI_device_session_query_operation_property");

    ASSERT(init_fn   != NULL, "resolve session_init");
    ASSERT(deinit_fn != NULL, "resolve session_deinit");
    ASSERT(dev_fn    != NULL, "resolve query_device_property");
    ASSERT(site_fn   != NULL, "resolve query_site_property");
    ASSERT(op_fn     != NULL, "resolve query_operation_property");
    printf("[OK] All required symbols resolved\n");

    /* ---- Open session ------------------------------------------------- */

    void *session = NULL;
    int ret = init_fn(&session);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "session_init returns success");
    ASSERT(session != NULL, "session handle is non-null");
    printf("[OK] Session opened (handle: %p)\n", session);

    /* ---- Query device name -------------------------------------------- */

    char name[256] = {0};
    size_t name_size = 0;
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_NAME, name, sizeof(name), &name_size);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query device name");
    ASSERT_EQ_STR(name, "Arvak Mock Device (5Q Linear)", "device name value");
    printf("[OK] Device name: '%s'\n", name);

    /* ---- Query version ------------------------------------------------ */

    char ver[64] = {0};
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_VERSION, ver, sizeof(ver), NULL);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query version");
    ASSERT_EQ_STR(ver, "0.1.0", "version value");
    printf("[OK] Version: '%s'\n", ver);

    /* ---- Query qubit count -------------------------------------------- */

    size_t num_qubits = 0;
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_QUBITSNUM, &num_qubits, sizeof(num_qubits), NULL);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query num_qubits");
    ASSERT_EQ_INT((int)num_qubits, 5, "num_qubits == 5");
    printf("[OK] Qubits: %zu\n", num_qubits);

    /* ---- Query sites -------------------------------------------------- */

    uintptr_t sites[16] = {0};
    size_t sites_size = 0;
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_SITES, sites, sizeof(sites), &sites_size);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query sites");
    size_t num_sites = sites_size / sizeof(uintptr_t);
    ASSERT_EQ_INT((int)num_sites, 5, "5 sites returned");
    printf("[OK] Sites: %zu [", num_sites);
    for (size_t i = 0; i < num_sites; i++) {
        printf("0x%lx%s", (unsigned long)sites[i], i < num_sites-1 ? ", " : "");
    }
    printf("]\n");

    /* ---- Query coupling map ------------------------------------------- */

    uintptr_t cmap[64] = {0};
    size_t cmap_size = 0;
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_COUPLINGMAP, cmap, sizeof(cmap), &cmap_size);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query coupling map");
    size_t num_edges = cmap_size / (sizeof(uintptr_t) * 2);
    ASSERT_EQ_INT((int)num_edges, 8, "8 directed edges (linear 5Q)");
    printf("[OK] Coupling map: %zu directed edges\n", num_edges);
    for (size_t i = 0; i < num_edges; i++) {
        printf("     0x%lx → 0x%lx\n",
            (unsigned long)cmap[i*2], (unsigned long)cmap[i*2+1]);
    }

    /* ---- Query operations --------------------------------------------- */

    uintptr_t ops[16] = {0};
    size_t ops_size = 0;
    ret = query_device_prop_buf(dev_fn, session, QDMI_DEVICE_PROPERTY_OPERATIONS, ops, sizeof(ops), &ops_size);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "query operations");
    size_t num_ops = ops_size / sizeof(uintptr_t);
    ASSERT_EQ_INT((int)num_ops, 3, "3 operations (H, CX, RZ)");
    printf("[OK] Operations: %zu\n", num_ops);

    /* ---- Query per-operation properties ------------------------------- */

    for (size_t i = 0; i < num_ops; i++) {
        void *op_handle = (void*)ops[i];

        /* Name */
        char op_name[64] = {0};
        size_t op_name_size = 0;
        ret = op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_NAME, 0, NULL, &op_name_size);
        if (ret == QDMI_SUCCESS) {
            op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_NAME, op_name_size, op_name, NULL);
        }

        /* Fidelity */
        double fidelity = 0.0;
        size_t fid_size = 0;
        ret = op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_FIDELITY, 0, NULL, &fid_size);
        if (ret == QDMI_SUCCESS) {
            op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_FIDELITY, sizeof(double), &fidelity, NULL);
        }
        ASSERT(fidelity > 0.0 && fidelity <= 1.0, "fidelity in valid range");

        /* Duration */
        double duration = 0.0;
        size_t dur_size = 0;
        ret = op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_DURATION, 0, NULL, &dur_size);
        if (ret == QDMI_SUCCESS) {
            op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_DURATION, sizeof(double), &duration, NULL);
        }

        /* Qubit count */
        size_t op_qubits = 0;
        size_t oq_size = 0;
        ret = op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_QUBITSNUM, 0, NULL, &oq_size);
        if (ret == QDMI_SUCCESS) {
            op_fn(session, op_handle, QDMI_OPERATION_PROPERTY_QUBITSNUM, sizeof(size_t), &op_qubits, NULL);
        }

        printf("[OK] Op '%s': fidelity=%.4f, duration=%.1fns, qubits=%zu\n",
            op_name, fidelity, duration * 1e9, op_qubits);
    }

    /* ---- Query per-site properties ------------------------------------ */

    for (size_t i = 0; i < num_sites; i++) {
        void *site_handle = (void*)sites[i];
        double t1 = 0, t2 = 0, readout_err = 0, freq = 0;
        size_t sz = 0;

        site_fn(session, site_handle, QDMI_SITE_PROPERTY_T1, 0, NULL, &sz);
        site_fn(session, site_handle, QDMI_SITE_PROPERTY_T1, sizeof(double), &t1, NULL);

        site_fn(session, site_handle, QDMI_SITE_PROPERTY_T2, 0, NULL, &sz);
        site_fn(session, site_handle, QDMI_SITE_PROPERTY_T2, sizeof(double), &t2, NULL);

        site_fn(session, site_handle, QDMI_SITE_PROPERTY_READOUTERROR, 0, NULL, &sz);
        site_fn(session, site_handle, QDMI_SITE_PROPERTY_READOUTERROR, sizeof(double), &readout_err, NULL);

        site_fn(session, site_handle, QDMI_SITE_PROPERTY_FREQUENCY, 0, NULL, &sz);
        site_fn(session, site_handle, QDMI_SITE_PROPERTY_FREQUENCY, sizeof(double), &freq, NULL);

        ASSERT(t1 > 0, "T1 > 0");
        ASSERT(t2 > 0, "T2 > 0");
        ASSERT(t1 >= t2, "T1 >= T2");
        ASSERT(readout_err > 0 && readout_err < 1, "readout error in range");
        ASSERT(freq > 4e9 && freq < 6e9, "frequency in GHz range");

        printf("[OK] Site %zu (0x%lx): T1=%.0fμs T2=%.0fμs readout_err=%.3f freq=%.2fGHz\n",
            i, (unsigned long)sites[i],
            t1 * 1e6, t2 * 1e6, readout_err, freq / 1e9);
    }

    /* ---- Test unsupported property ------------------------------------ */

    ret = dev_fn(session, 999, 0, NULL, NULL);
    ASSERT_EQ_INT(ret, QDMI_ERROR_NOTSUPPORTED, "unsupported property returns NOTSUPPORTED");
    printf("[OK] Unsupported property correctly returns NOTSUPPORTED\n");

    /* ---- Close session ------------------------------------------------ */

    ret = deinit_fn(session);
    ASSERT_EQ_INT(ret, QDMI_SUCCESS, "session_deinit returns success");
    printf("[OK] Session closed\n");

    /* ---- Cleanup ------------------------------------------------------ */

    dlclose(handle);

    /* ---- Summary ------------------------------------------------------ */

    printf("\n=== Results: %d tests, %d passed, %d failed ===\n",
        tests_run, tests_passed, tests_failed);

    return tests_failed > 0 ? 1 : 0;
}
