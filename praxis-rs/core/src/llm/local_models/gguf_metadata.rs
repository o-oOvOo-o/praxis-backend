use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::{self};
use std::path::Path;

const GGUF_MAGIC: [u8; 4] = *b"GGUF";
const GGUF_VERSION_MIN: u32 = 2;
const GGUF_VERSION_MAX: u32 = 3;
const GENERAL_ARCHITECTURE_KEY: &str = "general.architecture";
const MAX_METADATA_ENTRIES: u64 = 4_096;
const MAX_INSPECTION_BYTES: u64 = 4 * 1024 * 1024;
const MAX_KEY_BYTES: u64 = 16 * 1024;
const MAX_ARCHITECTURE_BYTES: u64 = 4 * 1024;
const MAX_ARRAY_DEPTH: u8 = 4;
const MAX_VARIABLE_ARRAY_ELEMENTS: u64 = 4_096;

const GGUF_TYPE_UINT8: u32 = 0;
const GGUF_TYPE_INT8: u32 = 1;
const GGUF_TYPE_UINT16: u32 = 2;
const GGUF_TYPE_INT16: u32 = 3;
const GGUF_TYPE_UINT32: u32 = 4;
const GGUF_TYPE_INT32: u32 = 5;
const GGUF_TYPE_FLOAT32: u32 = 6;
const GGUF_TYPE_BOOL: u32 = 7;
const GGUF_TYPE_STRING: u32 = 8;
const GGUF_TYPE_ARRAY: u32 = 9;
const GGUF_TYPE_UINT64: u32 = 10;
const GGUF_TYPE_INT64: u32 = 11;
const GGUF_TYPE_FLOAT64: u32 = 12;

pub(super) fn read_general_architecture(path: &Path) -> io::Result<Option<String>> {
    read_general_architecture_from(&mut fs::File::open(path)?)
}

fn read_general_architecture_from(reader: &mut (impl Read + Seek)) -> io::Result<Option<String>> {
    let origin = reader.stream_position()?;
    let mut magic = [0_u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != GGUF_MAGIC {
        return Ok(None);
    }
    let version = read_u32(reader)?;
    if !(GGUF_VERSION_MIN..=GGUF_VERSION_MAX).contains(&version) {
        return Ok(None);
    }
    let _tensor_count = read_u64(reader)?;
    let metadata_count = read_u64(reader)?;
    if metadata_count > MAX_METADATA_ENTRIES {
        return Ok(None);
    }

    for _ in 0..metadata_count {
        if inspection_limit_reached(reader, origin)? {
            return Ok(None);
        }
        let key = read_string(reader, MAX_KEY_BYTES)?;
        let value_type = read_u32(reader)?;
        if key == GENERAL_ARCHITECTURE_KEY {
            if value_type != GGUF_TYPE_STRING {
                return Ok(None);
            }
            return read_string(reader, MAX_ARCHITECTURE_BYTES).map(Some);
        }
        if !skip_value(reader, value_type, origin, 0)? {
            return Ok(None);
        }
    }
    Ok(None)
}

fn skip_value(
    reader: &mut (impl Read + Seek),
    value_type: u32,
    origin: u64,
    depth: u8,
) -> io::Result<bool> {
    if depth > MAX_ARRAY_DEPTH || inspection_limit_reached(reader, origin)? {
        return Ok(false);
    }
    let fixed_size = match value_type {
        GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => Some(1_u64),
        GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => Some(2),
        GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => Some(4),
        GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => Some(8),
        GGUF_TYPE_STRING => {
            let byte_count = read_u64(reader)?;
            return skip_bounded(reader, byte_count, origin);
        }
        GGUF_TYPE_ARRAY => {
            let element_type = read_u32(reader)?;
            let element_count = read_u64(reader)?;
            if let Some(element_size) = fixed_value_size(element_type) {
                let Some(byte_count) = element_count.checked_mul(element_size) else {
                    return Ok(false);
                };
                return skip_bounded(reader, byte_count, origin);
            }
            if element_count > MAX_VARIABLE_ARRAY_ELEMENTS {
                return Ok(false);
            }
            for _ in 0..element_count {
                if !skip_value(reader, element_type, origin, depth + 1)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        _ => return Ok(false),
    };
    skip_bounded(
        reader,
        fixed_size.expect("fixed GGUF type has a size"),
        origin,
    )
}

const fn fixed_value_size(value_type: u32) -> Option<u64> {
    match value_type {
        GGUF_TYPE_UINT8 | GGUF_TYPE_INT8 | GGUF_TYPE_BOOL => Some(1),
        GGUF_TYPE_UINT16 | GGUF_TYPE_INT16 => Some(2),
        GGUF_TYPE_UINT32 | GGUF_TYPE_INT32 | GGUF_TYPE_FLOAT32 => Some(4),
        GGUF_TYPE_UINT64 | GGUF_TYPE_INT64 | GGUF_TYPE_FLOAT64 => Some(8),
        _ => None,
    }
}

fn read_string(reader: &mut impl Read, maximum_bytes: u64) -> io::Result<String> {
    let byte_count = read_u64(reader)?;
    if byte_count > maximum_bytes || byte_count > usize::MAX as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GGUF string exceeds the bounded metadata probe",
        ));
    }
    let mut bytes = vec![0_u8; byte_count as usize];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "GGUF string is not UTF-8"))
}

fn skip_bounded(reader: &mut impl Seek, byte_count: u64, origin: u64) -> io::Result<bool> {
    let position = reader.stream_position()?;
    let Some(target) = position.checked_add(byte_count) else {
        return Ok(false);
    };
    let Some(consumed) = target.checked_sub(origin) else {
        return Ok(false);
    };
    if consumed > MAX_INSPECTION_BYTES || byte_count > i64::MAX as u64 {
        return Ok(false);
    }
    reader.seek(SeekFrom::Current(byte_count as i64))?;
    Ok(true)
}

fn inspection_limit_reached(reader: &mut impl Seek, origin: u64) -> io::Result<bool> {
    Ok(reader.stream_position()?.saturating_sub(origin) >= MAX_INSPECTION_BYTES)
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn push_string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn header(metadata_count: u64) -> Vec<u8> {
        let mut bytes = GGUF_MAGIC.to_vec();
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&metadata_count.to_le_bytes());
        bytes
    }

    #[test]
    fn architecture_probe_skips_prior_metadata_without_parsing_the_model() {
        let mut bytes = header(2);
        push_string(&mut bytes, "general.name");
        bytes.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
        push_string(&mut bytes, "local-model");
        push_string(&mut bytes, GENERAL_ARCHITECTURE_KEY);
        bytes.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
        push_string(&mut bytes, "qwen3");
        assert_eq!(
            read_general_architecture_from(&mut Cursor::new(bytes)).unwrap(),
            Some("qwen3".to_string())
        );
    }

    #[test]
    fn architecture_probe_returns_before_truncated_large_metadata_tail() {
        let mut bytes = header(2);
        push_string(&mut bytes, GENERAL_ARCHITECTURE_KEY);
        bytes.extend_from_slice(&GGUF_TYPE_STRING.to_le_bytes());
        push_string(&mut bytes, "llama");
        assert_eq!(
            read_general_architecture_from(&mut Cursor::new(bytes)).unwrap(),
            Some("llama".to_string())
        );
    }
}
