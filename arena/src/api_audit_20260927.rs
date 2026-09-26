// Regression tests for audit 20260927 (docs/ops/audit_20260927/REPORT.md),
// findings S2/S3/S4/S7. The original audit pins asserted the defective
// behavior; these tests assert the SAFE behavior required by the report
// acceptance criteria. No sockets, no Solana, isolated temp files only.

use super::*;
use serde_json::json;

fn isolated(tag: &str) -> (Arc<AppState>, PathBuf) {
    let p = std::env::temp_dir().join(format!("alashi-audit27-{}-{tag}", std::process::id()));
    (new_state_with_files(p.join("state.json"), p.join("seq")), p)
}

#[test]
fn rejected_action_payloads_are_not_retained_and_spam_is_contained() {
    let (s, p) = isolated("retention");
    let g = h_new_game(&s, &json!({"lobby_duration":604800}));
    let id = g["game_id"].as_u64().unwrap();
    let j = h_join(&s, id, &json!({"name":"audit", "model":"audit", "prompt":"audit"}));
    assert_eq!(j["ok"], true, "{j}");
    // вторая партия: спам в одной не должен останавливать другую
    let other = h_new_game(&s, &json!({"lobby_duration":604800}));
    let other_id = other["game_id"].as_u64().unwrap();
    let mut other_tokens = Vec::new();
    for n in 0..2 {
        let oj = h_join(&s, other_id, &json!({"name":format!("peer{n}"), "model":"audit", "prompt":format!("peer-{n}")}));
        assert_eq!(oj["ok"], true, "{oj}");
        other_tokens.push(oj["token"].as_str().unwrap().to_string());
    }
    let body = json!({"token":j["token"], "action":"produce", "params":{"unused":"x".repeat(65536)}});
    for _ in 0..32 {
        let r = h_act(&s, id, &body);
        assert_eq!(r["ok"], false);
        assert_eq!(r["error"], "bad_params", "{r}");
    }
    {
        let games = s.games.lock().unwrap();
        assert_eq!(games[&id].action_log.len(), 0);
    }
    // чужая партия живёт: лобби закрывается кранком в Market
    s.games.lock().unwrap().get_mut(&other_id).unwrap().sim.game.phase_ends_at = 0;
    crank_once(&s);
    assert_eq!(s.games.lock().unwrap()[&other_id].sim.game.phase, Phase::Market);
    save_snapshot(&s).unwrap();
    let size = std::fs::metadata(p.join("state.json")).unwrap().len();
    assert!(size < 200_000, "снимок слишком велик: {size}");
}

#[test]
fn active_game_creation_is_bounded() {
    let (s, _) = isolated("creation");
    for _ in 0..MAX_ACTIVE_GAMES {
        assert_eq!(h_new_game(&s, &json!({"lobby_duration":604800}))["ok"], true);
    }
    assert_eq!(s.games.lock().unwrap().len(), MAX_ACTIVE_GAMES);
    let over = h_new_game(&s, &json!({"lobby_duration":604800}));
    assert_eq!(over["ok"], false, "{over}");
    assert_eq!(over["error"], "arena_full");
}

#[test]
fn corrupt_snapshot_aborts_load_and_original_is_preserved() {
    let (s, p) = isolated("corrupt");
    assert_eq!(h_new_game(&s, &json!({}))["ok"], true);
    save_snapshot(&s).unwrap();
    std::fs::write(p.join("state.json"), "{incomplete").unwrap();
    let restored = new_state_with_files(p.join("state.json"), p.join("seq"));
    let err = load_snapshot(&restored).unwrap_err();
    assert!(err.contains("повреждён"), "{err}");
    assert!(restored.games.lock().unwrap().is_empty());
    let backups: Vec<std::path::PathBuf> = std::fs::read_dir(&p)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .map(|n| n.to_string_lossy().contains("corrupt"))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), "{incomplete");
}

#[test]
fn failed_snapshot_rolls_back_join_and_allows_retry() {
    let (s, p) = isolated("failed-save");
    let id = h_new_game(&s, &json!({}))["game_id"].as_u64().unwrap();
    // каталог на месте временного файла делает запись невозможной
    std::fs::create_dir_all(p.join("state.json.tmp")).unwrap();
    let body = json!({"name":"audit", "model":"audit", "prompt":"audit"});
    let r = h_join(&s, id, &body);
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], "storage_failed");
    assert_eq!(s.games.lock().unwrap()[&id].agents.len(), 0);
    // после устранения сбоя повтор даёт чистый результат
    std::fs::remove_dir_all(p.join("state.json.tmp")).unwrap();
    let retry = h_join(&s, id, &body);
    assert_eq!(retry["ok"], true, "{retry}");
}

#[test]
fn timestamp_seed_is_not_recoverable_from_public_law_cards() {
    use alashi_rules::logic::draw_law_index;
    use sha2::{Digest, Sha256};

    // Зеркало атаки: та же KDF, что на сервере, но с кандидатом вместо секрета
    fn kdf(master: u64, id: u64, round: u8) -> u64 {
        let mut h = Sha256::new();
        h.update(master.to_le_bytes());
        h.update(id.to_le_bytes());
        h.update(round.to_le_bytes());
        let d = h.finalize();
        u64::from_le_bytes(d[..8].try_into().unwrap())
    }

    let (s, _) = isolated("seed");
    let observation_time = now() as u64;
    let mut observations = Vec::new();
    for _ in 0..3 {
        let id = h_new_game(&s, &json!({"epoch":"classic"}))["game_id"].as_u64().unwrap();
        let mut games = s.games.lock().unwrap();
        let entry = games.get_mut(&id).unwrap();
        let mut cards = Vec::new();
        for round in 1..=6u8 {
            entry.sim.game.phase = Phase::Action;
            entry.sim.game.round = round;
            entry.sim.game.phase_ends_at = 0;
            let seed = round_seed(&s, id, round);
            entry.sim.advance(100, seed).unwrap();
            cards.push(entry.sim.game.law_card);
        }
        observations.push((id, cards));
    }
    // Самопроверка: зеркало с истинным секретом воспроизводит все карты
    let master = s.master_seed.load(Ordering::Relaxed);
    for (id, cards) in &observations {
        let mut mask = 0u8;
        for (i, &observed) in cards.iter().enumerate() {
            let (card, next) = draw_law_index(kdf(master, *id, i as u8 + 1), mask);
            assert_eq!(card, observed, "зеркало разошлось с сервером");
            mask = next;
        }
    }
    // Атака: перебор секундных кандидатов за сутки не находит seed
    let hits: Vec<u64> = (observation_time.saturating_sub(86400)..=observation_time)
        .filter(|&candidate| {
            observations.iter().all(|(id, cards)| {
                let mut mask = 0u8;
                cards.iter().enumerate().all(|(i, &observed)| {
                    let (card, next) = draw_law_index(kdf(candidate, *id, i as u8 + 1), mask);
                    mask = next;
                    card == observed
                })
            })
        })
        .collect();
    assert!(hits.is_empty(), "timestamp-кандидаты восстановили seed: {hits:?}");
    assert!(!(observation_time.saturating_sub(86400)..=observation_time).contains(&master));
}

#[test]
fn agent_id_encoding_is_unambiguous() {
    // Аудит 27.09 (S7): ("a|b","c") и ("a","b|c") больше не коллизируют
    assert_ne!(agent_id_of("a|b", "c"), agent_id_of("a", "b|c"));
    assert_ne!(agent_id_of("", "ab"), agent_id_of("a", "b"));
    assert_eq!(agent_id_of("model", "prompt"), agent_id_of("model", "prompt"));
}
