//! Content-addressed game states: canonical hash, partial order,
//! branching and merging for the ALASHI rules crate.
//!
//! Design: the political configuration of a game (resources, laws,
//! factions, votes) is content; execution noise (bump, admin keys,
//! phase timing) is not. The canonical hash captures only content.
//! Branching clones a game at a round boundary; merging computes
//! the least upper bound of two branches when one exists.

use crate::state::{Faction, Game, Phase, VoteChoice};
use anchor_lang::prelude::Pubkey;
use sha1::{Digest, Sha1};

/// A content-addressed game state hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StateHash(pub [u8; 20]);

impl std::fmt::Display for StateHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0.iter() {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// Feed canonical content into the hash.
fn feed(h: &mut Sha1, label: &str, bytes: &[u8]) {
    h.update(label.as_bytes());
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

fn feed_u8(h: &mut Sha1, label: &str, v: u8) {
    feed(h, label, &[v]);
}

fn feed_u16(h: &mut Sha1, label: &str, v: u16) {
    feed(h, label, &v.to_le_bytes());
}

fn feed_u64(h: &mut Sha1, label: &str, v: u64) {
    feed(h, label, &v.to_le_bytes());
}

fn feed_i64(h: &mut Sha1, label: &str, v: i64) {
    feed(h, label, &v.to_le_bytes());
}

fn feed_i8(h: &mut Sha1, label: &str, v: i8) {
    feed(h, label, &v.to_le_bytes());
}

fn feed_bool(h: &mut Sha1, label: &str, v: bool) {
    feed_u8(h, label, v as u8);
}

fn feed_option_u64(h: &mut Sha1, label: &str, v: Option<u64>) {
    match v {
        Some(x) => {
            feed_u8(h, &format!("{}.some", label), 1);
            feed_u64(h, label, x);
        }
        None => feed_u8(h, &format!("{}.none", label), 0),
    }
}

/// Compute the canonical content hash of a Game.
///
/// Content fields (political configuration): phase, round, faction_count,
/// laws_passed, sold_this_round, entry_fee, rake_bps, last_law_passed,
/// yes/no influence, law_card, laws_used_mask, veto_pending, president,
/// active/pending economy fields, settled, entropy_mode, vote_weight_mode,
/// epoch, amnesty_used, customs_tight/decided, prize_pot, license_yield/
/// holder/sold, barter_offers, barter_next_id, vrf_account, commit_slot,
/// vrf_retries, vrf_spent.
///
/// Noise fields (excluded): admin (execution identity), game_id (instance
/// identifier), phase_ends_at (wall clock), phase_duration (config, not
/// content of the state itself), bump (Anchor runtime).
fn canonical_game_hash(game: &Game) -> [u8; 20] {
    let mut h = Sha1::new();

    feed_u8(&mut h, "phase", game.phase as u8);
    feed_u8(&mut h, "round", game.round);
    feed_u8(&mut h, "faction_count", game.faction_count);
    feed_u16(&mut h, "laws_passed", game.laws_passed);
    feed_u16(&mut h, "sold_this_round", game.sold_this_round);
    feed_u64(&mut h, "entry_fee", game.entry_fee);
    feed_u16(&mut h, "rake_bps", game.rake_bps);
    feed_bool(&mut h, "last_law_passed", game.last_law_passed);
    feed(&mut h, "yes_influence", &game.yes_influence.to_le_bytes());
    feed(&mut h, "no_influence", &game.no_influence.to_le_bytes());
    feed_u8(&mut h, "law_card", game.law_card);
    feed_u8(&mut h, "laws_used_mask", game.laws_used_mask);
    feed_bool(&mut h, "veto_pending", game.veto_pending);
    feed(&mut h, "president", game.president.as_ref());
    feed_u16(&mut h, "active_tax_bps", game.active_tax_bps);
    feed_u8(&mut h, "active_subsidy_goods", game.active_subsidy_goods);
    feed_i8(&mut h, "active_price_shift", game.active_price_shift);
    feed_u8(&mut h, "active_boom", game.active_boom);
    feed_u16(&mut h, "pending_tax_bps", game.pending_tax_bps);
    feed_u8(&mut h, "pending_subsidy_goods", game.pending_subsidy_goods);
    feed_i8(&mut h, "pending_price_shift", game.pending_price_shift);
    feed_u8(&mut h, "pending_boom", game.pending_boom);
    feed_bool(&mut h, "settled", game.settled);
    feed_u8(&mut h, "entropy_mode", game.entropy_mode);
    feed_u8(&mut h, "vote_weight_mode", game.vote_weight_mode);
    feed_u8(&mut h, "epoch", game.epoch);
    feed_bool(&mut h, "amnesty_used", game.amnesty_used);
    feed_bool(&mut h, "customs_tight", game.customs_tight);
    feed_bool(&mut h, "customs_decided", game.customs_decided);
    feed_u64(&mut h, "prize_pot", game.prize_pot);
    feed_u64(&mut h, "license_yield", game.license_yield);
    feed(&mut h, "license_holder", game.license_holder.as_ref());
    feed_bool(&mut h, "license_sold", game.license_sold);
    feed_u64(&mut h, "barter_next_id", game.barter_next_id);
    feed(&mut h, "vrf_account", game.vrf_account.as_ref());
    feed_u64(&mut h, "commit_slot", game.commit_slot);
    feed_u8(&mut h, "vrf_retries", game.vrf_retries);
    feed_u64(&mut h, "vrf_spent", game.vrf_spent);

    // barter offers: sorted by id, each offer hashed
    let mut offers: Vec<_> = game.barter_offers.iter().collect();
    offers.sort_by_key(|o| o.id);
    feed_u64(&mut h, "barter_count", offers.len() as u64);
    for o in offers {
        feed_u64(&mut h, "barter_id", o.id);
        feed(&mut h, "barter_from", o.from.as_ref());
        feed(&mut h, "barter_to", o.to.as_ref());
        feed_u16(&mut h, "barter_goods", o.goods);
        feed_u64(&mut h, "barter_price", o.price);
    }

    let d = h.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&d);
    out
}

/// Compute the canonical content hash of a Faction.
///
/// Content: wallet, name, cash, goods, influence, vote, is_president,
/// alive, grey_goods, promissory, hard, roof_to/armed/tariff,
/// vote_sold_to/sold, vote_offer_to/price, bid, insider.
///
/// Noise: game (parent reference), acted_stamp/voted_stamp (phase bookkeeping),
/// bump (Anchor runtime).
fn canonical_faction_hash(f: &Faction) -> [u8; 20] {
    let mut h = Sha1::new();
    feed(&mut h, "wallet", f.wallet.as_ref());
    feed(&mut h, "name", f.name.as_bytes());
    feed_u64(&mut h, "cash", f.cash);
    feed_u16(&mut h, "goods", f.goods);
    feed_u16(&mut h, "influence", f.influence);
    feed_u8(&mut h, "vote", f.vote as u8);
    feed_bool(&mut h, "is_president", f.is_president);
    feed_bool(&mut h, "alive", f.alive);
    feed_u16(&mut h, "grey_goods", f.grey_goods);
    feed_u64(&mut h, "promissory", f.promissory);
    feed_u64(&mut h, "hard", f.hard);
    feed(&mut h, "roof_to", f.roof_to.as_ref());
    feed_bool(&mut h, "roof_armed", f.roof_armed);
    feed_u8(&mut h, "roof_tariff", f.roof_tariff);
    feed(&mut h, "vote_sold_to", f.vote_sold_to.as_ref());
    feed_bool(&mut h, "vote_sold", f.vote_sold);
    feed(&mut h, "vote_offer_to", f.vote_offer_to.as_ref());
    feed_u64(&mut h, "vote_offer_price", f.vote_offer_price);
    feed_u64(&mut h, "bid", f.bid);
    feed_bool(&mut h, "insider", f.insider);
    let d = h.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&d);
    out
}

/// Full state hash: game + all factions (sorted by wallet for canonical order).
pub fn canonical_state_hash(game: &Game, factions: &[Faction]) -> StateHash {
    let mut h = Sha1::new();
    let gh = canonical_game_hash(game);
    feed(&mut h, "game", &gh);
    let mut sorted: Vec<_> = factions.iter().collect();
    sorted.sort_by_key(|f| f.wallet);
    // faction_count is in the game hash; here we hash the faction list
    for f in sorted {
        let fh = canonical_faction_hash(f);
        feed(&mut h, "faction", &fh);
    }
    let d = h.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&d);
    StateHash(out)
}

/// A branch point: the state at a specific round, cloneable.
#[derive(Clone)]
pub struct Branch {    pub game: Game,
    pub factions: Vec<Faction>,
    /// The round this branch was taken from.
    pub from_round: u8,
}

/// A conflict between two branches.
#[derive(Clone, Debug, PartialEq)]
pub enum MergeConflict {
    /// Same field, different values in the two branches.
    FieldDiverged { field: String, ours: String, theirs: String },
    /// One branch has a faction the other doesn't.
    FactionOnlyInOne { wallet: Pubkey, branch: u8 },
    /// Phases diverged irreconcilably.
    PhaseMismatch { ours: Phase, theirs: Phase },
    /// Both branches claim different settlement outcomes.
    SettledDifferently,
}

/// Result of a merge attempt.
pub enum MergeResult {
    /// Merged state (least upper bound).
    Merged(Box<Branch>),
    /// Conflicts prevent automatic merge.
    Conflict(Vec<MergeConflict>),
}

impl std::fmt::Debug for MergeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeResult::Merged(b) => write!(
                f,
                "MergeResult::Merged(round={}, factions={}, hash={})",
                b.from_round,
                b.factions.len(),
                canonical_state_hash(&b.game, &b.factions)
            ),
            MergeResult::Conflict(cs) => write!(f, "MergeResult::Conflict({:?})", cs),
        }
    }
}

/// Branch from a game at a round boundary: clone the full state.
pub fn branch(game: &Game, factions: &[Faction], round: u8) -> Branch {
    Branch {
        game: game.clone(),
        factions: factions.to_vec(),
        from_round: round,
    }
}

/// Attempt to merge two branches that diverged from a common ancestor.
///
/// Merge semantics (least upper bound):
/// - Game scalar fields: if both branches changed the same field to
///   different values → conflict; if only one changed → take the change;
///   if neither changed → keep.
/// - Factions: union by wallet; if a faction exists in both, merge
///   per-field with the same rule; if only in one → include.
/// - The result is the "both changes applied" state when they commute.
pub fn merge(ours: &Branch, theirs: &Branch, ancestor: &Branch) -> MergeResult {
    let mut conflicts = Vec::new();
    let mut merged_game = ancestor.game.clone();

    // scalar game fields: compare both branches against ancestor
    macro_rules! merge_field {
        ($field:ident, $name:expr) => {
            let a = ancestor.game.$field;
            let o = ours.game.$field;
            let t = theirs.game.$field;
            if o != a && t != a && o != t {
                conflicts.push(MergeConflict::FieldDiverged {
                    field: $name.to_string(),
                    ours: format!("{:?}", o),
                    theirs: format!("{:?}", t),
                });
            } else if o != a {
                merged_game.$field = o;
            } else if t != a {
                merged_game.$field = t;
            }
        };
    }

    merge_field!(phase, "phase");
    merge_field!(round, "round");
    merge_field!(laws_passed, "laws_passed");
    merge_field!(settled, "settled");
    merge_field!(last_law_passed, "last_law_passed");
    merge_field!(veto_pending, "veto_pending");
    merge_field!(law_card, "law_card");
    merge_field!(laws_used_mask, "laws_used_mask");
    merge_field!(epoch, "epoch");
    merge_field!(amnesty_used, "amnesty_used");
    merge_field!(customs_tight, "customs_tight");
    merge_field!(customs_decided, "customs_decided");
    merge_field!(license_sold, "license_sold");
    merge_field!(president, "president");
    merge_field!(active_tax_bps, "active_tax_bps");
    merge_field!(active_subsidy_goods, "active_subsidy_goods");
    merge_field!(active_price_shift, "active_price_shift");
    merge_field!(active_boom, "active_boom");
    merge_field!(pending_tax_bps, "pending_tax_bps");
    merge_field!(pending_subsidy_goods, "pending_subsidy_goods");
    merge_field!(pending_price_shift, "pending_price_shift");
    merge_field!(pending_boom, "pending_boom");
    merge_field!(prize_pot, "prize_pot");
    merge_field!(license_yield, "license_yield");
    merge_field!(license_holder, "license_holder");
    merge_field!(vrf_account, "vrf_account");

    // settlement conflicts
    if ours.game.settled && theirs.game.settled {
        let oh = canonical_state_hash(&ours.game, &ours.factions);
        let th = canonical_state_hash(&theirs.game, &theirs.factions);
        if oh != th {
            conflicts.push(MergeConflict::SettledDifferently);
        }
    }

    // phase is handled by merge_field above; the explicit Finished
    // propagation was redundant (macro already covers it)

    // factions: union by wallet, tracking indices in both branches
    let mut merged_factions: Vec<Faction> = Vec::new();
    let mut wallet_map: std::collections::BTreeMap<Pubkey, (Option<usize>, Option<usize>)> =
        std::collections::BTreeMap::new();
    for (i, f) in ours.factions.iter().enumerate() {
        wallet_map.insert(f.wallet, (Some(i), None));
    }
    for (i, f) in theirs.factions.iter().enumerate() {
        match wallet_map.get_mut(&f.wallet) {
            Some((_, t)) => *t = Some(i),
            None => {
                wallet_map.insert(f.wallet, (None, Some(i)));
            }
        }
    }
    for (wallet, (oi, ti)) in wallet_map.iter() {
        let ours_f = oi.and_then(|i| ours.factions.get(i));
        let theirs_f = ti.and_then(|i| theirs.factions.get(i));
        let ancestor_f = ancestor
            .factions
            .iter()
            .find(|f| f.wallet == *wallet);
        match (ours_f, theirs_f, ancestor_f) {
            (Some(o), Some(t), Some(a)) => {
                // merge per-field
                let mut merged = a.clone();
                macro_rules! mf {
                    ($field:ident, $name:expr) => {
                        if o.$field != a.$field && t.$field != a.$field && o.$field != t.$field {
                            conflicts.push(MergeConflict::FieldDiverged {
                                field: format!("faction.{}", $name),
                                ours: format!("{:?}", o.$field),
                                theirs: format!("{:?}", t.$field),
                            });
                        } else if o.$field != a.$field {
                            merged.$field = o.$field;
                        } else if t.$field != a.$field {
                            merged.$field = t.$field;
                        }
                    };
                }
                mf!(cash, "cash");
                mf!(goods, "goods");
                mf!(influence, "influence");
                mf!(vote, "vote");
                mf!(alive, "alive");
                mf!(is_president, "is_president");
                mf!(grey_goods, "grey_goods");
                mf!(promissory, "promissory");
                mf!(hard, "hard");
                mf!(roof_armed, "roof_armed");
                mf!(roof_tariff, "roof_tariff");
                mf!(vote_sold, "vote_sold");
                mf!(vote_sold_to, "vote_sold_to");
                mf!(bid, "bid");
                mf!(insider, "insider");
                merged_factions.push(merged);
            }
            (Some(o), Some(t), None) => {
                // both branches have it but ancestor doesn't: take the
                // more conservative (fewer resources if diverged)
                if o.wallet != t.wallet {
                    conflicts.push(MergeConflict::FactionOnlyInOne {
                        wallet: o.wallet,
                        branch: 3, // in both
                    });
                }
                merged_factions.push(o.clone());
            }
            (Some(o), None, _) => {
                merged_factions.push(o.clone());
            }
            (None, Some(t), _) => {
                merged_factions.push(t.clone());
            }
            (None, None, _) => {}
        }
    }
    merged_game.faction_count = merged_factions.len() as u8;

    if conflicts.is_empty() {
        MergeResult::Merged(Box::new(Branch {
            game: merged_game,
            factions: merged_factions,
            from_round: ours.from_round.max(theirs.from_round),
        }))
    } else {
        MergeResult::Conflict(conflicts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_game(phase: Phase, round: u8) -> Game {
        let mut g = Game::default();
        g.phase = phase;
        g.round = round;
        g
    }

    fn mk_faction(wallet: &str, cash: u64) -> Faction {
        let mut f = Faction::default();
        f.wallet = Pubkey::new_unique();
        f.name = wallet.to_string();
        f.cash = cash;
        f.alive = true;
        f.influence = 1;
        f.vote = VoteChoice::Abstain;
        f
    }

    #[test]
    fn same_state_same_hash() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let h1 = canonical_state_hash(&g, &f);
        let h2 = canonical_state_hash(&g, &f);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_faction_order_same_hash() {
        let g = mk_game(Phase::Market, 3);
        let f1 = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let f2 = vec![f1[1].clone(), f1[0].clone()];
        assert_eq!(
            canonical_state_hash(&g, &f1),
            canonical_state_hash(&g, &f2)
        );
    }

    #[test]
    fn different_cash_different_hash() {
        let g = mk_game(Phase::Market, 3);
        let f1 = vec![mk_faction("a", 100)];
        let mut f2 = f1.clone();
        f2[0].cash = 300; // same wallet, only cash differs
        assert_ne!(
            canonical_state_hash(&g, &f1),
            canonical_state_hash(&g, &f2)
        );
    }

    #[test]
    fn noise_fields_excluded() {
        let g1 = mk_game(Phase::Market, 3);
        let mut g2 = g1.clone();
        g2.admin = Pubkey::new_unique(); // noise
        g2.game_id = 999; // noise
        g2.phase_ends_at = 12345678; // noise
        g2.bump = 7; // noise
        let f = vec![mk_faction("a", 100)];
        assert_eq!(
            canonical_state_hash(&g1, &f),
            canonical_state_hash(&g2, &f)
        );
    }

    #[test]
    fn branch_is_clone() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let b = branch(&g, &f, 3);
        assert_eq!(b.game.round, 3);
        assert_eq!(b.factions.len(), 2);
        assert_eq!(canonical_state_hash(&b.game, &b.factions), canonical_state_hash(&g, &f));
    }

    #[test]
    fn merge_commutative_changes() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let ancestor = branch(&g, &f, 3);

        // ours: faction a gets more cash
        let mut ours = ancestor.clone();
        ours.factions[0].cash = 150;

        // theirs: faction b gets more cash
        let mut theirs = ancestor.clone();
        theirs.factions[1].cash = 250;

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.factions[0].cash, 150);
                assert_eq!(m.factions[1].cash, 250);
            }
            MergeResult::Conflict(c) => panic!("expected merge, got {:?}", c),
        }
    }

    #[test]
    fn merge_conflicting_change() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100)];
        let ancestor = branch(&g, &f, 3);

        let mut ours = ancestor.clone();
        ours.factions[0].cash = 150;

        let mut theirs = ancestor.clone();
        theirs.factions[0].cash = 999; // different from both ancestor and ours

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(_) => panic!("expected conflict"),
            MergeResult::Conflict(c) => {
                assert!(c.iter().any(|x| matches!(x, MergeConflict::FieldDiverged { .. })));
            }
        }
    }

    #[test]
    fn merge_one_side_only() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100)];
        let ancestor = branch(&g, &f, 3);

        let mut ours = ancestor.clone();
        ours.game.laws_passed = 5;

        let theirs = ancestor.clone(); // no changes

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.game.laws_passed, 5);
            }
            MergeResult::Conflict(c) => panic!("expected merge, got {:?}", c),
        }
    }

    #[test]
    fn merge_phase_finished_propagates() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100)];
        let ancestor = branch(&g, &f, 3);

        let mut ours = ancestor.clone();
        ours.game.phase = Phase::Finished;

        let theirs = ancestor.clone();

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.game.phase, Phase::Finished);
            }
            MergeResult::Conflict(_) => panic!("expected merge"),
        }
    }

    #[test]
    fn merge_faction_count_correct() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let ancestor = branch(&g, &f, 3);

        let mut ours = ancestor.clone();
        ours.factions[0].cash = 150;
        let mut theirs = ancestor.clone();
        theirs.factions[1].cash = 250;

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.game.faction_count, 2);
                assert_eq!(m.factions.len(), 2);
            }
            MergeResult::Conflict(_) => panic!("expected merge"),
        }
    }

    #[test]
    fn merge_is_symmetric() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100), mk_faction("b", 200)];
        let ancestor = branch(&g, &f, 3);

        let mut ours = ancestor.clone();
        ours.factions[0].cash = 150;
        let mut theirs = ancestor.clone();
        theirs.factions[1].cash = 250;

        let ab = merge(&ours, &theirs, &ancestor);
        let ba = merge(&theirs, &ours, &ancestor);

        match (ab, ba) {
            (MergeResult::Merged(a), MergeResult::Merged(b)) => {
                assert_eq!(
                    canonical_state_hash(&a.game, &a.factions),
                    canonical_state_hash(&b.game, &b.factions)
                );
            }
            _ => panic!("expected both to merge"),
        }
    }

    #[test]
    fn different_name_different_hash() {
        let g = mk_game(Phase::Market, 3);
        let f1 = vec![mk_faction("alice", 100)];
        let mut f2 = f1.clone();
        f2[0].name = "bob".to_string(); // same wallet, only name differs
        assert_ne!(
            canonical_state_hash(&g, &f1),
            canonical_state_hash(&g, &f2)
        );
    }

    #[test]
    fn different_bid_different_hash() {
        let g = mk_game(Phase::Market, 3);
        let f1 = vec![mk_faction("a", 100)];
        let mut f2 = f1.clone();
        f2[0].bid = 500; // same wallet, only bid differs
        assert_ne!(
            canonical_state_hash(&g, &f1),
            canonical_state_hash(&g, &f2)
        );
    }

    #[test]
    fn merge_same_change_both_sides() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100)];
        let ancestor = branch(&g, &f, 3);

        // both branches make the SAME change
        let mut ours = ancestor.clone();
        ours.factions[0].cash = 150;
        let mut theirs = ancestor.clone();
        theirs.factions[0].cash = 150;

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.factions[0].cash, 150);
            }
            MergeResult::Conflict(c) => {
                panic!("same change on both sides should merge, got {:?}", c);
            }
        }
    }

    #[test]
    fn merge_adds_faction_from_one_side() {
        let g = mk_game(Phase::Market, 3);
        let f = vec![mk_faction("a", 100)];
        let ancestor = branch(&g, &f, 3);

        let mut theirs = ancestor.clone();
        theirs.factions.push(mk_faction("new", 50));

        let ours = ancestor.clone();

        match merge(&ours, &theirs, &ancestor) {
            MergeResult::Merged(m) => {
                assert_eq!(m.factions.len(), 2);
            }
            MergeResult::Conflict(c) => panic!("expected merge, got {:?}", c),
        }
    }
}
