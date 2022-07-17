use anyhow::anyhow;
use anyhow::Result;

// Required by parse_hex to check if the result is valid.
pub fn hex_decode(src: &[u8], dst: &mut [u8]) -> Result<()> {
    if src.is_empty() {
        return Err(anyhow!("Invalid length in dst {}", dst.len()));
    }
    let len = dst.len().checked_mul(2).unwrap();
    if src.len() < len || ((src.len() & 1) != 0) {
        return Err(anyhow!(
            "Invalid length in dst {}, expected: {}",
            dst.len(),
            len
        ));
    }
    hex::decode_to_slice(src, dst)?;

    Ok(())
}

// Parses a hex string into a byte array
// Used for creating a lock_arg for a Script
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
