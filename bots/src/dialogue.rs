//! Public-only deliberation. Validation authorizes no transaction: the caller must
//! reread canonical accounts and revalidate immediately before signing.
use anchor_lang::prelude::Pubkey;
use serde_json::{json, Value};
use std::str::FromStr;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicFaction {
    pub pda: String,
    pub goods: u64,
    pub cash: u64,
    pub acted_this_phase: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicOffer {
    pub id: String,
    pub proposer: String,
    pub recipient: Option<String>,
    pub goods: u64,
    pub price: u64,
    pub round: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicMessage {
    pub id: String,
    pub author: String,
    pub recipient: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicContext {
    pub game: String,
    pub faction: String,
    pub round: u8,
    pub phase: String,
    pub phase_ends_at: i64,
    pub observed_cursor: u64,
    pub produce_yield: u16,
    pub sold_this_round: u16,
    pub active_price_shift: i8,
    pub active_boom: u8,
    pub active_tax_bps: u16,
    pub factions: Vec<PublicFaction>,
    pub open_offers: Vec<PublicOffer>,
    pub incoming: Vec<PublicMessage>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Pass,
    Produce,
    Sell { units: u16 },
    BarterPropose { goods: u64, price: u64 },
    BarterAccept { offer_id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub to_faction_pda: Option<String>,
    pub in_reply_to: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub action: Action,
    pub public_message: Option<Message>,
}
fn address(s: &str) -> bool {
    Pubkey::from_str(s).is_ok()
}
fn decimal(s: &str) -> bool {
    !s.is_empty()
        && (s == "0" || !s.starts_with('0'))
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u64>().is_ok()
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
impl PublicContext {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.active_tax_bps > 10_000
            || !address(&self.game)
            || !address(&self.faction)
            || !(1..=6).contains(&self.round)
            || !["Market", "Action", "Law", "Finished", "Aborted"].contains(&self.phase.as_str())
            || !(2..=6).contains(&self.factions.len())
            || self.incoming.len() > 100
            || self.open_offers.len() > 100
        {
            return Err("invalid_context");
        }
        let mut ids = std::collections::HashSet::new();
        for f in &self.factions {
            if !address(&f.pda) || f.goods > u16::MAX as u64 || !ids.insert(&f.pda) {
                return Err("invalid_context");
            }
        }
        if !ids.contains(&self.faction) {
            return Err("invalid_context");
        }
        let mut offers = std::collections::HashSet::new();
        for o in &self.open_offers {
            if !decimal(&o.id)
                || !offers.insert(&o.id)
                || !ids.contains(&o.proposer)
                || o.recipient
                    .as_ref()
                    .is_some_and(|r| !ids.contains(r) || r == &o.proposer)
                || o.round != self.round
                || o.goods == 0
                || o.goods > u16::MAX as u64
            {
                return Err("invalid_context");
            }
        }
        let mut messages = std::collections::HashSet::new();
        for m in &self.incoming {
            if m.id.is_empty()
                || m.id.len() > 128
                || !messages.insert(&m.id)
                || !ids.contains(&m.author)
                || m.author == self.faction
                || m.text.trim().is_empty()
                || m.text.chars().count() > 240
                || m.recipient.as_ref().is_some_and(|r| r != &self.faction)
            {
                return Err("invalid_context");
            }
        }
        Ok(())
    }
    pub fn to_public_json(&self) -> Result<Value, &'static str> {
        self.validate()?;
        let own = self
            .factions
            .iter()
            .find(|f| f.pda == self.faction)
            .ok_or("invalid_context")?;
        let sell_one = if own.goods > 0 {
            sale_revenue(self, 1).ok()
        } else {
            None
        };
        let sell_all = if own.goods > 0 {
            sale_revenue(self, own.goods as u16).ok()
        } else {
            None
        };
        Ok(
            json!({"sell_one_net_revenue":sell_one,"sell_all_net_revenue":sell_all,"game":self.game,"faction":self.faction,"round":self.round,"phase":self.phase,
            "observed_cursor":self.observed_cursor,"phase_ends_at":self.phase_ends_at,"produce_yield":self.produce_yield,"sold_this_round":self.sold_this_round,"active_price_shift":self.active_price_shift,"active_boom":self.active_boom,"active_tax_bps":self.active_tax_bps,"cash_units_per_alashi":alashi::constants::PESO,
            "factions":self.factions.iter().map(|f| json!({"pda":f.pda,"goods":f.goods,"cash":f.cash,"acted_this_phase":f.acted_this_phase})).collect::<Vec<_>>(),
            "open_offers":self.open_offers.iter().map(|o| json!({"offer_id":o.id,"proposer":o.proposer,"recipient":o.recipient,"goods":o.goods,"price":o.price,"round":o.round})).collect::<Vec<_>>(),
            "incoming":self.incoming.iter().map(|m| json!({"id":m.id,"author":m.author,"recipient":m.recipient,"text":m.text})).collect::<Vec<_>>()}),
        )
    }
}
/// Net sale proceeds in raw game cash units; 1 alashi = constants::PESO.
pub fn sale_revenue(c: &PublicContext, units: u16) -> Result<u64, &'static str> {
    if units == 0 || c.active_tax_bps > 10_000 || c.sold_this_round.checked_add(units).is_none() {
        return Err("invalid_sale");
    }
    // Prices and the maximum u16 sale size bound the shared engine's gross well below u64.
    let trade = alashi::logic::compute_sale(
        units,
        c.sold_this_round,
        c.active_price_shift,
        c.active_boom,
    );
    let tax = trade
        .gross
        .checked_mul(c.active_tax_bps as u64)
        .ok_or("invalid_sale")?
        / 10_000;
    trade.gross.checked_sub(tax).ok_or("invalid_sale")
}
pub fn parse_decision(
    raw: &str,
    context: &PublicContext,
    private_owner_wish: bool,
) -> Result<Decision, &'static str> {
    context.validate()?;
    if raw.len() > 4096 {
        return Err("response_too_large");
    }
    let v: Value = serde_json::from_str(raw).map_err(|_| "invalid_response")?;
    if !exact(&v, &["action", "public_message"]) {
        return Err("invalid_response");
    }
    let a = &v["action"];
    let action = match a["type"].as_str() {
        Some("pass") if exact(a, &["type"]) => Action::Pass,
        Some("produce") if exact(a, &["type"]) => Action::Produce,
        Some("sell") if exact(a, &["type", "units"]) => Action::Sell {
            units: a["units"]
                .as_u64()
                .and_then(|u| u16::try_from(u).ok())
                .filter(|u| *u > 0)
                .ok_or("invalid_action")?,
        },
        Some("barter_propose") if exact(a, &["type", "goods", "price"]) => Action::BarterPropose {
            goods: a["goods"].as_u64().ok_or("invalid_action")?,
            price: a["price"].as_u64().ok_or("invalid_action")?,
        },
        Some("barter_accept") if exact(a, &["type", "offer_id"]) => {
            let id = a["offer_id"]
                .as_str()
                .filter(|s| decimal(s))
                .ok_or("invalid_action")?;
            Action::BarterAccept {
                offer_id: id.into(),
            }
        }
        _ => return Err("invalid_action"),
    };
    let public_message = if v["public_message"].is_null() {
        None
    } else {
        if private_owner_wish {
            return Err("private_provenance");
        }
        let m = &v["public_message"];
        if !exact(m, &["text", "to_faction_pda", "in_reply_to"]) {
            return Err("invalid_message");
        }
        let optional = |key: &str| -> Result<Option<String>, &'static str> {
            if m[key].is_null() {
                Ok(None)
            } else {
                m[key]
                    .as_str()
                    .map(|s| Some(s.into()))
                    .ok_or("invalid_message")
            }
        };
        Some(Message {
            text: m["text"].as_str().ok_or("invalid_message")?.into(),
            to_faction_pda: optional("to_faction_pda")?,
            in_reply_to: optional("in_reply_to")?,
        })
    };
    let decision = Decision {
        action,
        public_message,
    };
    decision.validate(context, private_owner_wish)?;
    Ok(decision)
}
impl Decision {
    pub fn validate(
        &self,
        c: &PublicContext,
        private_owner_wish: bool,
    ) -> Result<(), &'static str> {
        c.validate()?;
        let own = c
            .factions
            .iter()
            .find(|f| f.pda == c.faction)
            .ok_or("invalid_context")?;
        match &self.action {
            Action::Pass => {}
            Action::Produce => {
                if c.phase != "Action"
                    || own.acted_this_phase
                    || own
                        .goods
                        .checked_add(c.produce_yield as u64)
                        .is_none_or(|g| g > u16::MAX as u64)
                {
                    return Err("invalid_action");
                }
            }
            Action::Sell { units } => {
                if c.phase != "Market"
                    || own.acted_this_phase
                    || *units == 0
                    || *units as u64 > own.goods
                {
                    return Err("invalid_action");
                }
                let revenue = sale_revenue(c, *units)?;
                if own.cash.checked_add(revenue).is_none() {
                    return Err("invalid_action");
                }
            }
            Action::BarterPropose { goods, price } => {
                if c.phase != "Market"
                    || *goods == 0
                    || *goods > own.goods
                    || *goods > u16::MAX as u64
                    || *price == 0
                {
                    return Err("invalid_action");
                }
            }
            Action::BarterAccept { offer_id } => {
                let o = c
                    .open_offers
                    .iter()
                    .find(|o| &o.id == offer_id)
                    .ok_or("stale_offer")?;
                let seller = c
                    .factions
                    .iter()
                    .find(|f| f.pda == o.proposer)
                    .ok_or("invalid_context")?;
                if c.phase != "Market"
                    || o.proposer == c.faction
                    || o.recipient.as_ref().is_some_and(|r| r != &c.faction)
                    || own.cash < o.price
                    || seller.goods < o.goods
                    || own
                        .goods
                        .checked_add(o.goods)
                        .is_none_or(|goods| goods > u16::MAX as u64)
                    || seller.cash.checked_add(o.price).is_none()
                {
                    return Err("invalid_action");
                }
            }
        }
        if let Some(m) = &self.public_message {
            if private_owner_wish {
                return Err("private_provenance");
            }
            if !["Market", "Action", "Law"].contains(&c.phase.as_str())
                || m.to_faction_pda.is_none()
                || m.text.trim().is_empty()
                || m.text.chars().count() > 240
                || m.text.chars().any(char::is_control)
                || m.to_faction_pda
                    .as_ref()
                    .is_some_and(|r| r == &c.faction || !c.factions.iter().any(|f| &f.pda == r))
            {
                return Err("invalid_message");
            }
            if let Some(reply) = &m.in_reply_to {
                let parent = c
                    .incoming
                    .iter()
                    .find(|i| &i.id == reply)
                    .ok_or("unknown_reply")?;
                if m.to_faction_pda.as_ref() != Some(&parent.author) {
                    return Err("invalid_reply_recipient");
                }
            }
        }
        Ok(())
    }
    pub fn validate_fresh(
        &self,
        observed: &PublicContext,
        current: &PublicContext,
        private_owner_wish: bool,
    ) -> Result<(), &'static str> {
        if observed != current {
            return Err("stale_context");
        }
        self.validate(current, private_owner_wish)
    }
}
#[derive(Debug)]
pub struct SessionBudget {
    faction: String,
    remaining: u16,
    deadline_ms: u64,
    call_timeout_ms: u64,
}
impl SessionBudget {
    pub fn new(
        faction: String,
        calls: u16,
        deadline_ms: u64,
        call_timeout_ms: u64,
    ) -> Result<Self, &'static str> {
        if !address(&faction)
            || calls == 0
            || calls > 120
            || call_timeout_ms == 0
            || call_timeout_ms > 60_000
        {
            return Err("invalid_budget");
        }
        Ok(Self {
            faction,
            remaining: calls,
            deadline_ms,
            call_timeout_ms,
        })
    }
    pub fn reserve(&mut self, context: &PublicContext, now_ms: u64) -> Result<u64, &'static str> {
        context.validate()?;
        if context.faction != self.faction {
            return Err("wrong_agent_session");
        }
        if self.remaining == 0 || now_ms >= self.deadline_ms {
            return Err("budget_exhausted");
        }
        self.remaining -= 1;
        Ok(self.call_timeout_ms.min(self.deadline_ms - now_ms))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> PublicContext {
        PublicContext {
            game: Pubkey::new_from_array([3; 32]).to_string(),
            faction: Pubkey::new_from_array([1; 32]).to_string(),
            round: 1,
            phase: "Market".into(),
            phase_ends_at: 100,
            observed_cursor: 0,
            produce_yield: 2,
            sold_this_round: 0,
            active_price_shift: 0,
            active_boom: 0,
            active_tax_bps: 0,
            factions: vec![
                PublicFaction {
                    pda: Pubkey::new_from_array([1; 32]).to_string(),
                    goods: 3,
                    cash: 20,
                    acted_this_phase: false,
                },
                PublicFaction {
                    pda: Pubkey::new_from_array([2; 32]).to_string(),
                    goods: 2,
                    cash: 10,
                    acted_this_phase: false,
                },
            ],
            open_offers: vec![],
            incoming: vec![],
        }
    }
    fn response(action: Value, message: Value) -> String {
        json!({"action":action,"public_message":message}).to_string()
    }
    #[test]
    fn independent_contexts_can_initiate_and_accept() {
        let a = context();
        let raw = response(
            json!({"type":"barter_propose","goods":2,"price":7}),
            json!({"text":"Would you buy two goods?","to_faction_pda":Pubkey::new_from_array([2; 32]).to_string(),"in_reply_to":null}),
        );
        let decision = parse_decision(&raw, &a, false).unwrap();
        decision.validate_fresh(&a, &a, false).unwrap();
        let mut b = a.clone();
        b.faction = Pubkey::new_from_array([2; 32]).to_string();
        b.open_offers.push(PublicOffer {
            id: "9".into(),
            proposer: a.faction.clone(),
            recipient: Some(b.faction.clone()),
            goods: 2,
            price: 7,
            round: 1,
        });
        b.incoming.push(PublicMessage {
            id: "message-a".into(),
            author: a.faction.clone(),
            recipient: Some(b.faction.clone()),
            text: "Would you buy two goods?".into(),
        });
        let raw = response(
            json!({"type":"barter_accept","offer_id":"9"}),
            json!({"text":"I accept.","to_faction_pda":a.faction,"in_reply_to":"message-a"}),
        );
        parse_decision(&raw, &b, false).unwrap();
        assert!(decision.validate_fresh(&a, &b, false).is_err());
    }
    #[test]
    fn rejects_stale_offer_cash_goods_phase_and_unknown_keys() {
        let mut c = context();
        let accept = response(json!({"type":"barter_accept","offer_id":"1"}), Value::Null);
        assert_eq!(
            parse_decision(&accept, &c, false).unwrap_err(),
            "stale_offer"
        );
        c.open_offers.push(PublicOffer {
            id: "1".into(),
            proposer: Pubkey::new_from_array([2; 32]).to_string(),
            recipient: None,
            goods: 2,
            price: 21,
            round: 1,
        });
        assert!(parse_decision(&accept, &c, false).is_err());
        c.open_offers[0].price = 1;
        c.open_offers[0].goods = 3;
        assert!(parse_decision(&accept, &c, false).is_err());
        c.open_offers[0].goods = 1;
        c.phase = "Law".into();
        assert!(parse_decision(&accept, &c, false).is_err());
        let extra = response(json!({"type":"pass","secret":"hidden"}), Value::Null);
        assert!(parse_decision(&extra, &c, false).is_err());
        assert!(parse_decision("{}", &c, false).is_err());
    }
    #[test]
    fn private_messages_and_bad_replies_fail_closed() {
        let c = context();
        let pass = json!({"type":"pass"});
        let msg = json!({"text":"Hello","to_faction_pda":Pubkey::new_from_array([2; 32]).to_string(),"in_reply_to":null});
        assert_eq!(
            parse_decision(&response(pass.clone(), msg.clone()), &c, true).unwrap_err(),
            "private_provenance"
        );
        let mut m = msg.clone();
        m["text"] = json!("x".repeat(241));
        assert!(parse_decision(&response(pass.clone(), m), &c, false).is_err());
        let mut m = msg.clone();
        m["to_faction_pda"] = json!(c.faction);
        assert!(parse_decision(&response(pass.clone(), m), &c, false).is_err());
        let mut m = msg;
        m["in_reply_to"] = json!("missing");
        assert!(parse_decision(&response(pass, m), &c, false).is_err());
        assert!(parse_decision(&response(json!({"type":"pass"}), Value::Null), &c, true).is_ok());
    }
    #[test]
    fn budgets_are_per_agent_and_context_updates_invalidate_decisions() {
        let c = context();
        let d = parse_decision(&response(json!({"type":"pass"}), Value::Null), &c, false).unwrap();
        let mut newer = c.clone();
        newer.observed_cursor = 1;
        assert_eq!(d.validate_fresh(&c, &newer, false), Err("stale_context"));
        let mut budget = SessionBudget::new(c.faction.clone(), 1, 1000, 500).unwrap();
        let mut other = c.clone();
        other.faction = Pubkey::new_from_array([2; 32]).to_string();
        assert_eq!(budget.reserve(&other, 0), Err("wrong_agent_session"));
        assert_eq!(budget.reserve(&c, 800), Ok(200));
        assert_eq!(budget.reserve(&c, 801), Err("budget_exhausted"));
        newer.factions.push(newer.factions[0].clone());
        assert!(newer.validate().is_err());
    }
    #[test]
    fn rejects_chain_overflow_deadline_changes_and_terminal_speech() {
        let mut c = context();
        let raw = response(
            json!({"type":"barter_propose","goods":65536,"price":1}),
            Value::Null,
        );
        assert!(parse_decision(&raw, &c, false).is_err());
        c.open_offers.push(PublicOffer {
            id: "7".into(),
            proposer: c.factions[1].pda.clone(),
            recipient: Some(c.faction.clone()),
            goods: 1,
            price: 1,
            round: 1,
        });
        c.factions[1].cash = u64::MAX;
        assert!(parse_decision(
            &response(json!({"type":"barter_accept","offer_id":"7"}), Value::Null),
            &c,
            false
        )
        .is_err());
        let pass =
            parse_decision(&response(json!({"type":"pass"}), Value::Null), &c, false).unwrap();
        let mut fresh = c.clone();
        fresh.phase_ends_at += 1;
        assert_eq!(pass.validate_fresh(&c, &fresh, false), Err("stale_context"));
        let m = json!({"text":"Hello","to_faction_pda":c.factions[1].pda,"in_reply_to":null});
        c.phase = "Finished".into();
        assert!(parse_decision(&response(json!({"type":"pass"}), m), &c, false).is_err());
        c.game = "G".repeat(32);
        assert!(c.validate().is_err());
    }
    #[test]
    fn zero_price_incoming_offer_does_not_invalidate_public_context() {
        let mut c = context();
        c.open_offers.push(PublicOffer {
            id: "9".into(),
            proposer: c.factions[1].pda.clone(),
            recipient: None,
            goods: 1,
            price: 0,
            round: c.round,
        });
        assert!(
            c.validate().is_ok(),
            "existing free offer is valid public state"
        );
        let decision = parse_decision(
            &response(json!({"type":"barter_accept","offer_id":"9"}), Value::Null),
            &c,
            false,
        )
        .unwrap();
        assert_eq!(
            decision.action,
            Action::BarterAccept {
                offer_id: "9".into()
            }
        );
        assert!(
            parse_decision(
                &response(
                    json!({"type":"barter_propose","goods":1,"price":0}),
                    Value::Null
                ),
                &c,
                false
            )
            .is_err(),
            "runner policy can prohibit proposing free goods without rejecting observed offers"
        );
    }
    #[test]
    fn produce_sell_phase_stamps_math_and_freshness() {
        let mut c = context();
        let produce = response(json!({"type":"produce"}), Value::Null);
        assert!(parse_decision(&produce, &c, false).is_err());
        c.phase = "Action".into();
        assert_eq!(
            parse_decision(&produce, &c, false).unwrap().action,
            Action::Produce
        );
        c.factions[0].acted_this_phase = true;
        assert!(parse_decision(&produce, &c, false).is_err());
        c.factions[0].acted_this_phase = false;
        c.factions[0].goods = u16::MAX as u64;
        assert!(parse_decision(&produce, &c, false).is_err());
        c.factions[0].goods = 3;
        c.phase = "Market".into();
        c.active_tax_bps = 1000;
        assert_eq!(sale_revenue(&c, 2), Ok(19_800_000));
        let sell = response(json!({"type":"sell","units":2}), Value::Null);
        let d = parse_decision(&sell, &c, false).unwrap();
        assert_eq!(d.action, Action::Sell { units: 2 });
        for units in [0, 4, 65536] {
            assert!(parse_decision(
                &response(json!({"type":"sell","units":units}), Value::Null),
                &c,
                false
            )
            .is_err());
        }
        c.factions[0].cash = u64::MAX;
        assert!(parse_decision(&sell, &c, false).is_err());
        c.factions[0].cash = 0;
        c.factions[0].acted_this_phase = true;
        assert!(parse_decision(&sell, &c, false).is_err());
        c.factions[0].acted_this_phase = false;
        let mut fresh = c.clone();
        fresh.sold_this_round += 1;
        assert_eq!(d.validate_fresh(&c, &fresh, false), Err("stale_context"));
        c.sold_this_round = u16::MAX;
        assert!(sale_revenue(&c, 1).is_err());
        c.sold_this_round = 0;
        c.active_tax_bps = 10001;
        assert!(c.validate().is_err());
        assert!(parse_decision(
            &response(json!({"type":"produce","extra":1}), Value::Null),
            &context(),
            false
        )
        .is_err());
    }
}
