//! Original MIT. Read-only numeric parser provenance diagnostic.
fn main() {
    let text = "1096.9380545110537";
    let direct: f64 = text.parse().unwrap();
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let parsed = value.as_f64().unwrap();
    println!("lexeme={text} direct={direct:.17} direct_bits={:016x} json={parsed:.17} json_bits={:016x}", direct.to_bits(), parsed.to_bits());
}
