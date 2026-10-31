// GPU match finding for zstd frames: a feasibility test (research, not in the engine).
//
// The GPU finds, for every position of each 8 MiB block, the longest match among
// three candidates (zstd's long and short hashes over the block's history, and the
// earliest same-hash position inside the current 1024-byte window); the CPU runs a
// lazy parse over them and hands the sequences to ZSTD_compressSequences, so the
// output is an ordinary zstd frame. Compared against ZSTD_compress2 at level 3 with
// the same 8 MiB window, both checked by decompressing.
//
// Build (DEVELOPER_DIR set):
//   xcrun clang++ -std=c++17 -O2 -fobjc-arc -framework Metal -framework Foundation \
//     -I/opt/homebrew/include -L/opt/homebrew/lib -lzstd gpumatch.mm -o gpumatch
// Run:  ./gpumatch [-g groups] [-l hlogL] [-s hlogS] [-r repeats] files...

#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#define ZSTD_STATIC_LINKING_ONLY
#include <zstd.h>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include <dispatch/dispatch.h>

static const char* kSource = R"MSL(
#include <metal_stdlib>
using namespace metal;

struct Params { uint hlogL; uint hlogS; uint groups; uint per; };

inline ulong load8(device const uchar* d, uint p) {
    ulong v = 0;
    for (uint i = 0; i < 8; i++) v |= (ulong)d[p + i] << (8 * i);
    return v;
}

// Equal bytes from p and c on, at most cap.
inline uint match_len(device const uchar* d, uint c, uint p, uint cap) {
    uint l = 0;
    while (l + 4 <= cap) {
        uchar4 a = uchar4(*(device const packed_uchar4*)(d + c + l));
        uchar4 b = uchar4(*(device const packed_uchar4*)(d + p + l));
        if (any(a != b)) {
            if (a.x != b.x) return l;
            if (a.y != b.y) return l + 1;
            if (a.z != b.z) return l + 2;
            return l + 3;
        }
        l += 4;
    }
    while (l < cap && d[c + l] == d[p + l]) l++;
    return l;
}

kernel void find_matches(device const uchar* data [[buffer(0)]],
                         device const uint2* blocks [[buffer(1)]],
                         device uint* tables [[buffer(2)]],
                         device uint* out [[buffer(3)]],
                         constant Params& P [[buffer(4)]],
                         uint tid [[thread_index_in_threadgroup]],
                         uint tg [[threadgroup_position_in_grid]],
                         uint tgsize [[threads_per_threadgroup]]) {
    threadgroup atomic_uint sh[2048];
    const uint b = tg / P.groups, seg = tg % P.groups;
    const uint bs = blocks[b].x, be = blocks[b].y;
    const uint segLen = (be - bs + P.groups - 1) / P.groups;
    const uint s0 = bs + seg * segLen;
    const uint s1 = min(s0 + segLen, be);
    device uint* tL = tables + (ulong)b * ((1u << P.hlogL) + (1u << P.hlogS));
    device uint* tS = tL + (1u << P.hlogL);
    const uint W = tgsize * P.per;   // positions per window: P.per per thread
    for (uint base = s0; base < s1; base += W) {
        for (uint i = tid; i < 2048; i += tgsize) atomic_store_explicit(&sh[i], 0xFFFFFFFFu, memory_order_relaxed);
        threadgroup_barrier(mem_flags::mem_threadgroup);
        ulong v[8]; uint hL[8], hS[8], hW[8]; bool live[8];
        for (uint k = 0; k < P.per; k++) {
            const uint p = base + tid + k * tgsize;
            live[k] = p < s1 && p + 8 <= be;
            v[k] = live[k] ? load8(data, p) : 0;
            hL[k] = (uint)((v[k] * 0xCF1BBCDCB7A56463ul) >> (64 - P.hlogL));
            hS[k] = (uint)(((v[k] << 24) * 0xCF1BBCDCBBul) >> (64 - P.hlogS));
            hW[k] = ((uint)v[k] * 2654435761u) >> 21;
            if (live[k]) atomic_fetch_min_explicit(&sh[hW[k]], p - base, memory_order_relaxed);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
        for (uint k = 0; k < P.per; k++) {
            const uint p = base + tid + k * tgsize;
            uint best = 0, bestLen = 0, bestOff = 0;
            if (live[k]) {
                const uint w = atomic_load_explicit(&sh[hW[k]], memory_order_relaxed);
                uint cand[3] = { tL[hL[k]], tS[hS[k]], (w < p - base) ? base + w : 0xFFFFFFFFu };
                const uint cap = min(be - p, 511u);
                for (uint j = 0; j < 3; j++) {
                    const uint c = cand[j];
                    if (c == 0xFFFFFFFFu || c >= p || c < bs) continue;
                    const uint off = p - c;
                    if (off >= (1u << 23)) continue;
                    const uint l = match_len(data, c, p, cap);
                    if (l >= 4 && (l > bestLen || (l == bestLen && off < bestOff))) { bestLen = l; bestOff = off; }
                }
                if (bestLen) best = (bestLen << 23) | bestOff;
            }
            if (p < s1) out[p] = best;
        }
        threadgroup_barrier(mem_flags::mem_device | mem_flags::mem_threadgroup);
        for (uint k = 0; k < P.per; k++) {
            const uint p = base + tid + k * tgsize;
            if (live[k]) { tL[hL[k]] = p; tS[hS[k]] = p; }
        }
        threadgroup_barrier(mem_flags::mem_device);
    }
}
)MSL";

struct Params { uint32_t hlogL, hlogS, groups, per; };

static double now() {
    return std::chrono::duration<double>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

static void fail(const char* what) { fprintf(stderr, "error: %s\n", what); exit(1); }

int main(int argc, char** argv) {
    uint32_t groups = 4, hlogL = 17, hlogS = 16, repeats = 3, per = 1;
    bool lazy = true, validate = false;
    std::vector<std::string> files;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        if (a == "-g" && i + 1 < argc) groups = atoi(argv[++i]);
        else if (a == "-l" && i + 1 < argc) hlogL = atoi(argv[++i]);
        else if (a == "-s" && i + 1 < argc) hlogS = atoi(argv[++i]);
        else if (a == "-r" && i + 1 < argc) repeats = atoi(argv[++i]);
        else if (a == "-k" && i + 1 < argc) per = std::min(8, std::max(1, atoi(argv[++i])));
        else if (a == "--greedy") lazy = false;
        else if (a == "-v") validate = true;
        else files.push_back(a);
    }
    if (files.empty()) fail("no files");

    @autoreleasepool {
        id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
        if (!dev) fail("no Metal device");
        // Read every file into one shared buffer; blocks of 8 MiB per file.
        const uint32_t BLOCK = 8u << 20;
        std::vector<std::pair<std::string, std::pair<uint32_t, uint32_t>>> fileRanges;
        std::vector<uint32_t> blockPairs;
        size_t total = 0;
        std::vector<std::vector<uint8_t>> contents;
        for (auto& f : files) {
            FILE* fp = fopen(f.c_str(), "rb");
            if (!fp) fail(("open " + f).c_str());
            fseek(fp, 0, SEEK_END); long n = ftell(fp); fseek(fp, 0, SEEK_SET);
            std::vector<uint8_t> buf(n);
            if (fread(buf.data(), 1, n, fp) != (size_t)n) fail("read");
            fclose(fp);
            fileRanges.push_back({f, {(uint32_t)total, (uint32_t)(total + n)}});
            for (size_t off = 0; off < (size_t)n; off += BLOCK) {
                blockPairs.push_back((uint32_t)(total + off));
                blockPairs.push_back((uint32_t)(total + std::min((size_t)n, off + BLOCK)));
            }
            total += n;
            contents.push_back(std::move(buf));
        }
        const uint32_t nblocks = blockPairs.size() / 2;
        id<MTLBuffer> data = [dev newBufferWithLength:total + 64 options:MTLResourceStorageModeShared];
        uint8_t* dp = (uint8_t*)data.contents;
        { size_t at = 0; for (auto& c : contents) { memcpy(dp + at, c.data(), c.size()); at += c.size(); } memset(dp + total, 0, 64); }
        contents.clear();
        id<MTLBuffer> blocks = [dev newBufferWithBytes:blockPairs.data() length:blockPairs.size() * 4 options:MTLResourceStorageModeShared];
        const size_t tableWords = (size_t)nblocks * ((1u << hlogL) + (1u << hlogS));
        id<MTLBuffer> tables = [dev newBufferWithLength:tableWords * 4 options:MTLResourceStorageModePrivate];
        id<MTLBuffer> out = [dev newBufferWithLength:(total + 1024) * 4 options:MTLResourceStorageModeShared];
        Params P = { hlogL, hlogS, groups, per };

        NSError* err = nil;
        MTLCompileOptions* copts = [MTLCompileOptions new];
        id<MTLLibrary> lib = [dev newLibraryWithSource:[NSString stringWithUTF8String:kSource] options:copts error:&err];
        if (!lib) { fprintf(stderr, "%s\n", err.localizedDescription.UTF8String); fail("compile"); }
        id<MTLComputePipelineState> pso = [dev newComputePipelineStateWithFunction:[lib newFunctionWithName:@"find_matches"] error:&err];
        if (!pso) fail("pipeline");
        const NSUInteger tgsize = std::min<NSUInteger>(1024, pso.maxTotalThreadsPerThreadgroup);
        id<MTLCommandQueue> queue = [dev newCommandQueue];

        printf("%s: %u GPU cores? (device %s), %zu MB in %u blocks, %u groups per block, %lu threads per group, tables %u/%u\n",
               "gpumatch", 0, dev.name.UTF8String, total >> 20, nblocks, groups, (unsigned long)tgsize, hlogL, hlogS);

        double bestGpu = 1e9, bestWall = 1e9;
        for (uint32_t r = 0; r < repeats; r++) {
            double t0 = now();
            id<MTLCommandBuffer> cb = [queue commandBuffer];
            id<MTLBlitCommandEncoder> blit = [cb blitCommandEncoder];
            [blit fillBuffer:tables range:NSMakeRange(0, tableWords * 4) value:0xFF];
            [blit endEncoding];
            id<MTLComputeCommandEncoder> enc = [cb computeCommandEncoder];
            [enc setComputePipelineState:pso];
            [enc setBuffer:data offset:0 atIndex:0];
            [enc setBuffer:blocks offset:0 atIndex:1];
            [enc setBuffer:tables offset:0 atIndex:2];
            [enc setBuffer:out offset:0 atIndex:3];
            [enc setBytes:&P length:sizeof P atIndex:4];
            [enc dispatchThreadgroups:MTLSizeMake(nblocks * groups, 1, 1) threadsPerThreadgroup:MTLSizeMake(tgsize, 1, 1)];
            [enc endEncoding];
            [cb commit];
            [cb waitUntilCompleted];
            double wall = now() - t0;
            double gpu = cb.GPUEndTime - cb.GPUStartTime;
            if (cb.status != MTLCommandBufferStatusCompleted) fail("GPU command failed");
            bestGpu = std::min(bestGpu, gpu); bestWall = std::min(bestWall, wall);
            printf("  GPU pass %u: kernel %.1f ms, wall %.1f ms\n", r + 1, gpu * 1e3, wall * 1e3);
        }

        // CPU: parse + zstd frame per block, in parallel over blocks; zstd -3 reference likewise.
        const uint32_t* m = (const uint32_t*)out.contents;
        struct Res { size_t gpuSize = 0, refSize = 0, seqs = 0, matched = 0, bad = 0; double parse = 0, seqComp = 0, ref = 0; bool ok = true; };
        std::vector<Res> res(nblocks);
        auto run = [&](bool parallel) {
            auto body = [&](size_t bi) {
                Res& R = res[bi];
                const uint32_t bs = blockPairs[2 * bi], be = blockPairs[2 * bi + 1];
                const size_t n = be - bs;
                double t0 = now();
                std::vector<ZSTD_Sequence> seqs;
                seqs.reserve(n / 8 + 16);
                uint32_t p = bs, anchor = bs; size_t matched = 0;
                while (p < be) {
                    uint32_t v = m[p];
                    uint32_t len = v >> 23;
                    if (len >= 4) {
                        if (lazy && p + 1 < be) {
                            uint32_t l2 = m[p + 1] >> 23;
                            if (l2 >= len + 2) { p++; continue; }
                        }
                        ZSTD_Sequence s; s.offset = v & 0x7FFFFF; s.litLength = p - anchor; s.matchLength = len; s.rep = 0;
                        if (s.offset == 0 || s.offset > p - bs || p + len > be || memcmp(dp + p - s.offset, dp + p, len) != 0) {
                            if (R.bad < 3) fprintf(stderr, "block %zu: bad match at %u: off %u len %u\n", bi, p - bs, s.offset, len);
                            R.bad++;
                            p++;
                            continue;
                        }
                        seqs.push_back(s);
                        matched += len;
                        p += len; anchor = p;
                    } else p++;
                }
                // Trailing literals are implied (no block delimiters): a final sequence
                // with matchLength 0 reads as a delimiter there and fails validation.
                double t1 = now();
                ZSTD_CCtx* c = ZSTD_createCCtx();
                ZSTD_CCtx_setParameter(c, ZSTD_c_compressionLevel, 3);
                ZSTD_CCtx_setParameter(c, ZSTD_c_windowLog, 23);
                ZSTD_CCtx_setParameter(c, ZSTD_c_minMatch, 4);
                ZSTD_CCtx_setParameter(c, ZSTD_c_blockDelimiters, ZSTD_sf_noBlockDelimiters);
                ZSTD_CCtx_setParameter(c, ZSTD_c_validateSequences, validate ? 1 : 0);
                std::vector<uint8_t> dst(ZSTD_compressBound(n));
                size_t got = ZSTD_compressSequences(c, dst.data(), dst.size(), seqs.data(), seqs.size(), dp + bs, n);
                double t2 = now();
                if (ZSTD_isError(got)) {
                    fprintf(stderr, "block %zu: compressSequences: %s\n", bi, ZSTD_getErrorName(got));
                    // Bisect: the fewest leading sequences (and the prefix they cover) that fail.
                    size_t lo = 1, hi = seqs.size();
                    auto tryK = [&](size_t k) {
                        size_t pre = 0; for (size_t i = 0; i < k; i++) pre += seqs[i].litLength + seqs[i].matchLength;
                        ZSTD_CCtx_reset(c, ZSTD_reset_session_only);
                        size_t g = ZSTD_compressSequences(c, dst.data(), dst.size(), seqs.data(), k, dp + bs, pre);
                        return !ZSTD_isError(g);
                    };
                    while (lo < hi) { size_t mid = (lo + hi) / 2; if (tryK(mid)) lo = mid + 1; else hi = mid; }
                    size_t pos = 0; for (size_t i = 0; i + 1 < lo; i++) pos += seqs[i].litLength + seqs[i].matchLength;
                    fprintf(stderr, "  first failing with %zu sequences; sequence %zu starts at %zu in the block (block size %zu):\n", lo, lo - 1, pos, n);
                    for (size_t i = (lo >= 3 ? lo - 3 : 0); i < std::min(lo + 1, seqs.size()); i++)
                        fprintf(stderr, "    seq %zu: lit %u match %u off %u\n", i, seqs[i].litLength, seqs[i].matchLength, seqs[i].offset);
                    R.ok = false; ZSTD_freeCCtx(c); return;
                }
                std::vector<uint8_t> back(n);
                size_t dec = ZSTD_decompress(back.data(), n, dst.data(), got);
                if (ZSTD_isError(dec) || dec != n || memcmp(back.data(), dp + bs, n) != 0) { fprintf(stderr, "block %zu: round trip failed\n", bi); R.ok = false; }
                double t3 = now();
                ZSTD_CCtx_reset(c, ZSTD_reset_session_and_parameters);
                ZSTD_CCtx_setParameter(c, ZSTD_c_compressionLevel, 3);
                ZSTD_CCtx_setParameter(c, ZSTD_c_windowLog, 23);
                size_t ref = ZSTD_compress2(c, dst.data(), dst.size(), dp + bs, n);
                double t4 = now();
                ZSTD_freeCCtx(c);
                R.gpuSize = got; R.refSize = ref; R.seqs = seqs.size(); R.matched = matched;
                R.parse = t1 - t0; R.seqComp = t2 - t1; R.ref = t4 - t3;
            };
            if (parallel) dispatch_apply(nblocks, dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^(size_t bi) { body(bi); });
            else for (size_t bi = 0; bi < nblocks; bi++) body(bi);
        };
        double tw0 = now(); run(true); double parWall = now() - tw0;
        // CPU seconds: sum over blocks from the parallel run (per-block clocks are wall per block, close to CPU for one thread each)
        size_t gpuTotal = 0, refTotal = 0, seqTotal = 0, matchedTotal = 0, badTotal = 0; double parse = 0, seqComp = 0, ref = 0; bool ok = true;
        for (auto& R : res) badTotal += R.bad;
        printf("\nbad GPU matches skipped: %zu\n", badTotal);
        printf("\n%-14s %10s %12s %12s %7s %8s\n", "file", "bytes", "gpu+zstd", "zstd -3", "diff", "matched");
        for (auto& fr : fileRanges) {
            size_t g = 0, rf = 0, bytes = fr.second.second - fr.second.first, mt = 0;
            for (size_t bi = 0; bi < nblocks; bi++) {
                if (blockPairs[2 * bi] >= fr.second.first && blockPairs[2 * bi] < fr.second.second) { g += res[bi].gpuSize; rf += res[bi].refSize; mt += res[bi].matched; }
            }
            std::string name = fr.first.substr(fr.first.find_last_of('/') + 1);
            printf("%-14s %10zu %12zu %12zu %+6.2f%% %7.1f%%\n", name.c_str(), bytes, g, rf, 100.0 * g / rf - 100.0, 100.0 * mt / bytes);
        }
        for (auto& R : res) { gpuTotal += R.gpuSize; refTotal += R.refSize; seqTotal += R.seqs; matchedTotal += R.matched; parse += R.parse; seqComp += R.seqComp; ref += R.ref; ok &= R.ok; }
        printf("%-14s %10zu %12zu %12zu %+6.2f%% %7.1f%%\n", "total", total, gpuTotal, refTotal, 100.0 * gpuTotal / refTotal - 100.0, 100.0 * matchedTotal / total);
        printf("\nround trips %s; %zu sequences (%.1f bytes each)\n", ok ? "identical" : "FAILED", seqTotal, (double)total / seqTotal);
        printf("GPU matching: kernel %.1f ms (best of %u), wall %.1f ms\n", bestGpu * 1e3, repeats, bestWall * 1e3);
        printf("CPU, summed over blocks: parse %.0f ms, compressSequences %.0f ms, zstd -3 %.0f ms\n", parse * 1e3, seqComp * 1e3, ref * 1e3);
        printf("CPU wall, blocks in parallel (parse + compressSequences + check + zstd -3): %.0f ms\n", parWall * 1e3);
    }
    return 0;
}
