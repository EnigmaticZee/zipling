//! Arca archive engine.
//!
//! A content-aware archiver: files are sniffed, reversibly transformed into
//! more compressible forms (Lepton for JPEG, preflate for Deflate containers,
//! an ARM64 branch filter for Mach-O), deduplicated at file and chunk level,
//! routed into text/binary/store lanes, and packed into independent solid
//! blocks that compress and decompress on every core.

pub mod classify;
pub mod codec;
pub mod format;
pub mod hooks;
pub mod pack;
pub mod prof;
pub mod scan;
pub mod transform;
pub mod unpack;
mod wire;
pub mod zip;
pub mod zipwrite;

pub use format::{Codec, Entry, EntryKind, Index, Transform};
pub use hooks::{Hooks, ProgressFn};
pub use pack::{pack, Level, PackOptions, PackStats};
pub use unpack::{Archive, ExtractOptions, ExtractStats};
