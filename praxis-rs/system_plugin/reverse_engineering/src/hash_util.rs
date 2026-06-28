use sha2::Digest;
use std::io::Read;

pub fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut hasher = sha2::Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    to_hex(&hasher.finalize())
}

pub fn sha256_reader_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

pub fn short_id(prefix: &str, parts: &[&[u8]]) -> String {
    format!("{}_{}", prefix, &sha256_hex(parts)[..16])
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
