use std::path::Path;
use std::{env, fs};
fn main() {
    let bins = ["simple_udt", "trampoline-nft"];
    let out_dir = env::var_os("OUT_DIR").unwrap();
    bins.into_iter().for_each(|bin| {
        let dest_path = Path::new(&out_dir).join(bin);
        let dest_bytes = fs::read(format!("./binaries/{}", bin)).unwrap();
        assert!(!dest_bytes.is_empty());
        fs::write(&dest_path, dest_bytes)
            .unwrap_or_else(|_| panic!("Unable to write {} to output during build", bin));
    });
    println!("cargo:rerun-if-changed=contract/builtins/*");
}
