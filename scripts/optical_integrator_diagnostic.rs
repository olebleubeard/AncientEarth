//! Run diagnostic solvers against independent high-precision fixture states.
mod optical_step_diagnostic;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Pass the independent fixture JSON")?;
    let reference: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let results = reference["cases"]
        .as_array()
        .ok_or("cases")?
        .iter()
        .map(|case| {
            if case.get("trial_path_m").is_some() {
                optical_step_diagnostic::analyze_dense_reference(case)
            } else {
                optical_step_diagnostic::analyze_reference(case)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"reference":reference["method"],"cases":results})
        )?
    );
    Ok(())
}
