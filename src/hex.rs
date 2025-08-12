pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn hex_to_bytes(s: &str) -> Result<Vec<u8>, String> {
    let cleaned = s.split_whitespace().collect::<Vec<_>>().join("");
    if cleaned.len() % 2 != 0 {
        return Err("Odd length".into());
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}
