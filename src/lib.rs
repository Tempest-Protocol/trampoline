pub mod docker;
pub mod opts;
pub mod project;
pub mod schema;
pub mod network;
mod utils;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use std::path::Path;
use tera::{self, Tera};
pub use utils::*;

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = Tera::default();
        for path in DAPP_FILES.file_names() {
            let name = path
                .strip_prefix("templates/")
                .expect("Failed to remove prefix");
            let content = {
                let file_contents = DAPP_FILES.get(path).expect("read template");
                String::from_utf8(file_contents.to_vec()).expect("template contents")
            };

            tera.add_raw_template(name, &content)
                .expect("failed to add template");
        }
        tera
    };
}

#[allow(clippy::large_enum_variant)]
pub enum TrampolineResourceType {
    Project(project::TrampolineProject),
    Schema(schema::Schema),
}

pub trait TrampolineResource {
    type Error;
    type InitArgs;
    fn load(path: impl AsRef<Path>) -> Result<TrampolineResourceType, Self::Error>;    fn init(args: Self::InitArgs) -> Result<TrampolineResourceType, Self::Error>;
}

pub fn parse_hex(mut input: &str) -> Result<Vec<u8>> {
    if input.starts_with("0x") || input.starts_with("0X") {
        input = &input[2..];
    }
    if input.len() % 2 != 0 {
        return Err(anyhow!("Invalid hex string lenth: {}", input.len()));
    }
    let mut bytes = vec![0u8; input.len() / 2];
    hex_decode(input.as_bytes(), &mut bytes)
        .map_err(|err| anyhow!(format!("parse hex string failed: {:?}", err)))?;
    Ok(bytes)
}
