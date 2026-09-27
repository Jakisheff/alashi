// Tests for the social slice of the 27.09 "Zuckerberg/Muse" report:
// persistent character identity separated from strategy version, and
// pairwise meeting history built from recorded game events only.

use super::*;
use serde_json::{json, Value};

fn isolated(tag: &str) -> (Arc<AppState>, PathBuf) {
    let p = std::env::temp_dir().join(format!("alashi-social-{}-{tag}", std::process::id()));
    (new_state_with_files(p.join("state.json"), p.join("seq")), p)
}

fn new_game(state: &AppState) -> u64 {
    let r = h_new_game(state, &json!({"phase_duration": 30, "grace_s": 0}));
    assert_eq!(r["ok"], true, "{r}");
    r["game_id"].as_u64().unwrap()
}

fn join_character(
    state: &AppState,
    gid: u64,
    name: &str,
    owner_key: &str,
    model: &str,
    prompt: &str,
) -> Value {
    let r = h_join(
        state,
        gid,
        &json!({"name": name, "owner_key": owner_key, "model": model, "prompt": prompt}),
    );
    assert_eq!(r["ok"], true, "{r}");
    r
}

#[test]
fn character_identity_survives_strategy_change() {
    let (s, _) = isolated("identity");
    let gid = new_game(&s);
    let key_a = "ab".repeat(32);
    let j1 = join_character(&s, gid, "Zhambyl", &key_a, "glm-4.5", "prompt-v1");
    let cid = j1["character_id"].as_str().unwrap().to_string();
    let oid = j1["owner_id"].as_str().unwrap().to_string();
    assert_eq!(cid.len(), 64);
    assert_ne!(cid, j1["agent_id"].as_str().unwrap());

    let gid2 = new_game(&s);
    // та же личность, другая версия стратегии
    let j2 = join_character(&s, gid2, "Zhambyl", &key_a, "glm-5.3", "prompt-v2");
    assert_eq!(j2["character_id"].as_str().unwrap(), cid, "персонаж не изменился");
    assert_eq!(j2["owner_id"].as_str().unwrap(), oid);
    assert_ne!(j2["agent_id"].as_str().unwrap(), j1["agent_id"].as_str().unwrap());

    // другой персонаж того же владельца: другая личность, тот же владелец
    let j3 = join_character(&s, gid2, "Aisultan", &key_a, "glm-4.5", "prompt-v1");
    assert_ne!(j3["character_id"].as_str().unwrap(), cid);
    assert_eq!(j3["owner_id"].as_str().unwrap(), oid);

    // тот же персонаж не занимает второе место в одной партии
    let dup = h_join(
        &s,
        gid,
        &json!({"name": "Zhambyl", "owner_key": key_a, "model": "glm-4.5", "prompt": "prompt-v1"}),
    );
    assert_eq!(dup["ok"], false, "{dup}");
    assert_eq!(dup["error"], "join_failed");
    assert_eq!(s.games.lock().unwrap()[&gid].sim.factions.len(), 1);
}

#[test]
fn recovery_addresses_character_across_models() {
    let (s, _) = isolated("recover-character");
    let gid = new_game(&s);
    let key = "cd".repeat(32);
    let secret = "ef".repeat(32);
    let j = h_join(
        &s,
        gid,
        &json!({"name": "Timur", "owner_key": key, "recovery_secret": secret, "model": "m1", "prompt": "p1"}),
    );
    assert_eq!(j["ok"], true, "{j}");
    let cid = j["character_id"].as_str().unwrap().to_string();

    // восстановление с новой моделью и промптом: та же личность
    let rec = h_join(
        &s,
        gid,
        &json!({
            "name": "Timur", "owner_key": key, "recover": true, "recovery_secret": secret,
            "model": "m2", "prompt": "p2",
        }),
    );
    assert_eq!(rec["ok"], true, "{rec}");
    assert_eq!(rec["recovered"], true);
    assert_eq!(rec["character_id"].as_str().unwrap(), cid);
    // отпечаток прежней стратегии возвращается как факт партии
    assert_eq!(rec["agent_id"].as_str().unwrap(), j["agent_id"].as_str().unwrap());

    // чужой секрет не проходит даже с правильным именем и owner_key
    let attack = h_join(
        &s,
        gid,
        &json!({
            "name": "Timur", "owner_key": key, "recover": true,
            "recovery_secret": "00".repeat(32), "model": "m2", "prompt": "p2",
        }),
    );
    assert_eq!(attack["ok"], false, "{attack}");
}

#[test]
fn pair_history_counts_recorded_interactions() {
    let (s, _) = isolated("history");
    let gid = new_game(&s);
    let key_a = "11".repeat(32);
    let key_b = "22".repeat(32);
    let ja = join_character(&s, gid, "Alfa", &key_a, "m", "p");
    let jb = join_character(&s, gid, "Beta", &key_b, "m", "p");
    let cid_a = ja["character_id"].as_str().unwrap().to_string();
    let cid_b = jb["character_id"].as_str().unwrap().to_string();
    let ta = ja["token"].as_str().unwrap().to_string();
    let tb = jb["token"].as_str().unwrap().to_string();

    let mut bribed = false;
    let mut opposed_done = false;
    // стартовый кэш для взятки (экономика партии: взнос уходит в банк)
    {
        let mut games = s.games.lock().unwrap();
        games.get_mut(&gid).unwrap().sim.factions[0].cash = 10_000_000;
    }
    for _ in 0..60 {
        let (phase, round) = {
            let games = s.games.lock().unwrap();
            let Some(e) = games.get(&gid) else { break; };
            (e.sim.game.phase, e.sim.game.round)
        };
        assert_ne!(phase, Phase::Aborted, "партия не должна прерываться");
        match phase {
            Phase::Action if !bribed => {
                let r = h_act(&s, gid, &json!({"token": ta, "action": "bribe", "params": {"to": 1, "amount": 5_000_000}}));
                assert_eq!(r["ok"], true, "{r}");
                bribed = true;
            }
            Phase::Law => {
                let a_choice = if round % 2 == 0 || opposed_done { "yes" } else { "no" };
                h_act(&s, gid, &json!({"token": ta, "action": "vote", "params": {"choice": a_choice}}));
                let b_choice = if round % 2 == 0 { "yes" } else { "no" };
                let rb = h_act(&s, gid, &json!({"token": tb, "action": "vote", "params": {"choice": b_choice}}));
                if round % 2 == 1 && rb["ok"] == true { opposed_done = true; }
            }
            _ => {}
        }
        let mut games = s.games.lock().unwrap();
        let Some(e) = games.get_mut(&gid) else { break; };
        e.sim.game.phase_ends_at = 0;
        drop(games);
        crank_once(&s);
    }
    assert!(s.games.lock().unwrap().get(&gid).is_none(), "партия закрыта и записана");
    assert_eq!(s.completed.lock().unwrap().len(), 1);

    let path = format!("/history?character_id={cid_a}&peer={cid_b}");
    let h = h_history(&s, &path);
    assert_eq!(h["ok"], true, "{h}");
    assert_eq!(h["meetings"], 1, "{h}");
    let m = &h["history"][0];
    assert!(m["me"]["rank"].is_u64());
    assert!(m["peer"]["rank"].is_u64());
    let inter = &m["interactions"];
    assert_eq!(inter["bribes_to_peer"], 5_000_000, "взятка A -> B учтена");
    assert_eq!(inter["bribes_from_peer"], 0);
    assert!(inter["same_votes"].as_u64().unwrap() >= 1, "совпадающие голоса посчитаны");
    assert!(inter["opposed_votes"].as_u64().unwrap() >= 1, "расхождение голосов посчитано");

    // пара без общих партий пуста
    let stranger = "99".repeat(32);
    let empty = h_history(&s, &format!("/history?character_id={cid_a}&peer={stranger}"));
    assert_eq!(empty["ok"], true);
    assert_eq!(empty["meetings"], 0);
}
