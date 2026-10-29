/* Arca: libsais, compiled through this file instead of on its own, plus an
   inverse BWT that decodes its sampled segments on several threads. libsais's
   OpenMP build does the same with `#pragma omp`; Arca builds without OpenMP and
   uses plain threads. libsais.c is included unchanged. */

#include "../libbsc/bwt/libsais/libsais.c"

#include <pthread.h>

int32_t arca_unbwt_aux_mt(const uint8_t * T, uint8_t * U, int32_t * A, int32_t n, int32_t r, const int32_t * I, int32_t threads);

typedef struct
{
    uint8_t *   U;
    sa_uint_t * P;
    sa_sint_t   n;
    sa_sint_t   r;
    sa_uint_t * I;
    sa_uint_t * bucket2;
    uint16_t *  fastbits;
    fast_sint_t start;
    fast_sint_t count;
    fast_uint_t remainder;
} arca_unbwt_part;

static void * arca_unbwt_run(void * arg)
{
    arca_unbwt_part * p = (arca_unbwt_part *)arg;
    libsais_unbwt_decode(p->U + (fast_uint_t)p->r * (fast_uint_t)p->start, p->P, p->n, p->r, p->I + p->start, p->bucket2, p->fastbits, p->count, p->remainder);
    return NULL;
}

/* As libsais_unbwt_aux (same checks, same result), with the segments split
   over up to `threads` threads the way libsais_unbwt_decode_omp splits them. */
int32_t arca_unbwt_aux_mt(const uint8_t * T, uint8_t * U, int32_t * A, int32_t n, int32_t r, const int32_t * I, int32_t threads)
{
    if (threads <= 1 || n < 262144)
    {
        return libsais_unbwt_aux(T, U, A, n, NULL, r, I);
    }
    if ((T == NULL) || (U == NULL) || (A == NULL) || ((r != n) && ((r < 2) || ((r & (r - 1)) != 0))) || (I == NULL))
    {
        return -1;
    }
    fast_sint_t t; for (t = 0; t <= (n - 1) / r; ++t) { if (I[t] <= 0 || I[t] > n) { return -1; } }

    fast_uint_t shift = 0; while ((n >> shift) > ((sa_sint_t)1 << UNBWT_FASTBITS)) { shift++; }
    sa_uint_t * bucket2  = (sa_uint_t *)libsais_alloc_aligned(ALPHABET_SIZE * ALPHABET_SIZE * sizeof(sa_uint_t), 4096);
    uint16_t *  fastbits = (uint16_t *)libsais_alloc_aligned(((size_t)1 + (size_t)(n >> shift)) * sizeof(uint16_t), 4096);
    if (bucket2 == NULL || fastbits == NULL)
    {
        libsais_free_aligned(fastbits);
        libsais_free_aligned(bucket2);
        return -2;
    }

    libsais_unbwt_init_single(T, (sa_uint_t *)A, n, NULL, (const sa_uint_t *)I, bucket2, fastbits);

    /* T and U may be the same buffer: keep T[0] before decoding overwrites it. */
    fast_uint_t lastc     = T[0];
    fast_sint_t blocks    = 1 + (((fast_sint_t)n - 1) / (fast_sint_t)r);
    fast_uint_t remainder = (fast_uint_t)n - ((fast_uint_t)r * ((fast_uint_t)blocks - 1));
    fast_sint_t k         = blocks < threads ? blocks : threads;
    if (k > 64) { k = 64; }

    arca_unbwt_part parts[64];
    pthread_t       tids[64];
    int             started[64];
    fast_sint_t     stride = blocks / k, extra = blocks % k, i;
    for (i = 0; i < k; ++i)
    {
        arca_unbwt_part p = { U, (sa_uint_t *)A, n, r, (sa_uint_t *)I, bucket2, fastbits,
                              stride * i + (i < extra ? i : extra), stride + (i < extra), i < k - 1 ? (fast_uint_t)r : remainder };
        parts[i] = p;
    }
    for (i = 1; i < k; ++i)
    {
        started[i] = pthread_create(&tids[i], NULL, arca_unbwt_run, &parts[i]) == 0;
        if (!started[i]) { arca_unbwt_run(&parts[i]); }
    }
    arca_unbwt_run(&parts[0]);
    for (i = 1; i < k; ++i)
    {
        if (started[i]) { pthread_join(tids[i], NULL); }
    }

    U[n - 1] = (uint8_t)lastc;
    libsais_free_aligned(fastbits);
    libsais_free_aligned(bucket2);
    return 0;
}
