//! `apfm-import <world.db> <out-dir> [--verify-determinism]`
//!
//! Reads world.db read-only, builds the APFM world, runs the integrity checks and writes the world
//! file, its sidecars and the report into `<out-dir>`. Refuses to write if a blocking check fails.

use fm24_adapter::{Config, checks, pipeline, report};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verify = args.iter().any(|a| a == "--verify-determinism");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (Some(world_db), Some(out)) = (positional.first(), positional.get(1)) else {
        eprintln!("usage: apfm-import <world.db> <out-dir> [--verify-determinism]");
        std::process::exit(2);
    };
    let (world_db, out) = (PathBuf::from(world_db), PathBuf::from(out));
    if let Err(message) = execute(&world_db, &out, verify) {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn execute(world_db: &std::path::Path, out: &std::path::Path, verify: bool) -> Result<(), String> {
    let cfg = Config::default_config();
    let started = Instant::now();
    let run = pipeline::run(world_db, &out.join("package"), &cfg)?;
    let built = started.elapsed();
    let results = checks::run_checks(&run);
    let failed: Vec<_> = results.iter().filter(|c| !c.ok).collect();
    for c in &results {
        println!("[{}] {}{}: {}", if c.ok { " ok " } else { "FAIL" }, c.name, if c.blocking { "" } else { " (info)" }, c.detail);
    }
    let written = report::write_all(&run, &results, out)?;
    println!(
        "world: {} ({} bytes) sha256 {} — built in {:.1}s, {} check(s) failed",
        written.world_path.display(),
        written.world_bytes,
        written.world_sha256,
        built.as_secs_f64(),
        failed.len()
    );
    if verify {
        let second = pipeline::run(world_db, &out.join("package-second"), &cfg)?;
        let again = report::canonical_world_json(&second.world)?;
        let again_sha = report::sha256_hex(again.as_bytes());
        let _ = std::fs::remove_dir_all(out.join("package-second"));
        if again_sha == written.world_sha256 {
            println!("determinism: two independent runs give the same SHA-256");
        } else {
            return Err(format!("NOT deterministic: {} vs {}", written.world_sha256, again_sha));
        }
    }
    Ok(())
}
