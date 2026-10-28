use anyhow::Result;
use arca_core::{Archive, EntryKind, ExtractOptions, Level, PackOptions};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(name = "arca", version, about = "Content-aware, deduplicating, parallel archiver")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum LevelArg {
    Fast,
    Balanced,
    Max,
    Ultra,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an archive from files and folders.
    Pack {
        output: PathBuf,
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(short, long, value_enum, default_value_t = LevelArg::Balanced)]
        level: LevelArg,
        #[arg(short = 'j', long)]
        threads: Option<usize>,
        /// Disable format-aware transforms (Lepton, preflate, ARM64 filter).
        #[arg(long)]
        no_transforms: bool,
        #[arg(long)]
        no_dedup: bool,
        /// Override the block codec level (zstd 1-22, xz 0-9).
        #[arg(long)]
        codec_level: Option<i32>,
        /// Override the block size in MiB.
        #[arg(long)]
        block_mib: Option<usize>,
        /// Override the text-lane codec (zstd, xz, ppmd) and its level.
        #[arg(long)]
        text_codec: Option<String>,
        #[arg(long)]
        text_level: Option<i32>,
        /// Override the text-lane block size in MiB.
        #[arg(long)]
        text_block_mib: Option<usize>,
        /// Override preflate's match-search depth (research).
        #[arg(long)]
        deflate_chain: Option<u32>,
        /// Research: set a lane's codec, e.g. `--lane image=zstd:19` or
        /// `--lane binary=xz:9e` (lanes: text, binary, image).
        #[arg(long)]
        lane: Vec<String>,
        /// Print a machine-readable stats line.
        #[arg(long)]
        json: bool,
    },
    /// Extract an archive.
    Unpack {
        archive: PathBuf,
        #[arg(short = 'o', long, default_value = ".")]
        dest: PathBuf,
        #[arg(short = 'j', long)]
        threads: Option<usize>,
        #[arg(short, long)]
        force: bool,
        /// Only these paths (and what lies beneath them).
        only: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// List archive contents.
    List { archive: PathBuf },
    /// Extract a ZIP archive with Arca's own ZIP reader.
    Unzip {
        archive: PathBuf,
        #[arg(short = 'o', long, default_value = ".")]
        dest: PathBuf,
        #[arg(short = 'j', long)]
        threads: Option<usize>,
        #[arg(short, long)]
        force: bool,
        only: Vec<String>,
    },
    /// Check every entry of a ZIP archive with Arca's own ZIP reader.
    Ziptest { archive: PathBuf },
    /// Make a ZIP archive with Arca's own ZIP writer.
    Zip {
        output: PathBuf,
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Deflate level 1-9 (0 stores).
        #[arg(short, long, default_value_t = 6)]
        level: u32,
        #[arg(short = 'j', long)]
        threads: Option<usize>,
        /// Keep .DS_Store, ._ files and other Mac-only files.
        #[arg(long)]
        keep_mac_files: bool,
    },
    /// Decode everything and check every hash, writing nothing.
    Verify {
        archive: PathBuf,
        #[arg(short = 'j', long)]
        threads: Option<usize>,
    },
}

fn human(n: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", units[u])
    }
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Pack { output, inputs, level, threads, no_transforms, no_dedup, codec_level, block_mib, text_codec, text_level, text_block_mib, deflate_chain, lane, json } => {
            let level = match level {
                LevelArg::Fast => Level::Fast,
                LevelArg::Balanced => Level::Balanced,
                LevelArg::Max => Level::Max,
                LevelArg::Ultra => Level::Ultra,
            };
            let mut o = PackOptions::preset(level);
            if let Some(t) = threads {
                o.threads = t;
            }
            if no_transforms {
                o.transforms.jpeg = false;
                o.transforms.deflate = false;
                o.transforms.arm64 = false;
            }
            o.dedup = !no_dedup;
            if let Some(l) = codec_level {
                o.text_codec.level = l;
                o.binary_codec.level = l;
            }
            if let Some(c) = text_codec {
                o.text_codec.codec = match c.as_str() {
                    "zstd" => arca_core::Codec::Zstd,
                    "xz" => arca_core::Codec::Xz,
                    "ppmd" => arca_core::Codec::Ppmd,
                    other => anyhow::bail!("unknown codec {other}"),
                };
            }
            if let Some(l) = text_level {
                o.text_codec.level = l;
            }
            if let Some(b) = block_mib {
                o.block_size = b << 20;
                o.text_block_size = b << 20;
                o.adaptive_below = 0;
            }
            if let Some(b) = text_block_mib {
                o.text_block_size = b << 20;
            }
            if let Some(c) = deflate_chain {
                o.transforms.deflate_chain = c;
            }
            for spec in lane {
                let (name, rest) = spec.split_once('=').ok_or_else(|| anyhow::anyhow!("bad --lane {spec}"))?;
                let (codec, level) = rest.split_once(':').unwrap_or((rest, "0"));
                let extreme = level.ends_with('e');
                let cfg = arca_core::codec::CodecConfig {
                    codec: match codec {
                        "zstd" => arca_core::Codec::Zstd,
                        "xz" => arca_core::Codec::Xz,
                        "lzma2" => arca_core::Codec::Lzma2,
                        "ppmd" => arca_core::Codec::Ppmd,
                        "store" => arca_core::Codec::Store,
                        other => anyhow::bail!("unknown codec {other}"),
                    },
                    level: level.trim_end_matches('e').parse()?,
                    extreme,
                };
                match name {
                    "text" => o.text_codec = cfg,
                    "binary" => o.binary_codec = cfg,
                    "image" => o.image_codec = cfg,
                    other => anyhow::bail!("unknown lane {other}"),
                }
            }
            let s = arca_core::pack(&inputs, &output, &o)?;
            if json {
                println!(
                    "{{\"input\":{},\"output\":{},\"seconds\":{:.3},\"files\":{}}}",
                    s.input_bytes, s.output_bytes, s.seconds, s.files
                );
                return Ok(());
            }
            let ratio = s.output_bytes as f64 / s.input_bytes.max(1) as f64;
            println!("{} files, {} dirs, {} → {} ({:.2}% of original) in {:.2}s ({}/s)",
                s.files, s.dirs, human(s.input_bytes), human(s.output_bytes), ratio * 100.0, s.seconds,
                human((s.input_bytes as f64 / s.seconds.max(1e-9)) as u64));
            println!("  duplicate files skipped: {}   duplicate chunks skipped: {}", human(s.dup_file_bytes), human(s.dup_chunk_bytes));
            for (name, t) in &s.transforms {
                println!("  transform {:<8} {:>6} files  {} → {}", name, t.files, human(t.input), human(t.stored));
            }
            for (name, raw, comp) in &s.lanes {
                if *raw > 0 {
                    println!("  lane {:<7} {} → {}", name, human(*raw), human(*comp));
                }
            }
            println!("  {} blocks, index {}", s.blocks, human(s.index_bytes));
        }
        Cmd::Unpack { archive, dest, threads, force, only, json } => {
            let a = Archive::open(&archive)?;
            let s = a.extract(&dest, &ExtractOptions { threads: threads.unwrap_or(0), overwrite: force, only, ..Default::default() })?;
            if json {
                println!("{{\"bytes\":{},\"seconds\":{:.3},\"files\":{}}}", s.bytes, s.seconds, s.files);
            } else {
                println!("extracted {} files ({}) from {} blocks in {:.2}s ({}/s), all hashes verified",
                    s.files, human(s.bytes), s.blocks, s.seconds, human((s.bytes as f64 / s.seconds.max(1e-9)) as u64));
            }
        }
        Cmd::List { archive } => {
            let a = Archive::open(&archive)?;
            for e in &a.index.entries {
                match &e.kind {
                    EntryKind::File { size, object } => {
                        let o = &a.index.objects[*object as usize];
                        println!("{:>12}  {:<8} {}", size, o.transform.name(), e.path)
                    }
                    EntryKind::Dir => println!("{:>12}  {:<8} {}/", "", "dir", e.path),
                    EntryKind::Symlink { target } => {
                        println!("{:>12}  {:<8} {} -> {}", "", "symlink", e.path, String::from_utf8_lossy(target))
                    }
                    EntryKind::HardLink { target } => {
                        println!("{:>12}  {:<8} {} => {}", "", "hardlink", e.path, a.index.entries[*target as usize].path)
                    }
                }
            }
        }
        Cmd::Unzip { archive, dest, threads, force, only } => {
            let z = arca_core::zip::ZipArchive::open(&archive)?;
            let opts = arca_core::zip::ZipExtractOptions { threads: threads.unwrap_or(0), overwrite: force, only, ..Default::default() };
            let s = z.extract(&dest, &opts)?;
            println!("extracted {} files ({}) in {:.2}s, CRCs checked{}", s.files, human(s.bytes), s.seconds,
                if s.skipped_links.is_empty() { String::new() } else { format!("; {} unsafe links left out", s.skipped_links.len()) });
        }
        Cmd::Zip { output, inputs, level, threads, keep_mac_files } => {
            let opts = arca_core::zipwrite::ZipWriteOptions {
                level,
                threads: threads.unwrap_or(0),
                leave_out_mac_files: !keep_mac_files,
                ..Default::default()
            };
            let s = arca_core::zipwrite::write_zip(&inputs, &output, &opts)?;
            println!("{} files, {} → {} in {:.2}s", s.files, human(s.input_bytes), human(s.output_bytes), s.seconds);
        }
        Cmd::Ziptest { archive } => {
            let t = std::time::Instant::now();
            let z = arca_core::zip::ZipArchive::open(&archive)?;
            let n = z.verify(std::thread::available_parallelism().map_or(4, |n| n.get()))?;
            println!("OK: {} entries, {} checked in {:.2}s", z.entries.len(), human(n), t.elapsed().as_secs_f64());
        }
        Cmd::Verify { archive, threads } => {
            let a = Archive::open(&archive)?;
            let s = a.verify(threads.unwrap_or(0))?;
            println!("OK: {} files, {} verified in {:.2}s", s.files, human(s.bytes), s.seconds);
        }
    }
    Ok(())
}
