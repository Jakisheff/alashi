//! simrun: массовый прогон off-chain партий → JSONL + сводка.
//!
//! Примеры:
//!   cargo run --manifest-path arena/Cargo.toml --bin simrun -- \
//!       --games 100 --mix greedy,random,tactical --out data/sim/games.jsonl
//!   cargo run --manifest-path arena/Cargo.toml --bin simrun -- \
//!       --games 50 --mix greedy,tactical --seed 7

use arena::runner::{run_series, GameConfig};
use arena::strategies::ALL;
use std::io::Write;

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{}=", name)) {
            return Some(v.to_string());
        }
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let games: u64 = flag(&args, "--games")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let seed: u64 = flag(&args, "--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(1)
        });
    let mix: Vec<String> = flag(&args, "--mix")
        .unwrap_or_else(|| "greedy,random,tactical".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let mix_ref: Vec<&str> = mix.iter().map(|s| s.as_str()).collect();
    let out_path = flag(&args, "--out").unwrap_or_else(|| "-".into());
    let entry_fee: u64 = flag(&args, "--entry-fee")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000_000);

    for name in &mix_ref {
        if !ALL.contains(&name) {
            eprintln!("[ERROR] неизвестная стратегия: {} (доступны: {})", name, ALL.join(", "));
            std::process::exit(2);
        }
    }

    let cfg = GameConfig {
        entry_fee,
        phase_duration: 10,
    };
    eprintln!(
        "simrun: {} игр, микс [{}], seed {}, entry_fee {}",
        games,
        mix_ref.join(","),
        seed,
        entry_fee
    );
    let lines = match run_series(games, seed, &mix_ref, &cfg) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[ERROR] {}", e);
            std::process::exit(1);
        }
    };

    if out_path == "-" {
        let mut out = std::io::stdout();
        for l in &lines {
            writeln!(out, "{}", l).ok();
        }
    } else {
        if let Some(dir) = std::path::Path::new(&out_path).parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let mut f = match std::fs::File::create(&out_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[ERROR] не открыть {}: {}", out_path, e);
                std::process::exit(1);
            }
        };
        for l in &lines {
            writeln!(f, "{}", l).ok();
        }
        eprintln!("записано: {} ({} строк)", out_path, lines.len());
    }

    // Сводка: победы/средний ранг/средняя выплата по стратегиям.
    let n = mix_ref.len();
    let mut wins = vec![0u64; n];
    let mut rank_sum = vec![0u64; n];
    let mut payout_sum = vec![0u64; n];
    let mut laws_passed = 0u64;
    let mut vetoes = 0u64;
    let mut bribes = 0u64;
    for l in &lines {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        let ranks = v["ranks"].as_array().unwrap();
        let payouts = v["payouts"].as_array().unwrap();
        let strats = v["strategies"].as_array().unwrap();
        for (rank_pos, fi) in ranks.iter().enumerate() {
            let fi = fi.as_u64().unwrap() as usize;
            rank_sum[fi] += rank_pos as u64;
            if rank_pos == 0 {
                wins[fi] += 1;
            }
            payout_sum[fi] += payouts[fi].as_u64().unwrap_or(0);
        }
        for p in v["phases"].as_array().unwrap() {
            if p["phase"] == "law" {
                if p["law_passed"].as_bool() == Some(true) {
                    laws_passed += 1;
                }
                if p["vetoed"].as_bool() == Some(true) {
                    vetoes += 1;
                }
            }
            for a in p["actions"].as_array().unwrap() {
                if a["action"] == "bribe" && a["ok"].as_bool() == Some(true) {
                    bribes += 1;
                }
            }
        }
        let _ = strats;
    }
    eprintln!("--- сводка по стратегиям (позиция в миксе) ---");
    for i in 0..n {
        eprintln!(
            "{:>8}: побед {:>3}/{:<3} ср.ранг {:>5.2} ср.выплата {:>8.2} песо",
            mix_ref[i],
            wins[i],
            games,
            rank_sum[i] as f64 / games as f64,
            payout_sum[i] as f64 / games as f64 / 1_000_000.0
        );
    }
    eprintln!(
        "законов прошло: {}, вето: {}, взяток: {}",
        laws_passed, vetoes, bribes
    );
}
