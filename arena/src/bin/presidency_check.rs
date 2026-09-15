//! Save engine/control evidence to a new file. No external calls.
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: presidency_check NEW_OUTPUT.json".into());
    }
    // Fail before doing work if the evidence path already exists.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    let mut file = BufWriter::new(file);
    let mut report = arena::presidency::report();
    let sources = [
        ("arena/src/presidency.rs", include_str!("../presidency.rs")),
        ("arena/src/runner.rs", include_str!("../runner.rs")),
        ("arena/src/strategies.rs", include_str!("../strategies.rs")),
        (
            "rules/src/actions.rs",
            include_str!("../../../rules/src/actions.rs"),
        ),
        (
            "rules/src/transitions.rs",
            include_str!("../../../rules/src/transitions.rs"),
        ),
        (
            "rules/src/logic.rs",
            include_str!("../../../rules/src/logic.rs"),
        ),
        (
            "rules/src/constants.rs",
            include_str!("../../../rules/src/constants.rs"),
        ),
        (
            "rules/src/sim.rs",
            include_str!("../../../rules/src/sim.rs"),
        ),
        (
            "rules/src/state.rs",
            include_str!("../../../rules/src/state.rs"),
        ),
    ];
    report["source_sha256"] = sources
        .into_iter()
        .map(|(name, content)| {
            (
                name.to_string(),
                serde_json::json!(format!("{:x}", Sha256::digest(content.as_bytes()))),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.flush()?;
    println!(
        "Saved {} engine cases and {} control windows to {}",
        report["cases"].as_array().unwrap().len(),
        report["controls"].as_array().unwrap().len(),
        args[1]
    );
    Ok(())
}
