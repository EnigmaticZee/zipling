/* Arca's shim over the LZMA SDK: one call to encode a buffer as a raw LZMA2
   stream, one to decode it. Public domain, like the SDK. */

#include "Precomp.h"
#include "Alloc.h"
#include "Lzma2Dec.h"
#include "Lzma2Enc.h"

/* Encodes src as one solid LZMA2 stream into dst (capacity *dst_len; on return
   the bytes written) and returns its dictionary property byte in *prop.
   threads: 1, or 2 for a match finder running in its own thread (same output). */
int arca_lzma2_encode(const unsigned char *src, size_t src_len, unsigned char *dst, size_t *dst_len,
                      int level, unsigned dict_size, int fb, unsigned mc, int threads, unsigned char *prop)
{
  CLzma2EncProps props;
  SRes res;
  CLzma2EncHandle enc = Lzma2Enc_Create(&g_Alloc, &g_BigAlloc);
  if (!enc)
    return SZ_ERROR_MEM;
  Lzma2EncProps_Init(&props);
  props.lzmaProps.level = level;
  props.lzmaProps.dictSize = dict_size;
  props.lzmaProps.fb = fb;
  props.lzmaProps.mc = mc;
  props.lzmaProps.numThreads = threads;
  props.lzmaProps.reduceSize = src_len;
  props.blockSize = LZMA2_ENC_PROPS_BLOCK_SIZE_SOLID;
  props.numBlockThreads_Max = 1;
  props.numBlockThreads_Reduced = 1;
  props.numTotalThreads = threads;
  res = Lzma2Enc_SetProps(enc, &props);
  if (res == SZ_OK)
  {
    *prop = Lzma2Enc_WriteProperties(enc);
    Lzma2Enc_SetDataSize(enc, src_len);
    res = Lzma2Enc_Encode2(enc, NULL, dst, dst_len, NULL, src, src_len, NULL);
  }
  Lzma2Enc_Destroy(enc);
  return res;
}

/* Decodes exactly dst_len bytes from exactly src_len bytes of LZMA2. */
int arca_lzma2_decode(const unsigned char *src, size_t src_len, unsigned char *dst, size_t dst_len, unsigned char prop)
{
  SizeT dl = dst_len, sl = src_len;
  ELzmaStatus status;
  SRes res = Lzma2Decode(dst, &dl, src, &sl, prop, LZMA_FINISH_END, &status, &g_Alloc);
  if (res != SZ_OK)
    return res;
  if (dl != dst_len || sl != src_len || status != LZMA_STATUS_FINISHED_WITH_MARK)
    return SZ_ERROR_DATA;
  return SZ_OK;
}
