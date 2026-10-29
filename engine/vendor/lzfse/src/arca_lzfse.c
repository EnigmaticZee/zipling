/* Arca's entry points: lzfse_encode_buffer_with_scratch (lzfse_encode.c) with
 * the encoder's literal_runs setting, and an exact-size decode. Buffers of 4 GB
 * and up are refused rather than encoded (lzfse switches algorithm there). */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "lzfse.h"
#include "lzfse_internal.h"

size_t arca_lzfse_encode(uint8_t *dst, size_t dst_size, const uint8_t *src, size_t src_size,
                         uint32_t literal_runs) {
  if (src_size >= 0xffffffffU) return 0;
  void *scratch = malloc(lzfse_encode_scratch_size() + 1);
  if (!scratch) return 0;
  size_t out = 0;
  if (src_size < LZVN_ENCODE_MIN_SRC_SIZE) goto try_uncompressed;
  if (src_size < LZFSE_ENCODE_LZVN_THRESHOLD) {
    size_t extra_size = 4 + sizeof(lzvn_compressed_block_header);
    if (dst_size <= extra_size) goto try_uncompressed;
    size_t sz = lzvn_encode_buffer(dst + sizeof(lzvn_compressed_block_header), dst_size - extra_size, src,
                                   src_size, scratch);
    if (sz == 0 || sz >= src_size) goto try_uncompressed;
    lzvn_compressed_block_header header;
    header.magic = LZFSE_COMPRESSEDLZVN_BLOCK_MAGIC;
    header.n_raw_bytes = (uint32_t)src_size;
    header.n_payload_bytes = (uint32_t)sz;
    memcpy(dst, &header, sizeof(header));
    store4(dst + sizeof(lzvn_compressed_block_header) + sz, LZFSE_ENDOFSTREAM_BLOCK_MAGIC);
    out = sz + extra_size;
    goto done;
  }
  {
    lzfse_encoder_state *state = scratch;
    memset(state, 0x00, sizeof *state);
    if (lzfse_encode_init(state) != LZFSE_STATUS_OK) goto try_uncompressed;
    state->literal_runs = literal_runs;
    state->dst = dst;
    state->dst_begin = dst;
    state->dst_end = &dst[dst_size];
    state->src = src;
    state->src_encode_i = 0;
    state->src_end = (lzfse_offset)src_size;
    if (lzfse_encode_base(state) != LZFSE_STATUS_OK) goto try_uncompressed;
    if (lzfse_encode_finish(state) != LZFSE_STATUS_OK) goto try_uncompressed;
    out = state->dst - dst;
    goto done;
  }
try_uncompressed:
  if (src_size + 12 <= dst_size && src_size < INT32_MAX) {
    uncompressed_block_header header = {.magic = LZFSE_UNCOMPRESSED_BLOCK_MAGIC, .n_raw_bytes = (uint32_t)src_size};
    memcpy(dst, &header, sizeof header);
    memcpy(dst + sizeof header, src, src_size);
    store4(dst + sizeof header + src_size, LZFSE_ENDOFSTREAM_BLOCK_MAGIC);
    out = sizeof header + src_size + 4;
  }
done:
  free(scratch);
  return out;
}

/* Decodes a whole stream into exactly dst_size bytes; returns 1 on success. */
int arca_lzfse_decode_exact(uint8_t *dst, size_t dst_size, const uint8_t *src, size_t src_size) {
  /* One spare byte tells a too-long stream apart from an exact one. */
  uint8_t *buf = malloc(dst_size + 1);
  if (!buf) return 0;
  size_t n = lzfse_decode_buffer(buf, dst_size + 1, src, src_size, NULL);
  int ok = n == dst_size;
  if (ok) memcpy(dst, buf, dst_size);
  free(buf);
  return ok;
}
