//! Research tool: libbsc against the LZMA SDK on dumped blocks.
//! `bscbench BLOCK...` (one thread per encode; settings from BSC_LZP=hash,min and
//! BSC_CODER=static|adaptive|fast). Checks every round trip.
fn cpu() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn main() {
    let mut s = libbsc::DEFAULT;
    if let Ok(v) = std::env::var("BSC_LZP") {
        let (a, b) = v.split_once(',').unwrap();
        (s.lzp_hash, s.lzp_min) = (a.parse().unwrap(), b.parse().unwrap());
    }
    s.coder = match std::env::var("BSC_CODER").as_deref() {
        Ok("adaptive") => libbsc::CODER_QLFC_ADAPTIVE,
        Ok("fast") => libbsc::CODER_QLFC_FAST,
        _ => libbsc::CODER_QLFC_STATIC,
    };
    let lzma = std::env::var_os("BSC_NO_LZMA").is_none();
    for f in std::env::args().skip(1) {
        let d = std::fs::read(&f).unwrap();
        let name = f.rsplit('/').next().unwrap().to_owned();
        let t = cpu();
        let c = libbsc::compress(&d, s).unwrap();
        let tc = cpu() - t;
        let t = cpu();
        let back = libbsc::decompress_threads(&c, d.len(), 1).unwrap();
        let td = cpu() - t;
        assert!(back == d, "bsc round trip failed on {name}");
        let w = std::time::Instant::now();
        let back = libbsc::decompress(&c, d.len()).unwrap();
        let tw = w.elapsed().as_secs_f64();
        assert!(back == d, "threaded bsc decode differs on {name}");
        let mut line = format!(
            "{name}: {:.1} MB | bsc {s:?}: {:.3} MB, {tc:.2} s, decode {td:.2} s (threaded {tw:.2} s wall)",
            d.len() as f64 / 1e6,
            c.len() as f64 / 1e6
        );
        if lzma {
            let ls = lzma_sdk::Settings { level: 9, dict_size: (d.len() as u32).clamp(1 << 16, 1 << 26), fb: 273, mc: 0, threads: 1 };
            let t = cpu();
            let (lc, prop) = lzma_sdk::encode(&d, ls, d.len() + d.len() / 2 + 4096).unwrap();
            let tl = cpu() - t;
            let t = cpu();
            let _ = lzma_sdk::decode(&lc, prop, d.len()).unwrap();
            let tld = cpu() - t;
            line += &format!(" | LZMA fb273: {:.3} MB, {tl:.2} s, decode {tld:.2} s", lc.len() as f64 / 1e6);
        }
        if std::env::var_os("BSC_PPMD").is_some() {
            use arca_core::codec::{compress, decompress, CodecConfig};
            use arca_core::format::Codec;
            let t = cpu();
            let pc = compress(&d, CodecConfig { codec: Codec::Ppmd, level: 16, extreme: false, long: false, workers: 1 }).unwrap();
            let tp = cpu() - t;
            let t = cpu();
            let _ = decompress(&pc, Codec::Ppmd, d.len() as u64).unwrap();
            let tpd = cpu() - t;
            line += &format!(" | PPMd16: {:.3} MB, {tp:.2} s, decode {tpd:.2} s", pc.len() as f64 / 1e6);
        }
        println!("{line}");
    }
}
