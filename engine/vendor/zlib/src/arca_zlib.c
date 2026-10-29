/* Re-runs zlib's raw Deflate encoder with fixed settings. */
#include <limits.h>
#include <string.h>
#include "deflate.h"

/* Encodes `plain`; with `target` (length `cap`) compares the output as it is made
 * and stops at the first difference, else writes up to `cap` bytes to `out`.
 * `fast` 0 turns off Arca's faster match search (same output; for tests).
 * Returns the output length, or -1 on an error, a difference or overflow. */
static long long run(const unsigned char *plain, size_t n, int level, int wbits, int mem, int strategy,
                     const unsigned char *target, unsigned char *out, size_t cap, int fast) {
    if (n > UINT_MAX) return -1;
    z_stream s;
    memset(&s, 0, sizeof s);
    if (deflateInit2(&s, level, Z_DEFLATED, -wbits, mem, strategy) != Z_OK) return -1;
    if (!fast) ((deflate_state *)s.state)->arca_fast = 0;
    unsigned char buf[16384];
    size_t got = 0;
    long long res = -1;
    s.next_in = (unsigned char *)plain;
    s.avail_in = (uInt)n;
    for (;;) {
        unsigned char *dst = target ? buf : out + got;
        size_t room = target ? sizeof buf : cap - got;
        if (room > UINT_MAX) room = UINT_MAX;
        s.next_out = dst;
        s.avail_out = (uInt)room;
        int r = deflate(&s, Z_FINISH);
        size_t made = room - s.avail_out;
        if (got + made > cap) break;
        if (target && memcmp(buf, target + got, made) != 0) break;
        got += made;
        if (r == Z_STREAM_END) {
            res = (long long)got;
            break;
        }
        if ((r != Z_OK && r != Z_BUF_ERROR) || (!target && got == cap)) break;
    }
    deflateEnd(&s);
    return res;
}

int arca_zlib_matches(const unsigned char *plain, size_t n, int level, int wbits, int mem, int strategy,
                      const unsigned char *target, size_t len, int fast) {
    return run(plain, n, level, wbits, mem, strategy, target, NULL, len, fast) == (long long)len;
}

long long arca_zlib_deflate(const unsigned char *plain, size_t n, int level, int wbits, int mem, int strategy,
                            unsigned char *out, size_t cap, int fast) {
    return run(plain, n, level, wbits, mem, strategy, NULL, out, cap, fast);
}
