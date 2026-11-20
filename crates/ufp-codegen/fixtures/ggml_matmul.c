/* Generated-code proof fixture for ufp-codegen's narrow ggml f32 slice. */
#include <math.h>
#include <stdio.h>
#include <ggml.h>
#include <ggml-backend.h>
#include <ggml-alloc.h>
#include <ggml-cpu.h>

int main(void) {
    struct ggml_init_params params = {
        .mem_size = 16 * 1024 * 1024,
        .mem_buffer = NULL,
        .no_alloc = true,
    };
    struct ggml_context * ctx = ggml_init(params);
    if (!ctx) return 1;

    /* ggml stores dimension 0 contiguously; A is KxM, B is KxN. */
    struct ggml_tensor * a = ggml_new_tensor_2d(ctx, GGML_TYPE_F32, 3, 2);
    struct ggml_tensor * b = ggml_new_tensor_2d(ctx, GGML_TYPE_F32, 3, 2);
    const float av[] = {1, 2, 3, 4, 5, 6};
    const float bv[] = {7, 8, 9, 10, 11, 12};
    ggml_set_name(a, "A");
    ggml_set_name(b, "B");
    struct ggml_tensor * product = ggml_mul_mat(ctx, a, b);
    struct ggml_cgraph * graph = ggml_new_graph(ctx);
    ggml_build_forward_expand(graph, product);

    ggml_backend_t backend = ggml_backend_cpu_init();
    if (!backend || !ggml_backend_alloc_ctx_tensors(ctx, backend)) {
        fprintf(stderr, "failed to initialize ggml CPU backend/tensors\n");
        if (backend) ggml_backend_free(backend);
        ggml_free(ctx);
        return 1;
    }
    ggml_backend_tensor_set(a, av, 0, sizeof(av));
    ggml_backend_tensor_set(b, bv, 0, sizeof(bv));
    if (ggml_backend_graph_compute(backend, graph) != GGML_STATUS_SUCCESS) {
        fprintf(stderr, "ggml graph computation failed\n");
        ggml_backend_free(backend);
        ggml_free(ctx);
        return 1;
    }

    /* ggml dim 0 is contiguous: output rows are the A rows. */
    const float expected[] = {50, 122, 68, 167};
    float actual[4];
    ggml_backend_tensor_get(product, actual, 0, sizeof(actual));
    int ok = 1;
    for (int i = 0; i < 4; ++i) {
        if (fabsf(actual[i] - expected[i]) > 1e-5f) {
            fprintf(stderr, "result[%d]: got %.6f, expected %.6f\n",
                    i, actual[i], expected[i]);
            ok = 0;
        }
    }
    ggml_backend_free(backend);
    ggml_free(ctx);
    if (ok) puts("ggml f32 matmul smoke: ok");
    return ok ? 0 : 1;
}
