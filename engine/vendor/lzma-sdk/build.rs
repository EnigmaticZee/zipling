//! Compiles the vendored LZMA SDK files and Arca's shim.

fn main() {
    let arm64 = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64");
    let mut b = cc::Build::new();
    b.include("C").opt_level(2).warnings(false).define("Z7_NO_LONG_PATH", None);
    for f in ["Alloc", "CpuArch", "LzFind", "LzFindMt", "LzFindOpt", "Lzma2Dec", "Lzma2Enc", "LzmaEnc", "MtCoder", "MtDec", "Threads", "7zStream"] {
        b.file(format!("C/{f}.c"));
    }
    b.file("src/arca_lzma.c");
    b.compile("lzmasdk");

    // The decoder's hot loop has a hand-written ARM64 version (7-Zip's own build
    // uses it); LzmaDec.c then calls it instead of its C loop.
    let mut d = cc::Build::new();
    d.include("C").opt_level(2).warnings(false);
    if arm64 {
        d.define("Z7_LZMA_DEC_OPT", None).file("Asm/arm64/LzmaDecOpt.S");
    }
    d.file("C/LzmaDec.c").compile("lzmasdkdec");

    println!("cargo:rerun-if-changed=C");
    println!("cargo:rerun-if-changed=Asm");
    println!("cargo:rerun-if-changed=src/arca_lzma.c");
}
