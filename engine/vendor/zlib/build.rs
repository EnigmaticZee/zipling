//! Compiles zlib 1.3.2's encoder with prefixed symbols, plus Arca's helper.

fn main() {
    let mut c = cc::Build::new();
    c.opt_level(3).warnings(false).define("Z_PREFIX", None).include("zlib");
    for f in ["adler32", "crc32", "deflate", "trees", "zutil"] {
        c.file(format!("zlib/{f}.c"));
    }
    c.file("src/arca_zlib.c");
    c.compile("zpinned");
    println!("cargo:rerun-if-changed=zlib");
    println!("cargo:rerun-if-changed=src/arca_zlib.c");
}
