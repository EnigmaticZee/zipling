// C API of the Arca archive engine (Rust, engine/arca-ffi).
//
// Every operation returns a NUL-terminated JSON string, either {"ok": ...} or
// {"error": "..."}. Release it with arca_string_free. Long operations accept an
// optional ArcaTask (for cancellation) and an optional progress callback, which
// may be called from any thread.

#ifndef ARCA_H
#define ARCA_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ArcaTask ArcaTask;
typedef void (*ArcaProgress)(void *ctx, uint64_t done, uint64_t total);

const char *arca_version(void);

ArcaTask *arca_task_new(void);
void arca_task_cancel(ArcaTask *task);
void arca_task_free(ArcaTask *task);

void arca_string_free(char *s);

// {"ok": {archive_bytes, created, files, dirs, total_size, blocks,
//         entries: [{path, kind, size, mtime, mode, transform, codec, packed, target?}]}}
char *arca_list(const char *archive_path);

// request: {"inputs": [paths], "output": path, "level": "fast|balanced|max|ultra"}
char *arca_pack(const char *request, ArcaTask *task, ArcaProgress progress, void *ctx);

// request: {"archive": path, "dest": dir, "only": [paths], "overwrite": bool}
char *arca_extract(const char *request, ArcaTask *task, ArcaProgress progress, void *ctx);

char *arca_verify(const char *archive_path, ArcaTask *task, ArcaProgress progress, void *ctx);

#ifdef __cplusplus
}
#endif

#endif
