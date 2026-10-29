//! Compiles the vendored libbsc library (no CUDA, no OpenMP: Arca runs blocks in
//! parallel itself).

fn main() {
    let mut cpp = cc::Build::new();
    cpp.cpp(true).std("c++17").opt_level(3).warnings(false).define("NDEBUG", None);
    for f in [
        "adler32/adler32",
        "bwt/bwt",
        "coder/coder",
        "coder/qlfc/qlfc",
        "coder/qlfc/qlfc_model",
        "filters/detectors",
        "filters/preprocessing",
        "libbsc/libbsc",
        "lzp/lzp",
        "platform/platform",
        "st/st",
    ] {
        cpp.file(format!("libbsc/{f}.cpp"));
    }
    cpp.file("src/arca_bsc.cpp");
    cpp.compile("bsc");
    // libsais goes in through Arca's file, which includes it and adds a
    // threaded inverse BWT.
    cc::Build::new().opt_level(3).warnings(false).define("NDEBUG", None).file("src/arca_sais.c").compile("sais");
    println!("cargo:rerun-if-changed=libbsc");
    println!("cargo:rerun-if-changed=src/arca_bsc.cpp");
    println!("cargo:rerun-if-changed=src/arca_sais.c");
}
