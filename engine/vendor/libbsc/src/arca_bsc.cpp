/* Arca: bsc_decompress with its two parallel stages (the QLFC sub-blocks, and
   the sampled segments of the inverse BWT) on plain threads, for a build
   without OpenMP. Same format and the same checks as bsc_decompress; the LZP
   stage stays serial, as in libbsc. */

#include <atomic>
#include <cstring>
#include <thread>
#include <vector>

#include "../libbsc/libbsc.h"
#include "../libbsc/platform/platform.h"
#include "../libbsc/adler32/adler32.h"
#include "../libbsc/lzp/lzp.h"
#include "../libbsc/bwt/libsais/libsais.h"

int bsc_coder_decode_block(const unsigned char * input, unsigned char * output, int coder);
extern "C" int32_t arca_unbwt_aux_mt(const uint8_t * T, uint8_t * U, int32_t * A, int32_t n, int32_t r, const int32_t * I, int32_t threads);

static const int FEATURES = LIBBSC_FEATURE_FASTMODE;

/* bsc_coder_decompress, its blocks shared out to `threads` threads. */
static int arca_coder_decompress(const unsigned char * input, unsigned char * output, int coder, int threads)
{
    int nBlocks = input[0];
    if (nBlocks == 1)
    {
        return bsc_coder_decode_block(input + 1, output, coder);
    }

    std::vector<int> inPtr(nBlocks), inSize(nBlocks), outPtr(nBlocks), outSize(nBlocks), result(nBlocks);
    int ip = 1 + 8 * nBlocks, op = 0;
    for (int b = 0; b < nBlocks; ++b)
    {
        memcpy(&inSize[b] , input + 1 + 8 * b + 4, sizeof(int));
        memcpy(&outSize[b], input + 1 + 8 * b + 0, sizeof(int));
        inPtr[b] = ip; outPtr[b] = op; ip += inSize[b]; op += outSize[b];
    }

    std::atomic<int> next(0);
    auto work = [&]()
    {
        for (int b; (b = next.fetch_add(1)) < nBlocks; )
        {
            if (inSize[b] != outSize[b])
            {
                result[b] = bsc_coder_decode_block(input + inPtr[b], output + outPtr[b], coder);
            }
            else
            {
                result[b] = inSize[b]; memcpy(output + outPtr[b], input + inPtr[b], inSize[b]);
            }
        }
    };
    std::vector<std::thread> pool;
    for (int t = 1; t < threads && t < nBlocks; ++t) pool.emplace_back(work);
    work();
    for (auto & th : pool) th.join();

    int dataSize = 0, res = LIBBSC_NO_ERROR;
    for (int b = 0; b < nBlocks; ++b)
    {
        if (result[b] < LIBBSC_NO_ERROR) res = result[b];
        dataSize += result[b];
    }
    return res == LIBBSC_NO_ERROR ? dataSize : res;
}

/* bsc_bwt_decode with the threaded inverse BWT. */
static int arca_bwt_decode(unsigned char * T, int n, int index, unsigned char num_indexes, int * indexes, int threads)
{
    if ((T == NULL) || (n < 0) || (index <= 0) || (index > n)) return LIBBSC_BAD_PARAMETER;
    if (n <= 1) return LIBBSC_NO_ERROR;

    int * P = (int *)bsc_malloc((size_t)(n + 1) * sizeof(int));
    if (P == NULL) return LIBBSC_NOT_ENOUGH_MEMORY;

    int mod = n / 8;
    mod |= mod >> 1; mod |= mod >> 2; mod |= mod >> 4; mod |= mod >> 8; mod |= mod >> 16; mod >>= 1;

    int r;
    if (num_indexes == (unsigned char)((n - 1) / (mod + 1)) && indexes != NULL)
    {
        int I[256]; I[0] = index; for (int t = 0; t < num_indexes; ++t) { I[t + 1] = indexes[t] + 1; }
        r = arca_unbwt_aux_mt(T, T, P, n, mod + 1, I, threads);
    }
    else
    {
        r = libsais_unbwt(T, T, P, n, NULL, index);
    }
    bsc_free(P);

    switch (r)
    {
        case -1 : return LIBBSC_BAD_PARAMETER;
        case -2 : return LIBBSC_NOT_ENOUGH_MEMORY;
    }
    return LIBBSC_NO_ERROR;
}

extern "C" int arca_bsc_decompress_mt(const unsigned char * input, int inputSize, unsigned char * output, int outputSize, int threads)
{
    int blockSize = 0, dataSize = 0;
    int info = bsc_block_info(input, inputSize, &blockSize, &dataSize, FEATURES);
    if (info != LIBBSC_NO_ERROR) return info;
    if (inputSize < blockSize || outputSize < dataSize) return LIBBSC_UNEXPECTED_EOB;
    if (*(unsigned int *)(input + 20) != bsc_adler32(input + LIBBSC_HEADER_SIZE, blockSize - LIBBSC_HEADER_SIZE, FEATURES)) return LIBBSC_DATA_CORRUPT;

    int mode = *(int *)(input + 8);
    if (mode == 0)
    {
        memcpy(output, input + LIBBSC_HEADER_SIZE, dataSize);
        return LIBBSC_NO_ERROR;
    }
    int blockSorter = (mode >> 0) & 0x1f;
    if (blockSorter != LIBBSC_BLOCKSORTER_BWT)
    {
        return bsc_decompress(input, inputSize, output, outputSize, FEATURES);
    }

    int           index        = *(int *)(input + 12);
    unsigned int  adler32_data = *(int *)(input + 16);
    int           indexes[256];
    unsigned char num_indexes  = input[blockSize - 1];
    if (num_indexes > 0)
    {
        if (blockSize - 1 - 4 * num_indexes < LIBBSC_HEADER_SIZE) return LIBBSC_DATA_CORRUPT;
        memcpy(indexes, input + blockSize - 1 - 4 * num_indexes, 4 * num_indexes);
    }
    int lzpHashSize = (mode >> 16) & 0xff;
    int lzpMinLen   = (mode >>  8) & 0xff;
    int coder       = (mode >>  5) & 0x7;
    if (coder != LIBBSC_CODER_QLFC_STATIC && coder != LIBBSC_CODER_QLFC_ADAPTIVE && coder != LIBBSC_CODER_QLFC_FAST) return LIBBSC_BAD_PARAMETER;

    int lzSize = arca_coder_decompress(input + LIBBSC_HEADER_SIZE, output, coder, threads);
    if (lzSize < LIBBSC_NO_ERROR) return lzSize;

    int result = arca_bwt_decode(output, lzSize, index, num_indexes, indexes, threads);
    if (result < LIBBSC_NO_ERROR) return result;

    if (mode != (mode & 0xff))
    {
        unsigned char * buffer = (unsigned char *)bsc_malloc(lzSize);
        if (buffer == NULL) return LIBBSC_NOT_ENOUGH_MEMORY;
        memcpy(buffer, output, lzSize);
        result = bsc_lzp_decompress(buffer, output, lzSize, lzpHashSize, lzpMinLen, FEATURES);
        bsc_free(buffer);
        if (result < LIBBSC_NO_ERROR) return result;
        return result == dataSize ? (adler32_data == bsc_adler32(output, dataSize, FEATURES) ? LIBBSC_NO_ERROR : LIBBSC_DATA_CORRUPT) : LIBBSC_DATA_CORRUPT;
    }
    return lzSize == dataSize ? (adler32_data == bsc_adler32(output, dataSize, FEATURES) ? LIBBSC_NO_ERROR : LIBBSC_DATA_CORRUPT) : LIBBSC_DATA_CORRUPT;
}
