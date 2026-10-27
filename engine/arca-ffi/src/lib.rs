//! C API over `arca-core` for the macOS app (see `include/arca.h`).
//!
//! Every operation returns a NUL-terminated JSON string, either `{"ok": ...}` or
//! `{"error": "..."}`, which the caller releases with `arca_string_free`.
//! Panics never cross the boundary: they come back as errors.

use anyhow::Context;
use arca_core::{pack, Archive, EntryKind, ExtractOptions, Hooks, Level, PackOptions};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cancellation token shared between the caller and one running operation.
pub struct ArcaTask {
    cancel: Arc<AtomicBool>,
}

pub type ArcaProgress = Option<extern "C" fn(ctx: *mut c_void, done: u64, total: u64)>;

/// The caller promises `ctx` may be used from any thread for the call's duration.
struct Ctx(*mut c_void);
unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

impl Ctx {
    // A method, so closures capture the whole `Ctx` (Send + Sync), not the raw field.
    fn ptr(&self) -> *mut c_void {
        self.0
    }
}

fn hooks(task: *mut ArcaTask, progress: ArcaProgress, ctx: *mut c_void) -> Hooks {
    // SAFETY: a non-null task comes from `arca_task_new` and outlives the call.
    let cancel = unsafe { task.as_ref() }.map(|t| t.cancel.clone());
    let progress = progress.map(|cb| {
        let ctx = Ctx(ctx);
        Arc::new(move |done: u64, total: u64| cb(ctx.ptr(), done, total)) as arca_core::ProgressFn
    });
    Hooks { progress, cancel }
}

fn respond<T: Serialize>(f: impl FnOnce() -> anyhow::Result<T>) -> *mut c_char {
    let body = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => serde_json::json!({ "ok": v }),
        Ok(Err(e)) => serde_json::json!({ "error": format!("{e:#}") }),
        Err(_) => serde_json::json!({ "error": "internal error in the archive engine" }),
    };
    CString::new(body.to_string()).unwrap_or_default().into_raw()
}

/// # Safety
/// `s` must be null or a valid NUL-terminated string.
unsafe fn arg(s: *const c_char) -> anyhow::Result<String> {
    anyhow::ensure!(!s.is_null(), "missing argument");
    Ok(CStr::from_ptr(s).to_str().context("argument is not UTF-8")?.to_owned())
}

#[no_mangle]
pub extern "C" fn arca_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn arca_task_new() -> *mut ArcaTask {
    Box::into_raw(Box::new(ArcaTask { cancel: Arc::new(AtomicBool::new(false)) }))
}

/// # Safety
/// `task` must be null or come from `arca_task_new` and not yet be freed.
#[no_mangle]
pub unsafe extern "C" fn arca_task_cancel(task: *mut ArcaTask) {
    if let Some(t) = task.as_ref() {
        t.cancel.store(true, Ordering::Relaxed);
    }
}

/// # Safety
/// `task` must be null or come from `arca_task_new`; no call may still be using it.
#[no_mangle]
pub unsafe extern "C" fn arca_task_free(task: *mut ArcaTask) {
    if !task.is_null() {
        drop(Box::from_raw(task));
    }
}

/// # Safety
/// `s` must be null or a string returned by this library, freed only once.
#[no_mangle]
pub unsafe extern "C" fn arca_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[derive(Serialize)]
struct Listing {
    archive_bytes: u64,
    created: i64,
    files: u64,
    dirs: u64,
    total_size: u64,
    blocks: usize,
    entries: Vec<ListedEntry>,
}

#[derive(Serialize)]
struct ListedEntry {
    path: String,
    kind: &'static str,
    size: u64,
    mtime: i64,
    mode: u32,
    /// Reversible transform applied to the content (none, lepton, deflate, arm64).
    transform: &'static str,
    /// Codec of the block holding the content (zstd, xz, ppmd, store).
    codec: &'static str,
    /// Approximate compressed size.
    packed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

/// Lists an archive's entries; reads only its index, so it's instant at any size.
///
/// # Safety
/// `archive_path` must be a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn arca_list(archive_path: *const c_char) -> *mut c_char {
    respond(|| {
        let a = Archive::open(&PathBuf::from(arg(archive_path)?))?;
        let idx = &a.index;
        let packed = idx.packed_estimates();
        let (mut files, mut dirs, mut total) = (0, 0, 0);
        let entries = idx
            .entries
            .iter()
            .map(|e| {
                let mut out = ListedEntry {
                    path: e.path.clone(),
                    kind: "file",
                    size: 0,
                    mtime: e.mtime.0,
                    mode: e.mode,
                    transform: "none",
                    codec: "store",
                    packed: 0,
                    target: None,
                };
                match &e.kind {
                    EntryKind::File { size, object } => {
                        files += 1;
                        total += size;
                        let o = &idx.objects[*object as usize];
                        out.size = *size;
                        out.transform = o.transform.name();
                        out.packed = packed[*object as usize];
                        if let Some(&c) = o.chunks.first() {
                            out.codec = idx.blocks[idx.chunks[c as usize].block as usize].codec.name();
                        }
                    }
                    EntryKind::Dir => {
                        dirs += 1;
                        out.kind = "dir";
                    }
                    EntryKind::Symlink { target } => {
                        out.kind = "symlink";
                        out.target = Some(String::from_utf8_lossy(target).into_owned());
                    }
                    EntryKind::HardLink { target } => {
                        out.kind = "hardlink";
                        out.target = Some(idx.entries[*target as usize].path.clone());
                    }
                }
                out
            })
            .collect();
        Ok(Listing {
            archive_bytes: a.archive_bytes,
            created: idx.created,
            files,
            dirs,
            total_size: total,
            blocks: idx.blocks.len(),
            entries,
        })
    })
}

#[derive(Deserialize)]
struct PackRequest {
    inputs: Vec<PathBuf>,
    output: PathBuf,
    level: String,
    #[serde(default)]
    threads: usize,
}

#[derive(Serialize)]
struct PackResult {
    files: u64,
    dirs: u64,
    input_bytes: u64,
    output_bytes: u64,
    dup_file_bytes: u64,
    dup_chunk_bytes: u64,
    seconds: f64,
    /// (transform, files, bytes in, bytes out)
    transforms: Vec<(String, u64, u64, u64)>,
}

/// Packs files and folders into a new archive.
///
/// # Safety
/// `request` must be a valid NUL-terminated JSON string; `task` null or live.
#[no_mangle]
pub unsafe extern "C" fn arca_pack(
    request: *const c_char,
    task: *mut ArcaTask,
    progress: ArcaProgress,
    ctx: *mut c_void,
) -> *mut c_char {
    let hooks = hooks(task, progress, ctx);
    respond(|| {
        let req: PackRequest = serde_json::from_str(&arg(request)?).context("bad pack request")?;
        let level = match req.level.as_str() {
            "fast" => Level::Fast,
            "balanced" => Level::Balanced,
            "max" => Level::Max,
            "ultra" => Level::Ultra,
            other => anyhow::bail!("unknown level {other}"),
        };
        let mut opts = PackOptions::preset(level);
        if req.threads > 0 {
            opts.threads = req.threads;
        }
        opts.hooks = hooks;
        let s = pack(&req.inputs, &req.output, &opts)?;
        Ok(PackResult {
            files: s.files,
            dirs: s.dirs,
            input_bytes: s.input_bytes,
            output_bytes: s.output_bytes,
            dup_file_bytes: s.dup_file_bytes,
            dup_chunk_bytes: s.dup_chunk_bytes,
            seconds: s.seconds,
            transforms: s.transforms.iter().map(|(n, t)| (n.to_string(), t.files, t.input, t.stored)).collect(),
        })
    })
}

#[derive(Deserialize)]
struct ExtractRequest {
    archive: PathBuf,
    dest: PathBuf,
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Serialize)]
struct ExtractResult {
    files: u64,
    bytes: u64,
    seconds: f64,
}

/// Extracts an archive (or only some paths) into `dest`, verifying everything.
///
/// # Safety
/// `request` must be a valid NUL-terminated JSON string; `task` null or live.
#[no_mangle]
pub unsafe extern "C" fn arca_extract(
    request: *const c_char,
    task: *mut ArcaTask,
    progress: ArcaProgress,
    ctx: *mut c_void,
) -> *mut c_char {
    let hooks = hooks(task, progress, ctx);
    respond(|| {
        let req: ExtractRequest = serde_json::from_str(&arg(request)?).context("bad extract request")?;
        let a = Archive::open(&req.archive)?;
        let opts = ExtractOptions { overwrite: req.overwrite, only: req.only, hooks, ..Default::default() };
        let s = a.extract(&req.dest, &opts)?;
        Ok(ExtractResult { files: s.files, bytes: s.bytes, seconds: s.seconds })
    })
}

/// Decodes the whole archive and checks every hash without writing anything.
///
/// # Safety
/// `archive_path` must be a valid NUL-terminated string; `task` null or live.
#[no_mangle]
pub unsafe extern "C" fn arca_verify(
    archive_path: *const c_char,
    task: *mut ArcaTask,
    progress: ArcaProgress,
    ctx: *mut c_void,
) -> *mut c_char {
    let hooks = hooks(task, progress, ctx);
    respond(|| {
        let a = Archive::open(&PathBuf::from(arg(archive_path)?))?;
        let s = a.verify_with(&ExtractOptions { hooks, ..Default::default() })?;
        Ok(ExtractResult { files: s.files, bytes: s.bytes, seconds: s.seconds })
    })
}
