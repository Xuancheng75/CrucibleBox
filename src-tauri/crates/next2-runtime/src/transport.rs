use cruciblebox_next2_protocol::{validate_backend_frame, MAX_FRAME_BYTES};
use serde_json::Value;
use std::io::{self, Read, Write};

pub fn read_frame(reader: &mut impl Read) -> Result<Option<Value>, String> {
    let mut header = [0u8; 4];
    if reader.read(&mut header[..1]).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|e| e.to_string())?;
    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err("BUDGET_EXCEEDED".into());
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    let raw = std::str::from_utf8(&body).map_err(|e| e.to_string())?;
    let frame = validate_backend_frame(raw)?;
    serde_json::to_value(frame)
        .map(Some)
        .map_err(|e| e.to_string())
}
pub fn write_frame(writer: &mut impl Write, frame: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(frame)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BUDGET_EXCEEDED",
        ));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}
