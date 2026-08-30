use alashi_rules::state::{Faction, Game, Phase};
use anchor_lang::prelude::Pubkey;
use indexer::{agent_id_for, aggregate, rank_factions};
use std::collections::BTreeMap;

fn game_with(id: u64, settled: bool) -> Game {
    let mut g = Game::default();
    g.game_id = id;
    g.entry_fee = 100_000_000;
    g.phase = Phase::Finished;
    g.settled = settled;
    g.faction_count = 4;
    g
}

fn fac(wallet_first: u8, cash: u64) -> Faction {
    let mut f = Faction::default();
    f.wallet = Pubkey::new_from_array([wallet_first; 32]);
    f.cash = cash;
    f.alive = true;
    f
}

fn game_key_of(id: u64) -> Pubkey {
    let seed = id.to_le_bytes();
    Pubkey::find_program_address(
        &[alashi::constants::GAME_SEED, seed.as_ref()],
        &alashi::id(),
    )
    .0
}

#[test]
fn ranks_asymmetric_50_30_15_5() {
    let g = game_with(1, true);
    let key = game_key_of(1);
    let a = fac(1, 500);
    let b = fac(2, 300);
    let c = fac(3, 150);
    let d = fac(4, 50);
    let mut map = BTreeMap::new();
    map.insert(
        key,
        vec![d.clone(), b.clone(), a.clone(), c.clone()],
    );
    let order = rank_factions(&map[&key]);
    assert_eq!(order, vec![2, 1, 3, 0]);

    let reg = vec![indexer::AgentEntry {
        wallet: a.wallet,
        agent_id: agent_id_for("glm-4.5-flash", "Botagul prompt"),
        model: "glm-4.5-flash".into(),
        prompt: "Botagul prompt".into(),
    }];
    let agg = aggregate(&[g], &map, &reg);
    assert_eq!(agg.parties_indexed, 1);
    let reg_stats = agg.stats.get(&agent_id_for("glm-4.5-flash", "Botagul prompt")).unwrap();
    assert_eq!(reg_stats.matches, 1);
    assert_eq!(reg_stats.rank_sum, 1);
    let unreg = agg.stats.get(&b.wallet.to_string()).unwrap();
    assert_eq!(unreg.rank_counts[1], 1);
    assert_eq!(unreg.avg_rank(), 2.0);
    let last = agg.stats.get(&d.wallet.to_string()).unwrap();
    assert_eq!(last.rank_counts[3], 1);
}

#[test]
fn tie_break_by_wallet_asc() {
    let x = fac(9, 100);
    let y = fac(2, 100);
    let order = rank_factions(&[x, y]);
    assert_eq!(order, vec![1, 0]);
}

#[test]
fn unsettled_games_ignored() {
    let map = BTreeMap::new();
    let agg = aggregate(&[game_with(2, false)], &map, &[]);
    assert_eq!(agg.parties_indexed, 0);
}

#[test]
fn agent_id_deterministic_and_prompt_sensitive() {
    let a = agent_id_for("m1", "p1");
    assert_eq!(a, agent_id_for("m1", "p1"));
    assert_ne!(a, agent_id_for("m1", "p2"));
    assert_ne!(a, agent_id_for("m2", "p1"));
}
