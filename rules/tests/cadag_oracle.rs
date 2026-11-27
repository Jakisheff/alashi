// Differential oracle for branching: a branch from round N,
// played forward, then merged back, must produce the same state
// as the linear game at round N.
//
// Pattern (from the mini-git oracle):
//   1. play a linear game from round 1 to round 6;
//   2. at every round boundary (the state right after a Law phase
//      closes, i.e. the entry into the next round's Market, before
//      any market action) clone the game;
//   3. replay the identical action script and seed schedule from
//      that clone to completion (round 6, Finished);
//   4. merge the replayed branch back into the linear trajectory
//      with a three-way merge against the boundary as the common
//      ancestor;
//   5. the merged state's canonical hash must equal the linear
//      game's hash at every compared point, boundaries and the
//      completion state alike.
//
// Scope note: `cadag::merge` propagates the political-configuration
// fields it lists and leaves the rest at the ancestor. The canonical
// hash, however, also covers a few fields merge does not propagate:
// sold_this_round, yes/no_influence, barter_next_id/barter_offers,
// commit_slot/vrf_retries/vrf_spent, faction.roof_to and
// faction.vote_offer_to/price. The oracle script therefore stays on
// the merge-propagated surface:
//   - no Yes/No votes: the tally counters would keep the ancestor's
//     values (Abstain votes are cast instead, so the vote() path is
//     still exercised);
//   - no roof and no barter: roof_to and barter_next_id persist and
//     are not propagated;
//   - no market trades in the last round: sold_this_round is 0 at
//     every boundary, but the completion state keeps the final
//     round's market total.
// faction.vote_offer_to/price are reset by every Law-phase close, so
// they are default at every boundary regardless of the script.
// The determinism test below has no such restrictions and covers the
// full action surface (Yes/No votes, passing laws, roof, barter,
// vote sale, last-round trades).

use alashi_rules::cadag::{
    branch, canonical_state_hash, merge, Branch, MergeResult, StateHash,
};
use alashi_rules::constants::*;
use alashi_rules::market_price;
use alashi_rules::sim::Simulator;
use alashi_rules::state::{Phase, VoteChoice};
use anchor_lang::prelude::Pubkey;

const SEEDS: [u64; 3] = [1, 42, 0xDEAD_BEEF];
const START_CASH: u64 = 100 * PESO;
const N: usize = MAX_FACTIONS as usize;

fn wallet(i: usize) -> Pubkey {
    Pubkey::new_from_array([(i + 1) as u8; 32])
}

/// One deterministic seed per (game, round): splitmix-style mixing of
/// the base seed and the round number. The seed schedule is the only
/// randomness input of a game, so replaying it from a cloned state
/// reproduces the linear game exactly.
fn round_seed(base: u64, round: u8) -> u64 {
    let mut x = base ^ (u64::from(round) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn new_game(base: u64) -> Simulator {
    let mut sim = Simulator::new(base, 10 * PESO, 0, ENTROPY_SLOTHASH);
    sim.game.epoch = EPOCH_90S;
    for i in 0..N {
        sim.join(wallet(i), &format!("F{i}")).unwrap();
    }
    // The simulator has no entry-fee escrow: seed the starting cash
    // directly, the way the other rules tests do.
    for f in sim.factions.iter_mut() {
        f.cash = START_CASH;
    }
    sim
}

/// Lobby -> Market(1): with the full six factions the game starts
/// immediately, regardless of the clock.
fn enter_round_one(sim: &mut Simulator, base: u64, clock: &mut i64) {
    *clock += 100;
    sim.advance(*clock, round_seed(base, 0)).unwrap();
    assert_eq!((sim.game.phase, sim.game.round), (Phase::Market, 1));
}

fn president_index(sim: &Simulator) -> Option<usize> {
    sim.factions
        .iter()
        .position(|f| f.alive && f.wallet == sim.game.president)
}

fn hash_of(b: &Branch) -> StateHash {
    canonical_state_hash(&b.game, &b.factions)
}

// ---------- oracle script (merge-propagated surface only) ----------

fn oracle_market(sim: &mut Simulator, r: u8) {
    if r == ROUNDS {
        return;
    }
    for i in 0..N {
        if sim.factions[i].goods >= 2 {
            if (r + i as u8) % 4 == 0 {
                // M4: promissory note path (revenue x1.25 later)
                sim.sell_credit(i, 1).unwrap();
            } else {
                sim.sell(i, 2).unwrap();
            }
        } else {
            let price = market_price(
                sim.game.sold_this_round,
                sim.game.active_price_shift,
                sim.game.active_boom,
            );
            if sim.factions[i].cash >= price {
                sim.buy(i, 1).unwrap();
            }
        }
    }
}

fn oracle_action(sim: &mut Simulator, r: u8) {
    // M8: the sitting president (elected at the previous round's
    // Action->Law) picks the customs regime; does not spend the action.
    if let Some(pi) = president_index(sim) {
        sim.set_customs(pi, r % 2 == 0).unwrap();
    }
    for i in 0..N {
        match (r + i as u8) % 4 {
            0 => {
                sim.produce(i).unwrap();
            }
            1 => {
                // M3: grey channel, customs risk at the border
                sim.shuttle(i).unwrap();
            }
            2 => {
                if sim.factions[i].cash >= BRIBE_PRICE {
                    sim.bribe(i, (i + 1) % N, BRIBE_PRICE).unwrap();
                } else {
                    sim.produce(i).unwrap();
                }
            }
            _ => {
                if sim.factions[i].cash >= DONKEY_PRICE * PESO {
                    sim.donkey(i).unwrap();
                } else {
                    sim.produce(i).unwrap();
                }
            }
        }
    }
    // M9: license insight and sealed bids in the auction round
    if r == AUCTION_ROUND {
        if !sim.factions[0].insider && sim.factions[0].cash >= LICENSE_INSIGHT_PRICE {
            sim.inspect_license(0).unwrap();
        }
        if sim.factions[1].cash >= 20 * PESO {
            sim.bid_license(1, 20 * PESO).unwrap();
        }
        if sim.factions[2].cash >= 10 * PESO {
            sim.bid_license(2, 10 * PESO).unwrap();
        }
    }
    // M6: money changer round trip on hard currency
    if r == 3 && sim.factions[3].cash >= 2 * PESO {
        sim.exchange(3, true).unwrap();
    }
    if r == 5 && sim.factions[3].hard >= 2 * PESO {
        sim.exchange(3, false).unwrap();
    }
}

fn oracle_law(sim: &mut Simulator, r: u8) {
    for i in 0..N {
        if (r + i as u8) % 2 == 0 {
            // Abstain-only: a counted vote would move yes/no_influence,
            // which the hash covers but the merge does not propagate.
            sim.vote(i, VoteChoice::Abstain).unwrap();
        }
    }
    if r == 2 || r == 5 {
        if let Some(pi) = president_index(sim) {
            sim.veto(pi).unwrap();
        }
    }
}

/// Drive one full round starting in Market(r): market actions,
/// advance to Action, action phase, advance to Law (card drawn),
/// voting, close into Market(r+1) or Finished.
fn play_oracle_round(sim: &mut Simulator, r: u8, base: u64, clock: &mut i64) {
    play_oracle_round_with(sim, r, base, clock, false);
}

fn play_oracle_round_with(
    sim: &mut Simulator,
    r: u8,
    base: u64,
    clock: &mut i64,
    divergent_vote: bool,
) {
    let seed = round_seed(base, r);
    assert_eq!((sim.game.phase, sim.game.round), (Phase::Market, r));
    oracle_market(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
    assert_eq!(sim.game.phase, Phase::Action);
    oracle_action(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
    assert_eq!(sim.game.phase, Phase::Law);
    if divergent_vote {
        // the one different decision for the negative control
        sim.vote(0, VoteChoice::Yes).unwrap();
    }
    oracle_law(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
}

// ---------- rich script (full action surface, determinism only) ----------

fn rich_market(sim: &mut Simulator, r: u8) {
    // M11: one direct goods-for-cash barter in round 2
    if r == 2 && sim.factions[0].goods >= 2 && sim.factions[1].cash >= 10 * PESO {
        let id = sim.barter_propose(0, Some(1), 2, 10 * PESO).unwrap();
        sim.barter_accept(1, id).unwrap();
    }
    for i in 0..N {
        if sim.factions[i].goods >= 2 {
            if (r + i as u8) % 4 == 0 {
                sim.sell_credit(i, 1).unwrap();
            } else {
                sim.sell(i, 2).unwrap();
            }
        } else {
            let price = market_price(
                sim.game.sold_this_round,
                sim.game.active_price_shift,
                sim.game.active_boom,
            );
            if sim.factions[i].cash >= price {
                sim.buy(i, 1).unwrap();
            }
        }
    }
}

fn rich_action(sim: &mut Simulator, r: u8) {
    if let Some(pi) = president_index(sim) {
        sim.set_customs(pi, r % 2 == 0).unwrap();
    }
    if r == AUCTION_ROUND {
        if !sim.factions[0].insider && sim.factions[0].cash >= LICENSE_INSIGHT_PRICE {
            sim.inspect_license(0).unwrap();
        }
        if sim.factions[1].cash >= 20 * PESO {
            sim.bid_license(1, 20 * PESO).unwrap();
        }
        if sim.factions[2].cash >= 10 * PESO {
            sim.bid_license(2, 10 * PESO).unwrap();
        }
    }
    for i in 0..N {
        match (r + i as u8) % 5 {
            0 => {
                sim.produce(i).unwrap();
            }
            1 => {
                sim.shuttle(i).unwrap();
            }
            2 => {
                if sim.factions[i].cash >= BRIBE_PRICE {
                    sim.bribe(i, (i + 1) % N, BRIBE_PRICE).unwrap();
                } else {
                    sim.produce(i).unwrap();
                }
            }
            3 => {
                if sim.factions[i].cash >= DONKEY_PRICE * PESO {
                    sim.donkey(i).unwrap();
                } else {
                    sim.produce(i).unwrap();
                }
            }
            _ => {
                // M7: a red roof contract, once per game per faction
                if !sim.factions[i].roof_armed && sim.factions[i].cash >= 10 * PESO {
                    sim.roof(i, (i + 1) % N, ROOF_RED).unwrap();
                } else {
                    sim.produce(i).unwrap();
                }
            }
        }
    }
    if r == 3 && sim.factions[4].cash >= 2 * PESO {
        sim.exchange(4, true).unwrap();
    }
    if r == 5 && sim.factions[4].hard >= 2 * PESO {
        sim.exchange(4, false).unwrap();
    }
}

fn rich_law(sim: &mut Simulator, r: u8) {
    for i in 0..N {
        let choice = if i % 2 == 0 { VoteChoice::Yes } else { VoteChoice::No };
        sim.vote(i, choice).unwrap();
    }
    // M10: faction 5 sells its vote to faction 0 in round 2
    if r == 2 && sim.factions[0].cash >= 2 * PESO {
        sim.offer_vote(5, 0, 2 * PESO).unwrap();
        sim.accept_vote_offer(0).unwrap();
    }
    if r == 5 {
        if let Some(pi) = president_index(sim) {
            sim.veto(pi).unwrap();
        }
    }
}

fn play_rich_round(sim: &mut Simulator, r: u8, base: u64, clock: &mut i64) {
    let seed = round_seed(base, r);
    assert_eq!((sim.game.phase, sim.game.round), (Phase::Market, r));
    rich_market(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
    assert_eq!(sim.game.phase, Phase::Action);
    rich_action(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
    assert_eq!(sim.game.phase, Phase::Law);
    rich_law(sim, r);
    *clock += 100;
    sim.advance(*clock, seed).unwrap();
}

/// One straight rich-script game, snapshotted at every round boundary
/// (Market entry of rounds 1..6) and at completion.
fn run_rich_game(base: u64) -> Vec<Branch> {
    let mut sim = new_game(base);
    let mut clock = 0;
    enter_round_one(&mut sim, base, &mut clock);
    let mut snaps = vec![branch(&sim.game, &sim.factions, sim.game.round)];
    for r in 1..=ROUNDS {
        play_rich_round(&mut sim, r, base, &mut clock);
        snaps.push(branch(&sim.game, &sim.factions, sim.game.round));
    }
    assert_eq!((sim.game.phase, sim.game.round), (Phase::Finished, ROUNDS));
    snaps
}

// ---------- tests ----------

#[test]
fn branch_replay_is_deterministic() {
    for &base in &SEEDS {
        // Same game, same actions, same seed schedule -> same hash at
        // every boundary: the linear reference run.
        let linear = run_rich_game(base);

        // Same game, but a branch is taken at the round-3 boundary.
        let mut sim = new_game(base);
        let mut clock = 0;
        enter_round_one(&mut sim, base, &mut clock);
        for r in 1..=2 {
            play_rich_round(&mut sim, r, base, &mut clock);
        }
        assert_eq!((sim.game.phase, sim.game.round), (Phase::Market, 3));
        // the branch point itself matches the linear game
        assert_eq!(
            hash_of(&branch(&sim.game, &sim.factions, 3)),
            hash_of(&linear[2]),
            "branch point diverged (seed {base:#x})"
        );

        let mut branch_sim = sim.clone();
        let mut main_tail: Vec<Branch> = Vec::new();
        for r in 3..=ROUNDS {
            play_rich_round(&mut sim, r, base, &mut clock);
            main_tail.push(branch(&sim.game, &sim.factions, sim.game.round));
        }
        assert_eq!(sim.game.phase, Phase::Finished);

        // the clone replays the same actions under its own clock
        let mut branch_clock = 1_000_000;
        let mut branch_tail: Vec<Branch> = Vec::new();
        for r in 3..=ROUNDS {
            play_rich_round(&mut branch_sim, r, base, &mut branch_clock);
            branch_tail.push(branch(&branch_sim.game, &branch_sim.factions, branch_sim.game.round));
        }
        assert_eq!(branch_sim.game.phase, Phase::Finished);

        // main continuation, branch replay and the linear game must
        // carry the same content at every post-branch boundary
        for k in 0..main_tail.len() {
            let lin = &linear[3 + k];
            assert_eq!(
                hash_of(&main_tail[k]),
                hash_of(lin),
                "main continuation diverged (seed {base:#x}, state after round {})",
                3 + k
            );
            assert_eq!(
                hash_of(&branch_tail[k]),
                hash_of(lin),
                "branch replay diverged (seed {base:#x}, state after round {})",
                3 + k
            );
        }
    }
}

#[test]
fn differential_merge_oracle_matches_linear() {
    for &base in &SEEDS {
        // 1. Linear game from round 1 to round 6, snapshot at every
        //    round boundary and at completion.
        let mut sim = new_game(base);
        let mut clock = 0;
        enter_round_one(&mut sim, base, &mut clock);
        let mut linear: Vec<Branch> = vec![branch(&sim.game, &sim.factions, sim.game.round)];
        // A branch is taken at every boundary, including Market(1).
        let mut branch_sims: Vec<Simulator> = vec![sim.clone()];
        for r in 1..=ROUNDS {
            play_oracle_round(&mut sim, r, base, &mut clock);
            linear.push(branch(&sim.game, &sim.factions, sim.game.round));
            if r < ROUNDS {
                branch_sims.push(sim.clone());
            }
        }
        assert_eq!((sim.game.phase, sim.game.round), (Phase::Finished, ROUNDS));

        // 2. Each branch replays the identical future to completion.
        for (i, mut bs) in branch_sims.into_iter().enumerate() {
            let n = i as u8 + 1; // the branch was taken at Market(n)
            let ancestor = linear[i].clone();
            assert_eq!((bs.game.phase, bs.game.round), (Phase::Market, n));

            let mut branch_clock = 10_000_000;
            let mut tail: Vec<Branch> = Vec::new();
            for r in n..=ROUNDS {
                play_oracle_round(&mut bs, r, base, &mut branch_clock);
                tail.push(branch(&bs.game, &bs.factions, bs.game.round));
            }
            assert_eq!(bs.game.phase, Phase::Finished);

            // 3. Merge the branch back at every later boundary and at
            //    completion: the merged hash must equal the linear hash.
            //    tail[j] is the state after round n+j; its linear
            //    counterpart is linear[n + j].
            for (j, theirs) in tail.iter().enumerate() {
                let ours = &linear[n as usize + j];
                assert_eq!(
                    hash_of(theirs),
                    hash_of(ours),
                    "branch replay diverged from the linear game (seed {base:#x}, branch at round {n}, state after round {})",
                    n as usize + j
                );
                match merge(ours, theirs, &ancestor) {
                    MergeResult::Merged(m) => {
                        assert_eq!(
                            hash_of(&m),
                            hash_of(ours),
                            "merging the replayed branch back changed the linear state (seed {base:#x}, branch at round {n}, state after round {})",
                            n as usize + j
                        );
                    }
                    MergeResult::Conflict(cs) => panic!(
                        "identical replay must merge cleanly (seed {base:#x}, branch at round {n}, state after round {}): {cs:?}",
                        n as usize + j
                    ),
                }
            }
        }
    }
}

#[test]
fn divergent_branch_is_detected_by_the_merge() {
    // Negative control: an oracle that cannot fail proves nothing.
    // One different decision on the branch (faction 0 casts a Yes
    // vote, which the oracle script never does) must surface either
    // as a merge conflict or as a merged state whose hash differs
    // from the linear game.
    for &base in &SEEDS {
        let mut sim = new_game(base);
        let mut clock = 0;
        enter_round_one(&mut sim, base, &mut clock);
        for r in 1..=2 {
            play_oracle_round(&mut sim, r, base, &mut clock);
        }

        let mut branch_sim = sim.clone();
        let ancestor = branch(&sim.game, &sim.factions, 3);

        // main line: the scripted round 3
        play_oracle_round(&mut sim, 3, base, &mut clock);
        let ours = branch(&sim.game, &sim.factions, 4);

        // branch: identical script except the one divergent vote
        let mut branch_clock = 1_000_000;
        play_oracle_round_with(&mut branch_sim, 3, base, &mut branch_clock, true);
        let theirs = branch(&branch_sim.game, &branch_sim.factions, 4);

        assert_ne!(
            hash_of(&theirs),
            hash_of(&ours),
            "the divergent branch must reach a different state (seed {base:#x})"
        );
        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_ne!(
                    hash_of(&m),
                    hash_of(&ours),
                    "merge folded a divergent branch into the linear state (seed {base:#x})"
                );
            }
            MergeResult::Conflict(_) => {
                // divergence surfaced as a merge conflict: detected
            }
        }
    }
}
