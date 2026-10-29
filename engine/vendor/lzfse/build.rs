//! Compiles lzfse 1.0 (with Arca's literal_runs setting) and Arca's entry points,
//! with prefixed symbols.

fn main() {
    let mut c = cc::Build::new();
    c.opt_level(3).warnings(false).include("lzfse").flag("-include").flag("src/prefix.h");
    for f in ["lzfse_decode", "lzfse_decode_base", "lzfse_encode", "lzfse_encode_base", "lzfse_fse", "lzvn_decode_base", "lzvn_encode_base"] {
        c.file(format!("lzfse/{f}.c"));
    }
    c.file("src/arca_lzfse.c");
    c.compile("lzfsepinned");
    println!("cargo:rerun-if-changed=lzfse");
    println!("cargo:rerun-if-changed=src/arca_lzfse.c");
    println!("cargo:rerun-if-changed=src/prefix.h");
}
