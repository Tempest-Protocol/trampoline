use includedir_codegen::Compression;
use std::path::Path;
use std::{env, fs};
fn main() {
    includedir_codegen::start("DAPP_FILES")
        .dir("templates/", Compression::Gzip)
        .build("templates.rs")
        .unwrap();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("simple_udt");

    let sudt_bin = fs::read(Path::new("./builtins/bins/simple_udt")).unwrap();
    assert!(!sudt_bin.is_empty());

    fs::write(&dest_path, sudt_bin).unwrap();
}
