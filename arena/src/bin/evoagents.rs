//! evoagents: эволюция геномов GenomeBot → лестница калиброванных соперников.
//!
//! Фитнес особи — средняя выплата на фиксированном расписании seed×seat
//! против одного и того же набора соперников (common random numbers):
//! все геномы играют одни и те же партии, поэтому оценки сравнимы.
//! Уровни лестницы — чемпионы контрольных поколений (ранний = слабее).
//!
//! Пример:
//!   cargo run --manifest-path arena/Cargo.toml --bin evoagents -- \
//!       --generations 12 --population 16 --out data/eval/evolved/ladder.json

use arena::runner::{play_game, GameConfig};
use arena::strategies::{from_genome, by_name, Genome, GENE_MAX, GENE_MIN, GENE_NAMES};
use std::io::Write;

const DEFAULT_OPPONENTS: &[&str] = &["greedy", "tactical", "greedy", "random"];

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, lo: u8, hi: u8) -> u8 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % ((hi - lo + 1) as u64)) as u8
    }
}

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

/// Расписание (seed, seat): фиксировано на весь прогон.
fn schedule(base_seed: u64, n_seeds: usize, seats: usize) -> Vec<(u64, usize)> {
    let mut out = Vec::with_capacity(n_seeds * seats);
    for s in 0..n_seeds {
        let seed = base_seed.wrapping_add((s as u64) * 7919);
        for seat in 0..seats {
            out.push((seed, seat));
        }
    }
    out
}

/// Средняя выплата генома на расписании против фиксированного микса.
fn fitness(genome: &Genome, sched: &[(u64, usize)], opponents: &[&str], cfg: &GameConfig) -> f64 {
    let seats = opponents.len() + 1;
    let mut total = 0u64;
    for &(seed, seat) in sched {
        let mut strategies: Vec<Box<dyn arena::strategies::Strategy>> = opponents
            .iter()
            .enumerate()
            .map(|(i, name)| {
                // Идентичность соперника сохраняет свой поток при ротации мест.
                by_name(name, seed.wrapping_add(i as u64 + 1)).expect("unknown opponent")
            })
            .collect();
        let me = from_genome(genome.clone());
        strategies.insert(seat.min(seats - 1), me);
        let rec = play_game(0, seed, &mut strategies, cfg);
        total += rec.payouts[seat.min(seats - 1)];
    }
    total as f64 / sched.len() as f64
}

fn tournament(fitnesses: &[f64], k: usize, rng: &mut Rng) -> usize {
    let mut best = (rng.next() % fitnesses.len() as u64) as usize;
    for _ in 1..k.max(1) {
        let cand = (rng.next() % fitnesses.len() as u64) as usize;
        if fitnesses[cand] > fitnesses[best] {
            best = cand;
        }
    }
    best
}

fn crossover(a: &Genome, b: &Genome, rng: &mut Rng) -> Genome {
    let (va, vb) = (a.to_vec(), b.to_vec());
    let mut child = [0u8; 6];
    for i in 0..6 {
        child[i] = if rng.next() % 2 == 0 { va[i] } else { vb[i] };
    }
    Genome::from_vec(&child)
}

fn mutate(g: &Genome, rate: f64, rng: &mut Rng) -> Genome {
    let mut v = g.to_vec();
    for i in 0..6 {
        if (rng.next() % 10_000) as f64 / 10_000.0 < rate {
            let delta = (rng.next() % 3 + 1) as i16;
            let signed = if rng.next() % 2 == 0 { delta } else { -delta };
            v[i] = (v[i] as i16 + signed).clamp(GENE_MIN[i] as i16, GENE_MAX[i] as i16) as u8;
        }
    }
    Genome::from_vec(&v)
}

struct Params {
    generations: usize,
    population: usize,
    seeds: usize,
    base_seed: u64,
    mutation_rate: f64,
}

/// Полный прогон эволюции; возвращает (уровни лестницы, история фитнеса).
///
/// `target_alashi` = None: максимизируем выплату (сильнейший уровень).
/// Иначе подгоняем выплату под цель: фитнес = −|выплата − цель|, так уровень
/// калибруется по силе, а не гонится за максимумом.
fn evolve(p: &Params, opponents: &[&str], cfg: &GameConfig, target_alashi: Option<f64>)
          -> (Vec<(usize, Genome, f64)>, Vec<f64>) {
    let seats = opponents.len() + 1;
    let sched = schedule(p.base_seed, p.seeds, seats);
    let scored = |g: &Genome| {
        let payout = fitness(g, &sched, opponents, cfg);
        let s = match target_alashi {
            Some(t) => -(payout - t).abs(),
            None => payout,
        };
        (s, payout)
    };
    let mut rng = Rng::new(p.base_seed ^ target_alashi.map(|t| t.to_bits()).unwrap_or(0));
    let mut population: Vec<Genome> = (0..p.population)
        .map(|_| {
            let mut v = [0u8; 6];
            for i in 0..6 {
                v[i] = rng.range(GENE_MIN[i], GENE_MAX[i]);
            }
            Genome::from_vec(&v)
        })
        .collect();

    let mut history = Vec::new();
    let mut ladder: Vec<(usize, Genome, f64)> = Vec::new();
    let marks: Vec<usize> = [0usize, p.generations / 4, p.generations / 2,
                             3 * p.generations / 4, p.generations - 1]
        .into_iter()
        .filter(|&g| g < p.generations)
        .collect();

    for gen in 0..p.generations {
        let fitnesses: Vec<f64> = population.iter().map(|g| scored(g).0).collect();
        let best = fitnesses
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
            .map(|(i, f)| (i, *f))
            .expect("nonempty population");
        history.push(best.1);
        if marks.contains(&gen) {
            ladder.push((gen, population[best.0].clone(), scored(&population[best.0]).1));
        }
        // Элитизм + турнирная селекция + кроссовер + мутация.
        let mut next = vec![population[best.0].clone()];
        if p.population > 1 {
            let second = fitnesses
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != best.0)
                .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
                .map(|(i, _)| i)
                .expect("nonempty population");
            next.push(population[second].clone());
        }
        while next.len() < p.population {
            let ai = tournament(&fitnesses, 3, &mut rng);
            let bi = tournament(&fitnesses, 3, &mut rng);
            let child = mutate(&crossover(&population[ai], &population[bi], &mut rng),
                               p.mutation_rate, &mut rng);
            next.push(child);
        }
        population = next;
    }
    (ladder, history)
}

/// Парная проверка лестницы на свежих сидах: более поздний уровень
/// против более раннего на зеркальных местах.
fn verify(ladder: &[(usize, Genome, f64)], base_seed: u64, seeds: usize,
          opponents: &[&str], cfg: &GameConfig) -> Vec<serde_json::Value> {    let seats = opponents.len() + 1;
    let mut out = Vec::new();
    for w in ladder.windows(2) {
        let (ga, gb) = (&w[0].1, &w[1].1);
        let mut diff = 0i64;
        let mut n = 0u64;
        for s in 0..seeds {
            let seed = base_seed.wrapping_add((s as u64) * 104_729);
            for seat in 0..seats {
                let mut strat_a: Vec<Box<dyn arena::strategies::Strategy>> = opponents
                    .iter().enumerate()
                    .map(|(i, name)| by_name(name, seed.wrapping_add(i as u64 + 1)).expect("unknown opponent"))
                    .collect();
                let mut strat_b: Vec<Box<dyn arena::strategies::Strategy>> = opponents
                    .iter().enumerate()
                    .map(|(i, name)| by_name(name, seed.wrapping_add(i as u64 + 1)).expect("unknown opponent"))
                    .collect();
                let mirror = seats - 1 - seat;
                strat_a.insert(seat, from_genome(ga.clone()));
                strat_b.insert(mirror, from_genome(gb.clone()));
                let ra = play_game(0, seed, &mut strat_a, cfg);
                let rb = play_game(0, seed, &mut strat_b, cfg);
                diff += rb.payouts[mirror] as i64 - ra.payouts[seat] as i64;
                n += 1;
            }
        }
        out.push(serde_json::json!({
            "later_generation": w[1].0, "earlier_generation": w[0].0,
            "mean_payout_delta": diff as f64 / n as f64, "paired_matches": n,
        }));
    }
    out
}

/// Оценка обобщения лестницы: выплата каждого уровня на тренировочных
/// сидах эволюции против выплаты на скрытом расписании, которое эволюция
/// не видела. Разрыв (visible − heldout) — местный аналог reward hacking
/// gap: на сколько селекция подогнала геном под видимые партии.
fn heldout_report(ladder: &[(usize, Genome, f64)], p: &Params, heldout_seeds: usize,
                  opponents: &[&str], cfg: &GameConfig) -> serde_json::Value {
    let seats = opponents.len() + 1;
    let visible_sched = schedule(p.base_seed, p.seeds, seats);
    // База скрытого расписания не пересекается ни с тренировочной
    // (base_seed + s*7919), ни с проверочной (base_seed ^ 0x5EED).
    let heldout_base = p.base_seed ^ 0x0DDBA11;
    let heldout_sched = schedule(heldout_base, heldout_seeds, seats);
    let per_level: Vec<serde_json::Value> = ladder
        .iter()
        .enumerate()
        .map(|(i, &(_, ref g, _))| {
            let visible = fitness(g, &visible_sched, opponents, cfg);
            let heldout = fitness(g, &heldout_sched, opponents, cfg);
            let gap = visible - heldout;
            serde_json::json!({
                "level": i + 1,
                "visible_payout_alashi": visible / 1_000_000.0,
                "heldout_payout_alashi": heldout / 1_000_000.0,
                "gap_alashi": gap / 1_000_000.0,
                "retention": if visible.abs() > f64::EPSILON {
                    Some(heldout / visible)
                } else { None },
            })
        })
        .collect();
    serde_json::json!({
        "heldout_base_seed": heldout_base,
        "heldout_seeds": heldout_seeds,
        "per_level": per_level,
        "interpretation": "Разрыв visible − heldout показывает, сколько силы уровня \
                           существует только на тренировочном расписании сидов; \
                           retention ниже 1 — подгонка под видимые партии.",
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let params = Params {
        generations: flag(&args, "--generations").and_then(|v| v.parse().ok()).unwrap_or(12),
        population: flag(&args, "--population").and_then(|v| v.parse().ok()).unwrap_or(16),
        seeds: flag(&args, "--seeds").and_then(|v| v.parse().ok()).unwrap_or(4),
        base_seed: flag(&args, "--seed").and_then(|v| v.parse().ok()).unwrap_or(7),
        mutation_rate: flag(&args, "--mutation-rate").and_then(|v| v.parse().ok()).unwrap_or(0.25),
    };
    let opponents_arg = flag(&args, "--opponents")
        .unwrap_or_else(|| DEFAULT_OPPONENTS.join(","));
    let opponents: Vec<&str> = opponents_arg.split(',').map(|s| s.trim()).collect();
    if params.generations < 2 || params.population < 4 || params.seeds < 1 {
        eprintln!("[ERROR] --generations >= 2, --population >= 4, --seeds >= 1");
        std::process::exit(2);
    }
    if !opponents.iter().all(|n| arena::strategies::ALL.contains(n) && *n != "genome") {
        eprintln!("[ERROR] соперники: имена встроенных ботов без genome: {}",
                  arena::strategies::ALL.join(", "));
        std::process::exit(2);
    }
    let cfg = GameConfig::default();
    let started = std::time::Instant::now();
    eprintln!("evoagents: {} поколений на уровень, популяция {}, сидов {}, соперники [{}]",
              params.generations, params.population, params.seeds, opponents.join(","));

    // Целевые уровни силы (млн alashi) + финальный уровень «максимум».
    let targets_arg = flag(&args, "--targets").unwrap_or_else(|| "6,10,14,18".to_string());
    let mut targets: Vec<Option<f64>> = targets_arg
        .split(',')
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .map(|m| Some(m * 1_000_000.0))
        .collect();
    if targets.is_empty() {
        eprintln!("[ERROR] --targets: числа через запятую, например 6,10,14,18");
        std::process::exit(2);
    }
    targets.push(None);

    let mut ladder: Vec<(usize, Genome, f64)> = Vec::new();
    let mut histories: Vec<Vec<f64>> = Vec::new();
    let mut targets_out: Vec<Option<f64>> = Vec::new();
    for (idx, target) in targets.iter().enumerate() {
        let mut p = Params { base_seed: params.base_seed.wrapping_add((idx as u64) * 0x9E37_79B9), ..params_clone(&params) };
        p.mutation_rate = params.mutation_rate;
        let (levels, history) = evolve(&p, &opponents, &cfg, *target);
        let (_, genome, payout) = levels.last().expect("ladder nonempty").clone();
        match target {
            Some(t) => eprintln!("цель {:>5.1}M: достигнуто {:>7.3}M alashi [{:?}]",
                                 t / 1e6, payout / 1e6, genome),
            None => eprintln!("максимум  : достигнуто {:>7.3}M alashi [{:?}]", payout / 1e6, genome),
        }
        targets_out.push(*target);
        ladder.push((idx, genome, payout));
        histories.push(history);
    }
    // Сортировка уровней по достигнутой силе: level 1 = слабейший.
    let mut order: Vec<usize> = (0..ladder.len()).collect();
    order.sort_by(|&a, &b| ladder[a].2.partial_cmp(&ladder[b].2).unwrap());
    ladder = order.iter().map(|&i| ladder[i].clone()).collect();
    targets_out = order.iter().map(|&i| targets_out[i]).collect();
    histories = order.iter().map(|&i| histories[i].clone()).collect();

    let verify_seeds: usize = flag(&args, "--verify-seeds").and_then(|v| v.parse().ok()).unwrap_or(8);
    let verification = verify(&ladder, params.base_seed ^ 0x5EED, verify_seeds, &opponents, &cfg);
    for (i, v) in verification.iter().enumerate() {
        eprintln!("проверка: уровень {} против {}: средняя разница выплат {:+.3} alashi за {} пар",
                  i + 2, i + 1,
                  v["mean_payout_delta"].as_f64().unwrap_or(0.0) / 1_000_000.0,
                  v["paired_matches"]);
    }
    // Скрытая оценка обобщения: считается только при явном флаге.
    let heldout_seeds: usize = flag(&args, "--heldout-seeds").and_then(|v| v.parse().ok()).unwrap_or(0);
    let heldout = if heldout_seeds >= 1 {
        let report = heldout_report(&ladder, &params, heldout_seeds, &opponents, &cfg);
        for level in report["per_level"].as_array().expect("per_level array") {
            eprintln!("обобщение: уровень {:>2}: видимые {:>8.3}M, скрытые {:>8.3}M, разрыв {:+.3}M alashi",
                      level["level"], level["visible_payout_alashi"].as_f64().unwrap_or(0.0),
                      level["heldout_payout_alashi"].as_f64().unwrap_or(0.0),
                      level["gap_alashi"].as_f64().unwrap_or(0.0));
        }
        Some(report)
    } else {
        None
    };
    let payload = serde_json::json!({
        "schema_version": 1,
        "params": {
            "generations_per_level": params.generations, "population": params.population,
            "seeds": params.seeds, "base_seed": params.base_seed,
            "mutation_rate": params.mutation_rate, "opponents": opponents,
            "targets_arg": targets_arg,
        },
        "gene_names": GENE_NAMES,
        "gene_min": GENE_MIN, "gene_max": GENE_MAX,
        "levels": ladder.iter().enumerate().map(|(i, (_, g, f))| serde_json::json!({
            "level": i + 1, "target_alashi": targets_out[i].map(|t| t / 1e6),
            "achieved_alashi": f / 1_000_000.0, "genome": g,
        })).collect::<Vec<_>>(),
        "fitness_histories_alashi": histories.iter()
            .map(|h| h.iter().map(|f| f / 1_000_000.0).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "verification": verification,
        "heldout": heldout,
        "elapsed_s": started.elapsed().as_secs_f64(),
        "interpretation": "Локальная калибровка на фиксированном расписании сидов; сила уровня гарантируется только относительно этого микса соперников.",
    });
    let text = serde_json::to_string_pretty(&payload).expect("serialize ladder");
    match flag(&args, "--out") {
        Some(path) => {
            if let Some(dir) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(dir).ok();
            }
            let mut f = match std::fs::File::create(&path) {
                Ok(f) => f,
                Err(e) => { eprintln!("[ERROR] не открыть {}: {}", path, e); std::process::exit(1); }
            };
            writeln!(f, "{}", text).ok();
            eprintln!("записано: {}", path);
        }
        None => println!("{}", text),
    }
}

fn params_clone(p: &Params) -> Params {
    Params { generations: p.generations, population: p.population, seeds: p.seeds,
             base_seed: p.base_seed, mutation_rate: p.mutation_rate }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_cfg() -> (Params, GameConfig) {
        (Params { generations: 3, population: 6, seeds: 2, base_seed: 11, mutation_rate: 0.25 },
         GameConfig::default())
    }

    #[test]
    fn genome_operators_respect_bounds() {
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let mut a = [0u8; 6];
            let mut b = [0u8; 6];
            for i in 0..6 {
                a[i] = rng.range(GENE_MIN[i], GENE_MAX[i]);
                b[i] = rng.range(GENE_MIN[i], GENE_MAX[i]);
            }
            let ga = Genome::from_vec(&a);
            let gb = Genome::from_vec(&b);
            let child = mutate(&crossover(&ga, &gb, &mut rng), 1.0, &mut rng);
            for (i, v) in child.to_vec().iter().enumerate() {
                assert!(*v >= GENE_MIN[i] && *v <= GENE_MAX[i]);
            }
        }
    }

    #[test]
    fn evolution_is_deterministic_by_seed() {
        let (p, cfg) = small_cfg();
        let (l1, h1) = evolve(&p, DEFAULT_OPPONENTS, &cfg, None);
        let (l2, h2) = evolve(&p, DEFAULT_OPPONENTS, &cfg, None);
        assert_eq!(h1, h2);
        assert_eq!(l1.len(), l2.len());
        assert!(l1.iter().zip(l2.iter()).all(|(a, b)| a.1 == b.1 && a.2 == b.2));
    }

    #[test]
    fn genome_genes_change_fitness_deterministically() {
        let (_, cfg) = small_cfg();
        let sched = schedule(7, 3, DEFAULT_OPPONENTS.len() + 1);
        // Проверено на фиксированном расписании: глубокая закупка на дне
        // таблицы цен (арбитраж 1..2 → 12) даёт заметно большую выплату,
        // чем редкая закупка и мелкие резервы.
        let deepdip = Genome { dip_buy: 1, buy_reserve: 2, max_buy_units: 1,
                               bribe_appetite: 0, bribe_gate: 20, license_bid: 0 };
        let passive = Genome { dip_buy: 2, buy_reserve: 4, max_buy_units: 3,
                               bribe_appetite: 1, bribe_gate: 6, license_bid: 8 };
        let f_deep = fitness(&deepdip, &sched, DEFAULT_OPPONENTS, &cfg);
        let f_passive = fitness(&passive, &sched, DEFAULT_OPPONENTS, &cfg);
        assert!(f_deep > f_passive, "deepdip {} <= passive {}", f_deep, f_passive);
    }

    #[test]
    fn heldout_report_uses_disjoint_seeds_and_is_deterministic() {
        let (p, cfg) = small_cfg();
        let opponents = ["greedy", "random"];
        let (ladder, _) = evolve(&p, &opponents, &cfg, None);
        assert!(!ladder.is_empty());
        let r1 = heldout_report(&ladder, &p, 3, &opponents, &cfg);
        let r2 = heldout_report(&ladder, &p, 3, &opponents, &cfg);
        assert_eq!(r1, r2, "отчёт обобщения должен быть детерминированным");
        // Скрытая база не совпадает с тренировочной и проверочной.
        let heldout_base = r1["heldout_base_seed"].as_u64().expect("seed");
        assert_ne!(heldout_base, p.base_seed);
        assert_ne!(heldout_base, p.base_seed ^ 0x5EED);
        let levels = r1["per_level"].as_array().expect("per_level");
        assert_eq!(levels.len(), ladder.len());
        for level in levels {
            assert!(level["visible_payout_alashi"].is_f64());
            assert!(level["heldout_payout_alashi"].is_f64());
            let gap = level["visible_payout_alashi"].as_f64().unwrap()
                    - level["heldout_payout_alashi"].as_f64().unwrap();
            assert!((gap - level["gap_alashi"].as_f64().unwrap()).abs() < 1e-9);
        }
    }

    #[test]
    fn heldout_schedule_shares_no_seed_with_training_schedule() {
        let seats = DEFAULT_OPPONENTS.len() + 1;
        let p = Params { generations: 3, population: 6, seeds: 8, base_seed: 42, mutation_rate: 0.25 };
        let training: std::collections::HashSet<u64> =
            schedule(p.base_seed, p.seeds, seats).iter().map(|&(s, _)| s).collect();
        let heldout: std::collections::HashSet<u64> =
            schedule(p.base_seed ^ 0x0DDBA11, 12, seats).iter().map(|&(s, _)| s).collect();
        assert!(training.is_disjoint(&heldout),
                "скрытые сиды пересекаются с тренировочными");
    }
}
