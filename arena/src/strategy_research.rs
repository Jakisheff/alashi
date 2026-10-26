//! Private research workflow: independent forecasts, decision inbox and reviewed reuse.
use super::*;

const MAX_PARTICIPANTS: usize = 32;
const LICENSE: &str = "CC0-1.0";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchState {
    pub backlog: BTreeMap<String, Backlog>,
    pub rounds: BTreeMap<String, ForecastRound>,
    pub applied: BTreeMap<String, Applied>,
    pub cards: BTreeMap<String, Card>,
    pub acknowledgements: BTreeMap<String, DigestAck>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backlog {
    pub id: String,
    pub record: String,
    pub owner: String,
    pub client_id: String,
    pub title: String,
    pub hypothesis: String,
    pub priority: String,
    pub comparison_id: Option<String>,
    pub source_card_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Forecast {
    pub probability_bps: u16,
    pub reason: String,
    pub submitted_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastRound {
    pub comparison_id: String,
    pub participants: BTreeSet<String>,
    pub forecasts: BTreeMap<String, Forecast>,
    pub created_at: i64,
    pub closed_at: Option<i64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    pub comparison_id: String,
    pub attempt_id: String,
    pub version_id: String,
    pub applied_at: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CardContent {
    pub title: String,
    pub mechanism: String,
    pub task: String,
    pub conditions: String,
    pub result: String,
    pub limitations: String,
    pub license: String,
    pub consent: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    pub id: String,
    pub record: String,
    pub owner: String,
    pub source_kind: String,
    pub source_id: String,
    pub attempt_id: Option<String>,
    pub content: CardContent,
    pub created_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestAck {
    pub owner: String,
    pub day: String,
    pub fingerprint: String,
    pub at: i64,
}
fn prose(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.contains('\0')
}
fn client(value: &str) -> bool {
    (8..=80).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn title(value: &str) -> bool {
    prose(value, 100) && !value.chars().any(char::is_control)
}
fn backlog_id(record: &str, client: &str) -> String {
    sha256_hex(&format!("research-backlog-v1:{record}:{client}"))
}
fn card_id(card: &Card) -> String {
    digest(&(
        &card.record,
        &card.owner,
        &card.source_kind,
        &card.source_id,
        &card.attempt_id,
        &card.content,
    ))
}
fn card_valid(c: &CardContent) -> bool {
    title(&c.title)
        && prose(&c.mechanism, 2000)
        && prose(&c.task, 2000)
        && prose(&c.conditions, 2000)
        && prose(&c.result, 4000)
        && prose(&c.limitations, 4000)
        && c.consent
        && c.license == LICENSE
}
fn comparison_owned<'a>(
    data: &'a StrategyState,
    id: &str,
    record: &str,
    owner: &str,
) -> Result<&'a comparison::Comparison> {
    data.comparisons
        .get(id)
        .filter(|c| c.content.record == record && c.content.owner == owner)
        .ok_or("comparison_forbidden")
}
fn selected_attempt<'a>(
    data: &'a StrategyState,
    id: &str,
    aid: &str,
    record: &str,
    owner: &str,
) -> Result<(&'a str, i64)> {
    let c = comparison_owned(data, id, record, owner)?;
    let a = c
        .attempts
        .iter()
        .find(|a| a.id == aid)
        .ok_or("comparison_attempt_forbidden")?;
    let d = a.decision.as_ref().ok_or("comparison_decision_required")?;
    let version = match d.outcome.as_str() {
        "keep_baseline" => &c.content.baseline_version,
        "adopt_candidate" => &c.content.candidate_version,
        _ => return Err("comparison_decision_inconclusive"),
    };
    Ok((version, d.at))
}
pub fn selected_version<'a>(data: &'a StrategyState, record: &str, owner: &str) -> Option<&'a str> {
    let a = data.research.applied.get(record)?;
    selected_attempt(data, &a.comparison_id, &a.attempt_id, record, owner)
        .ok()
        .map(|(v, _)| v)
}
pub(super) fn blocks_start(data: &StrategyState, comparison: &str) -> bool {
    data.research
        .rounds
        .get(comparison)
        .is_some_and(|r| r.closed_at.is_none_or(|at| at > now()))
}
fn card_view(c: &Card) -> Value {
    json!({"id":c.id,"title":c.content.title,"mechanism":c.content.mechanism,"task":c.content.task,"conditions":c.content.conditions,"result":c.content.result,"limitations":c.content.limitations,"license":c.content.license,"evidence_status":if c.source_kind=="local"{"owner_reported_local"}else{"model"},"created_at":c.created_at})
}
fn backlog_view(b: &Backlog) -> Value {
    json!({"id":b.id,"client_id":b.client_id,"title":b.title,"hypothesis":b.hypothesis,"priority":b.priority,"comparison_id":b.comparison_id,"source_card_id":b.source_card_id,"created_at":b.created_at,"updated_at":b.updated_at})
}
fn round_visible(c: &comparison::Comparison, r: &ForecastRound, record: &str, owner: &str) -> bool {
    c.content.record == record && c.content.owner == owner || r.participants.contains(owner)
}
fn forecast_summary(r: &ForecastRound) -> Value {
    if r.closed_at.is_none() {
        return Value::Null;
    }
    let values: Vec<u16> = r.forecasts.values().map(|f| f.probability_bps).collect();
    // The sealed round must contain at least one response before closure.
    let mean = values.iter().map(|v| u64::from(*v)).sum::<u64>() / values.len().max(1) as u64;
    json!({"submitted_count":values.len(),"missing_count":r.participants.len()-values.len(),"min_probability_bps":values.iter().min(),"max_probability_bps":values.iter().max(),"mean_probability_bps":mean,"mean_rounding":"floor"})
}
fn round_view(c: &comparison::Comparison, r: &ForecastRound, record: &str, owner: &str) -> Value {
    let forecasts = r.closed_at.map(|_| r.forecasts.iter().map(|(wallet,f)| json!({"wallet":wallet,"probability_bps":f.probability_bps,"reason":f.reason,"submitted_at":f.submitted_at})).collect::<Vec<_>>());
    json!({"comparison_id":r.comparison_id,"forecast_subject":"candidate_satisfies_decision_criterion","summary":forecast_summary(r),"hypothesis":c.content.hypothesis,"decision_criterion":c.content.decision_criterion,"is_owner":c.content.record==record&&c.content.owner==owner,"participants":r.participants,"submitted_count":r.forecasts.len(),"missing_count":r.participants.len()-r.forecasts.len(),"created_at":r.created_at,"closed_at":r.closed_at,"own_forecast":r.forecasts.get(owner),"forecasts":forecasts})
}
// Gregorian civil date from Unix days. Almaty uses UTC+5; no server-local TZ dependency.
fn day(at: i64) -> String {
    let z = (at + 5 * 3600).div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    format!("{:04}-{:02}-{:02}", y + i64::from(m <= 2), m, d)
}
fn digest_parts(data: &StrategyState, record: &str, owner: &str) -> (Vec<Value>, Vec<Value>) {
    let mut items = Vec::new();
    let mut next = Vec::new();
    for (id, r) in &data.research.rounds {
        let c = &data.comparisons[id];
        if !round_visible(c, r, record, owner) {
            continue;
        }
        if let Some(at) = r.closed_at {
            items.push(json!({"kind":"forecasts_closed","comparison_id":id,"attempt_id":null,"title":"Independent forecasts are available","summary":forecast_summary(r),"at":at}));
        } else if r.participants.contains(owner) && !r.forecasts.contains_key(owner) {
            next.push(json!({"kind":"submit_forecast","comparison_id":id,"title":"Record your independent forecast"}));
        } else if c.content.record == record && c.content.owner == owner {
            next.push(json!({"kind":"close_forecasts","comparison_id":id,"title":"Close the forecast round before running"}));
        }
    }
    for c in data
        .comparisons
        .values()
        .filter(|c| c.content.record == record && c.content.owner == owner)
    {
        for a in &c.attempts {
            if let Some(d) = &a.decision {
                items.push(json!({"kind":"decision_recorded","comparison_id":c.id,"attempt_id":a.id,"title":"Owner decision recorded","at":d.at}));
                if d.outcome != "inconclusive"
                    && !data
                        .research
                        .applied
                        .get(record)
                        .is_some_and(|x| x.attempt_id == a.id)
                {
                    next.push(json!({"kind":"apply_decision","comparison_id":c.id,"attempt_id":a.id,"title":"Choose whether to use this version for future runs"}));
                }
            } else if a.baseline.status == "completed" && a.candidate.status == "completed" {
                let at = [&a.baseline, &a.candidate]
                    .iter()
                    .filter_map(|r| r.result.as_ref().and_then(|v| v["finished_at"].as_i64()))
                    .max()
                    .unwrap_or(a.created_at);
                items.push(json!({"kind":"comparison_completed","comparison_id":c.id,"attempt_id":a.id,"title":"Comparison is ready for your decision","at":at}));
                next.push(json!({"kind":"decide","comparison_id":c.id,"attempt_id":a.id,"title":"Record your decision"}));
            }
        }
    }
    for b in data
        .research
        .backlog
        .values()
        .filter(|b| b.record == record && b.owner == owner && b.priority == "now")
    {
        next.push(json!({"kind":"backlog_now","comparison_id":b.comparison_id,"backlog_id":b.id,"title":b.title}));
    }
    items.sort_by_key(|i| i["at"].as_i64().unwrap_or(0));
    (items, next)
}
fn owned_view(data: &StrategyState, record: &str, owner: &str, at: i64) -> Value {
    let (items, next_actions) = digest_parts(data, record, owner);
    let fingerprint = digest(&(&items, &next_actions));
    let today = day(at);
    let unread =
        (!items.is_empty() || !next_actions.is_empty())
            && !data.research.acknowledgements.get(record).is_some_and(|a| {
                a.owner == owner && a.day == today && a.fingerprint == fingerprint
            });
    json!({"ok":true,
        "backlog":data.research.backlog.values().filter(|b|b.record==record&&b.owner==owner).map(backlog_view).collect::<Vec<_>>(),
        "rounds":data.research.rounds.iter().filter_map(|(id,r)|{let c=&data.comparisons[id];round_visible(c,r,record,owner).then(||round_view(c,r,record,owner))}).collect::<Vec<_>>(),
        "applied":data.research.applied.get(record).filter(|a|comparison_owned(data,&a.comparison_id,record,owner).is_ok()),
        "digest":{"day":today,"timezone":"Asia/Almaty","fingerprint":fingerprint,"unread":unread,"items":items,"next_actions":next_actions},
        "library":data.research.cards.values().map(card_view).collect::<Vec<_>>()})
}
pub fn owned(state: &AppState, record: &str, bearer: &str) -> Value {
    let owner = match owner_auth::owner_session_for(state, record, bearer) {
        Ok(s) => s.wallet,
        Err(e) => return err(e),
    };
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    owned_view(&data, record, &owner, now())
}
pub fn public(state: &AppState) -> Value {
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    json!({"ok":true,"cards":data.research.cards.values().map(card_view).collect::<Vec<_>>()})
}
fn required<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body[key].as_str().ok_or("bad_research_request")
}
fn exact(body: &Value, keys: &[&str]) -> Result<()> {
    fields(body, keys)?;
    if keys.iter().any(|k| body.get(k).is_none()) {
        return Err("bad_research_request");
    }
    Ok(())
}
pub fn action(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    action: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(e) => return err(e),
    };
    let registered: BTreeSet<String> = if action == "round-open" {
        state
            .registrations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|r| r.receipt.is_some())
            .map(|r| r.wallet.clone())
            .collect()
    } else {
        BTreeSet::new()
    };
    mutate(state, |data| {
        let at = now();
        match action {
            "backlog-upsert" => {
                exact(
                    body,
                    &[
                        "client_id",
                        "title",
                        "hypothesis",
                        "priority",
                        "comparison_id",
                    ],
                )?;
                let cid = required(body, "client_id")?;
                let headline = required(body, "title")?;
                let hypothesis = required(body, "hypothesis")?;
                let priority = required(body, "priority")?;
                if !client(cid)
                    || !title(headline)
                    || !prose(hypothesis, 2000)
                    || !matches!(priority, "now" | "later")
                {
                    return Err("bad_research_backlog");
                }
                let comparison = if body["comparison_id"].is_null() {
                    None
                } else {
                    let id = required(body, "comparison_id")?;
                    comparison_owned(data, id, record, &owner)?;
                    Some(id.to_string())
                };
                let id = backlog_id(record, cid);
                let prior = data.research.backlog.get(&id);
                if prior.is_none() && data.research.backlog.len() >= MAX_ITEMS {
                    return Err("strategy_capacity");
                }
                if prior.is_some_and(|p| p.owner != owner) {
                    return Err("research_forbidden");
                }
                let b = Backlog {
                    id: id.clone(),
                    record: record.into(),
                    owner: owner.clone(),
                    client_id: cid.into(),
                    title: headline.into(),
                    hypothesis: hypothesis.into(),
                    priority: priority.into(),
                    comparison_id: comparison,
                    source_card_id: prior.and_then(|p| p.source_card_id.clone()),
                    created_at: prior.map_or(at, |p| p.created_at),
                    updated_at: at,
                };
                data.research.backlog.insert(id, b);
            }
            "round-open" => {
                exact(body, &["comparison_id", "participants"])?;
                let id = required(body, "comparison_id")?;
                let c = comparison_owned(data, id, record, &owner)?;
                let roster: Vec<String> = serde_json::from_value(body["participants"].clone())
                    .map_err(|_| "bad_forecast_participants")?;
                let participants: BTreeSet<_> = roster.iter().cloned().collect();
                if roster.is_empty()
                    || roster.len() > MAX_PARTICIPANTS
                    || participants.len() != roster.len()
                    || !participants.is_subset(&registered)
                {
                    return Err("bad_forecast_participants");
                }
                if let Some(r) = data.research.rounds.get(id) {
                    if r.participants != participants {
                        return Err("forecast_round_frozen");
                    }
                } else {
                    if !c.attempts.is_empty() {
                        return Err("forecast_requires_unstarted_comparison");
                    }
                    if data.research.rounds.len() >= MAX_ITEMS {
                        return Err("strategy_capacity");
                    }
                    data.research.rounds.insert(
                        id.into(),
                        ForecastRound {
                            comparison_id: id.into(),
                            participants,
                            forecasts: BTreeMap::new(),
                            created_at: at,
                            closed_at: None,
                        },
                    );
                }
            }
            "forecast" => {
                exact(body, &["comparison_id", "probability_bps", "reason"])?;
                let id = required(body, "comparison_id")?;
                let probability = body["probability_bps"]
                    .as_u64()
                    .filter(|p| *p <= 10000)
                    .ok_or("bad_forecast")? as u16;
                let reason = required(body, "reason")?;
                if !prose(reason, 2000) {
                    return Err("bad_forecast");
                }
                let r = data
                    .research
                    .rounds
                    .get_mut(id)
                    .filter(|r| r.participants.contains(&owner))
                    .ok_or("forecast_forbidden")?;
                if let Some(f) = r.forecasts.get(&owner) {
                    if f.probability_bps != probability || f.reason != reason {
                        return Err("forecast_frozen");
                    }
                } else {
                    if r.closed_at.is_some() {
                        return Err("forecast_round_closed");
                    }
                    r.forecasts.insert(
                        owner.clone(),
                        Forecast {
                            probability_bps: probability,
                            reason: reason.into(),
                            submitted_at: at,
                        },
                    );
                }
            }
            "round-close" => {
                exact(body, &["comparison_id"])?;
                let id = required(body, "comparison_id")?;
                comparison_owned(data, id, record, &owner)?;
                let r = data
                    .research
                    .rounds
                    .get_mut(id)
                    .ok_or("unknown_forecast_round")?;
                if r.forecasts.is_empty() {
                    return Err("forecast_required");
                }
                if r.closed_at.is_none() {
                    r.closed_at = Some(at);
                }
            }
            "digest-ack" => {
                exact(body, &["day", "fingerprint"])?;
                let today = day(at);
                if required(body, "day")? != today {
                    return Err("digest_day_changed");
                }
                let (items, next) = digest_parts(data, record, &owner);
                let fingerprint = digest(&(&items, &next));
                if required(body, "fingerprint")? != fingerprint {
                    return Err("digest_changed");
                }
                if !data.research.acknowledgements.contains_key(record)
                    && data.research.acknowledgements.len() >= MAX_ITEMS
                {
                    return Err("strategy_capacity");
                }
                data.research.acknowledgements.insert(
                    record.into(),
                    DigestAck {
                        owner: owner.clone(),
                        day: today,
                        fingerprint,
                        at,
                    },
                );
            }
            "apply" => {
                exact(body, &["comparison_id", "attempt_id"])?;
                let id = required(body, "comparison_id")?;
                let aid = required(body, "attempt_id")?;
                let (version, _) = selected_attempt(data, id, aid, record, &owner)?;
                let applied = Applied {
                    comparison_id: id.into(),
                    attempt_id: aid.into(),
                    version_id: version.into(),
                    applied_at: at,
                };
                if !data.research.applied.contains_key(record)
                    && data.research.applied.len() >= MAX_ITEMS
                {
                    return Err("strategy_capacity");
                }
                if !data
                    .research
                    .applied
                    .get(record)
                    .is_some_and(|a| a.comparison_id == id && a.attempt_id == aid)
                {
                    data.research.applied.insert(record.into(), applied);
                }
            }
            "publish" | "publish-local" => {
                let local = action == "publish-local";
                let mut keys = vec![
                    "title",
                    "mechanism",
                    "task",
                    "conditions",
                    "result",
                    "limitations",
                    "license",
                    "consent",
                ];
                keys.extend(if local {
                    vec!["client_id"]
                } else {
                    vec!["comparison_id", "attempt_id"]
                });
                exact(body, &keys)?;
                let source = required(body, if local { "client_id" } else { "comparison_id" })?;
                let attempt = if local {
                    if !client(source) {
                        return Err("bad_client_id");
                    }
                    None
                } else {
                    let aid = required(body, "attempt_id")?;
                    let c = comparison_owned(data, source, record, &owner)?;
                    let a = c
                        .attempts
                        .iter()
                        .find(|a| a.id == aid)
                        .ok_or("comparison_attempt_forbidden")?;
                    if a.decision.is_none() {
                        return Err("comparison_decision_required");
                    }
                    Some(aid.to_string())
                };
                let mut content = body.clone();
                let obj = content.as_object_mut().ok_or("bad_research_request")?;
                for k in ["client_id", "comparison_id", "attempt_id"] {
                    obj.remove(k);
                }
                let content: CardContent =
                    serde_json::from_value(content).map_err(|_| "bad_mechanism_card")?;
                if !card_valid(&content) {
                    return Err("bad_mechanism_card");
                }
                let mut card = Card {
                    id: String::new(),
                    record: record.into(),
                    owner: owner.clone(),
                    source_kind: if local { "local" } else { "comparison" }.into(),
                    source_id: source.into(),
                    attempt_id: attempt,
                    content,
                    created_at: at,
                };
                card.id = card_id(&card);
                if let Some(old) = data.research.cards.values().find(|c| {
                    c.record == record
                        && c.source_kind == card.source_kind
                        && c.source_id == source
                        && c.attempt_id == card.attempt_id
                }) {
                    if old.id != card.id {
                        return Err("publication_frozen");
                    }
                } else {
                    if data.research.cards.len() >= MAX_ITEMS {
                        return Err("strategy_capacity");
                    }
                    data.research.cards.insert(card.id.clone(), card);
                }
            }
            "library-reuse" => {
                exact(body, &["card_id", "client_id"])?;
                let card = required(body, "card_id")?;
                let cid = required(body, "client_id")?;
                if !client(cid) {
                    return Err("bad_client_id");
                }
                let c = data
                    .research
                    .cards
                    .get(card)
                    .ok_or("unknown_mechanism_card")?;
                let id = backlog_id(record, cid);
                if let Some(old) = data.research.backlog.get(&id) {
                    if old.owner != owner || old.source_card_id.as_deref() != Some(card) {
                        return Err("idempotency_conflict");
                    }
                } else {
                    if data.research.backlog.len() >= MAX_ITEMS {
                        return Err("strategy_capacity");
                    }
                    let b = Backlog {
                        id: id.clone(),
                        record: record.into(),
                        owner: owner.clone(),
                        client_id: cid.into(),
                        title: c.content.title.clone(),
                        hypothesis: c.content.mechanism.clone(),
                        priority: "later".into(),
                        comparison_id: None,
                        source_card_id: Some(card.into()),
                        created_at: at,
                        updated_at: at,
                    };
                    data.research.backlog.insert(id, b);
                }
            }
            _ => return Err("bad_research_request"),
        }
        Ok(owned_view(data, record, &owner, at))
    })
}

pub(super) fn validate(data: &StrategyState) -> bool {
    let s = &data.research;
    if [
        s.backlog.len(),
        s.rounds.len(),
        s.applied.len(),
        s.cards.len(),
        s.acknowledgements.len(),
    ]
    .iter()
    .any(|n| *n > MAX_ITEMS)
    {
        return false;
    }
    for (id, b) in &s.backlog {
        if id != &b.id
            || *id != backlog_id(&b.record, &b.client_id)
            || !hex32(&b.record)
            || crate::registration::validate_wallet(&b.owner).is_err()
            || !client(&b.client_id)
            || !title(&b.title)
            || !prose(&b.hypothesis, 2000)
            || !matches!(b.priority.as_str(), "now" | "later")
            || b.created_at < 0
            || b.updated_at < b.created_at
            || b.comparison_id
                .as_ref()
                .is_some_and(|id| comparison_owned(data, id, &b.record, &b.owner).is_err())
            || b.source_card_id
                .as_ref()
                .is_some_and(|id| s.cards.get(id).is_none_or(|c| c.created_at > b.created_at))
        {
            return false;
        }
    }
    for (id, r) in &s.rounds {
        let Some(c) = data.comparisons.get(id) else {
            return false;
        };
        if id != &r.comparison_id
            || r.created_at < c.created_at
            || r.participants.is_empty()
            || r.participants.len() > MAX_PARTICIPANTS
            || r.participants
                .iter()
                .any(|w| crate::registration::validate_wallet(w).is_err())
            || r.forecasts.keys().any(|w| !r.participants.contains(w))
            || r.forecasts.values().any(|f| {
                f.probability_bps > 10000
                    || !prose(&f.reason, 2000)
                    || f.submitted_at < r.created_at
                    || r.closed_at.is_some_and(|at| f.submitted_at > at)
            })
        {
            return false;
        }
        match r.closed_at {
            None if !c.attempts.is_empty() => return false,
            Some(at)
                if !(0..=253402300799).contains(&at)
                    || at < r.created_at
                    || r.forecasts.is_empty()
                    || c.attempts.iter().any(|a| a.created_at < at) =>
            {
                return false
            }
            _ => (),
        }
    }
    for (record, a) in &s.applied {
        let Some(c) = data.comparisons.get(&a.comparison_id) else {
            return false;
        };
        let Ok((version, decision_at)) = selected_attempt(
            data,
            &a.comparison_id,
            &a.attempt_id,
            record,
            &c.content.owner,
        ) else {
            return false;
        };
        if a.version_id != version || a.applied_at < decision_at {
            return false;
        }
    }
    let mut sources = BTreeSet::new();
    for (id, c) in &s.cards {
        if id != &c.id
            || *id != card_id(c)
            || !hex32(&c.record)
            || crate::registration::validate_wallet(&c.owner).is_err()
            || !card_valid(&c.content)
            || c.created_at < 0
            || !sources.insert((&c.record, &c.source_kind, &c.source_id, &c.attempt_id))
        {
            return false;
        }
        match c.source_kind.as_str() {
            "local" if client(&c.source_id) && c.attempt_id.is_none() => (),
            "comparison" => {
                let Ok(comparison) = comparison_owned(data, &c.source_id, &c.record, &c.owner)
                else {
                    return false;
                };
                if !comparison.attempts.iter().any(|a| {
                    Some(&a.id) == c.attempt_id.as_ref()
                        && a.decision.as_ref().is_some_and(|d| d.at <= c.created_at)
                }) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    for (record, a) in &s.acknowledgements {
        if !hex32(record)
            || crate::registration::validate_wallet(&a.owner).is_err()
            || !hex32(&a.fingerprint)
            || !(0..=253402300799).contains(&a.at)
            || a.day != day(a.at)
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::tests::{actor, brief, fixture, plan};
    use super::*;
    use crate::api::{load_snapshot, new_state_with_files};
    const ORIGIN: &str = "https://alashi.network";
    fn act(s: &AppState, who: usize, action_name: &str, body: Value) -> Value {
        let (record, bearer) = actor(who);
        action(s, &record, ORIGIN, &bearer, action_name, &body)
    }
    fn view(s: &AppState, who: usize) -> Value {
        let (record, bearer) = actor(who);
        owned(s, &record, &bearer)
    }
    fn wallet(s: &AppState, who: usize) -> String {
        s.registrations.lock().unwrap()[&actor(who).0]
            .wallet
            .clone()
    }
    fn setup(s: &AppState) -> String {
        let (record, bearer) = actor(0);
        let challenge = brief(s, "research-example", 10000);
        let mut p = plan();
        let a = super::super::create(
            s,
            &record,
            ORIGIN,
            &bearer,
            &json!({"parent":null,"title":"PRIVATE PLAN","plan":p,"change":"","published":false}),
        );
        let baseline = a["version"]["id"].as_str().unwrap();
        p[0]["instruction"] = json!("PRIVATE CANDIDATE instruction");
        let b = super::super::create(
            s,
            &record,
            ORIGIN,
            &bearer,
            &json!({"parent":baseline,"title":"PRIVATE PLAN","plan":p,"change":"PRIVATE CHANGE","published":false}),
        );
        let c = comparison::create(
            s,
            &record,
            ORIGIN,
            &bearer,
            &json!({"challenge_id":challenge,"baseline_version":baseline,"candidate_version":b["version"]["id"],"hypothesis":"Shared hypothesis","prediction":"PRIVATE PREDICTION","decision_criterion":"Shared decision criterion","changed_task":"research","profile":{"source":"test_provider","provider":"local-fixture","model":"deterministic-test","temperature":0,"max_tokens":256},"attempt_limit":2}),
        );
        assert_eq!(c["ok"], true, "{c}");
        c["comparison"]["id"].as_str().unwrap().into()
    }
    fn start(s: &AppState, c: &str) -> Value {
        let (record, bearer) = actor(0);
        comparison::start(
            s,
            &record,
            ORIGIN,
            &bearer,
            c,
            &json!({"client_id":"research-attempt-01"}),
        )
    }
    fn forecast(s: &AppState, c: &str, who: usize) -> Value {
        act(
            s,
            who,
            "forecast",
            json!({"comparison_id":c,"probability_bps":6000,"reason":format!("Sealed reason from {who}")}),
        )
    }
    fn complete(s: &AppState, c: &str, aid: &str, label: &str) {
        let (record, bearer) = actor(0);
        for task in ["research", "deliver"] {
            let reserve = comparison::action(
                s,
                &record,
                ORIGIN,
                &bearer,
                c,
                aid,
                label,
                "reserve",
                &json!({"task":task,"execution_id":"a".repeat(64),"max_total_tokens":100,"source":"test_provider","provider":"local-fixture","model":"deterministic-test"}),
            );
            assert_eq!(reserve["ok"], true, "{reserve}");
            let report = if task == "deliver" {
                json!({"headline":"Build with ALASHI","body":"Bring your agents and tools to a shared challenge. Spend model tokens on your plan and compare the evidence.","cta":"Explore ALASHI"})
            } else {
                json!({"facts":"PRIVATE REPORT"})
            };
            let reported = comparison::action(
                s,
                &record,
                ORIGIN,
                &bearer,
                c,
                aid,
                label,
                "report",
                &json!({"task":task,"execution_id":"a".repeat(64),"response_id":format!("{aid}-{label}-{task}"),"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"cached_tokens":0},"report":report,"request_failed":false}),
            );
            assert_eq!(reported["ok"], true, "{reported}");
        }
        let done = comparison::action(
            s,
            &record,
            ORIGIN,
            &bearer,
            c,
            aid,
            label,
            "finish",
            &json!({}),
        );
        assert_eq!(done["ok"], true, "{done}");
    }
    fn decide(s: &AppState, c: &str, aid: &str, outcome: &str) {
        let (record, bearer) = actor(0);
        let decision = comparison::decision(
            s,
            &record,
            ORIGIN,
            &bearer,
            c,
            aid,
            &json!({"outcome":outcome,"reason":"PRIVATE DECISION"}),
        );
        assert_eq!(decision["ok"], true, "{decision}");
    }
    fn publication() -> Value {
        json!({"title":"Reviewed mechanism","mechanism":"Use a declared audience","task":"Draft an advertisement","conditions":"Static supplied facts","result":"Both outputs passed format checks","limitations":"No measured sales outcome","consent":true,"license":"CC0-1.0"})
    }
    #[test]
    fn sealed_forecasts_are_role_projected_immutable_and_block_execution_until_close() {
        let s = fixture();
        let c = setup(&s);
        let open =
            json!({"comparison_id":c,"participants":[wallet(&s,0),wallet(&s,1),wallet(&s,2)]});
        assert_eq!(act(&s, 0, "round-open", open.clone())["ok"], true);
        assert_eq!(
            act(&s, 1, "round-open", open.clone())["error"],
            "comparison_forbidden"
        );
        assert_eq!(start(&s, &c)["error"], "forecast_round_open");
        assert_eq!(
            act(&s, 0, "round-close", json!({"comparison_id":c}))["error"],
            "forecast_required"
        );
        assert_eq!(forecast(&s, &c, 1)["ok"], true);
        assert_eq!(forecast(&s, &c, 1)["ok"], true);
        assert_eq!(forecast(&s, &c, 3)["error"], "forecast_forbidden");
        for who in [0, 1, 2] {
            let v = view(&s, who);
            assert_eq!(v["rounds"][0]["forecasts"], Value::Null);
            assert_eq!(v["rounds"][0]["summary"], Value::Null);
            assert_eq!(v["rounds"][0]["submitted_count"], 1);
            assert_eq!(v["rounds"][0]["own_forecast"].is_null(), who != 1);
            let wire = v.to_string();
            for private in [
                "PRIVATE",
                "actual_diff",
                "baseline_version",
                "prediction",
                "plan",
                "response_id",
            ] {
                assert!(!wire.contains(private), "leaked {private}: {wire}");
            }
            if who != 1 {
                assert!(!wire.contains("Sealed reason from 1"));
            }
        }
        assert!(view(&s, 3)["rounds"].as_array().unwrap().is_empty());
        assert_eq!(
            act(&s, 1, "round-close", json!({"comparison_id":c}))["error"],
            "comparison_forbidden"
        );
        let close = act(&s, 0, "round-close", json!({"comparison_id":c}));
        assert_eq!(close["ok"], true);
        assert_eq!(close["rounds"][0]["missing_count"], 2);
        assert_eq!(close["rounds"][0]["summary"]["min_probability_bps"], 6000);
        assert_eq!(close["rounds"][0]["summary"]["mean_probability_bps"], 6000);
        assert_eq!(
            close["rounds"][0]["forecasts"][0]["reason"],
            "Sealed reason from 1"
        );
        assert_eq!(
            act(&s, 0, "round-close", json!({"comparison_id":c}))["rounds"],
            close["rounds"]
        );
        assert_eq!(
            forecast(&s, &c, 1)["ok"],
            true,
            "same request can recover a lost response after closure"
        );
        assert_eq!(forecast(&s, &c, 2)["error"], "forecast_round_closed");
        assert_eq!(
            act(
                &s,
                1,
                "forecast",
                json!({"comparison_id":c,"probability_bps":8000,"reason":"changed"})
            )["error"],
            "forecast_frozen"
        );
        assert_eq!(start(&s, &c)["ok"], true);
        assert_eq!(start(&s, &c)["ok"], true);
        assert_eq!(
            act(&s, 0, "round-open", open)["ok"],
            true,
            "opening retry does not reopen the round"
        );
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert_eq!(view(&s, 0), view(&restored, 0));
        assert!(super::super::validate(
            &restored.strategy_cycle.lock().unwrap()
        ));
    }
    #[test]
    fn forecast_validation_rejects_unknown_wallets_late_rounds_and_missing_fields() {
        let s = fixture();
        let c = setup(&s);
        for participants in [
            json!([]),
            json!([wallet(&s, 1), wallet(&s, 1)]),
            json!(["unregistered"]),
        ] {
            assert_eq!(
                act(
                    &s,
                    0,
                    "round-open",
                    json!({"comparison_id":c,"participants":participants})
                )["error"],
                "bad_forecast_participants"
            );
        }
        assert_eq!(
            act(&s, 0, "round-open", json!({"comparison_id":c}))["error"],
            "bad_research_request"
        );
        assert_eq!(start(&s, &c)["ok"], true);
        assert_eq!(
            act(
                &s,
                0,
                "round-open",
                json!({"comparison_id":c,"participants":[wallet(&s,1)]})
            )["error"],
            "forecast_requires_unstarted_comparison"
        );
        let (record, bearer) = actor(0);
        assert_eq!(
            action(
                &s,
                &record,
                "https://attacker.example",
                &bearer,
                "digest-ack",
                &json!({"day":day(now()),"fingerprint":view(&s,0)["digest"]["fingerprint"]})
            )["error"],
            "owner_origin_forbidden"
        );
        assert_eq!(owned(&s, &record, &"0".repeat(64))["ok"], false);
    }
    #[test]
    fn backlog_digest_applied_version_and_reviewed_public_card_survive_reload() {
        let s = fixture();
        let c = setup(&s);
        let backlog = json!({"client_id":"backlog-0001","title":"PRIVATE IDEA","hypothesis":"PRIVATE HYPOTHESIS","priority":"now","comparison_id":c});
        assert_eq!(act(&s, 0, "backlog-upsert", backlog.clone())["ok"], true);
        assert!(view(&s, 1)["backlog"].as_array().unwrap().is_empty());
        let today = view(&s, 0)["digest"]["day"].clone();
        assert_eq!(
            act(
                &s,
                0,
                "digest-ack",
                json!({"day":today,"fingerprint":view(&s,0)["digest"]["fingerprint"]})
            )["digest"]["unread"],
            false
        );
        let a = start(&s, &c);
        let aid = a["attempt"]["id"].as_str().unwrap();
        assert_eq!(
            act(&s, 0, "apply", json!({"comparison_id":c,"attempt_id":aid}))["error"],
            "comparison_decision_required"
        );
        complete(&s, &c, aid, "baseline");
        complete(&s, &c, aid, "candidate");
        assert_eq!(view(&s, 0)["digest"]["unread"], true);
        decide(&s, &c, aid, "adopt_candidate");
        let applied = act(&s, 0, "apply", json!({"comparison_id":c,"attempt_id":aid}));
        assert_eq!(applied["ok"], true, "{applied}");
        let selected = applied["applied"]["version_id"].as_str().unwrap();
        assert_eq!(
            selected,
            s.strategy_cycle.lock().unwrap().comparisons[&c]
                .content
                .candidate_version
        );
        let owner = wallet(&s, 0);
        assert_eq!(
            selected_version(&s.strategy_cycle.lock().unwrap(), &actor(0).0, &owner),
            Some(selected)
        );
        assert_eq!(
            selected_version(
                &s.strategy_cycle.lock().unwrap(),
                &actor(0).0,
                &wallet(&s, 1)
            ),
            None
        );
        assert_eq!(
            act(&s, 1, "apply", json!({"comparison_id":c,"attempt_id":aid}))["error"],
            "comparison_forbidden"
        );
        let mut publish = publication();
        publish["comparison_id"] = json!(c);
        publish["attempt_id"] = json!(aid);
        let mut unreviewed = publish.clone();
        unreviewed["consent"] = json!(false);
        assert_eq!(
            act(&s, 0, "publish", unreviewed)["error"],
            "bad_mechanism_card"
        );
        assert_eq!(act(&s, 0, "publish", publish.clone())["ok"], true);
        assert_eq!(act(&s, 0, "publish", publish)["ok"], true);
        let library = public(&s);
        let card = library["cards"][0]["id"].as_str().unwrap();
        assert_eq!(library["cards"][0]["evidence_status"], "model");
        let wire = library.to_string();
        for private in [
            "PRIVATE",
            &owner,
            &actor(0).0,
            &c,
            aid,
            "source_id",
            "comparison_id",
            "attempt_id",
            "consent",
        ] {
            assert!(!wire.contains(private), "public leak {private}: {wire}");
        }
        let reuse = act(
            &s,
            1,
            "library-reuse",
            json!({"card_id":card,"client_id":"reuse-0001"}),
        );
        assert_eq!(reuse["backlog"][0]["source_card_id"], card);
        assert_eq!(reuse["backlog"][0]["priority"], "later");
        assert_eq!(
            act(
                &s,
                1,
                "library-reuse",
                json!({"card_id":card,"client_id":"reuse-0001"})
            )["backlog"],
            reuse["backlog"]
        );
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert_eq!(view(&s, 0), view(&restored, 0));
        assert_eq!(public(&s), public(&restored));
        assert!(super::super::validate(
            &restored.strategy_cycle.lock().unwrap()
        ));
    }
    #[test]
    fn local_publication_has_explicit_boundary_and_source_receipt_is_private() {
        let s = fixture();
        let mut p = publication();
        p["client_id"] = json!("private-local-run-0001");
        assert_eq!(act(&s, 0, "publish-local", p.clone())["ok"], true);
        assert_eq!(act(&s, 0, "publish-local", p.clone())["ok"], true);
        let out = public(&s);
        assert_eq!(out["cards"][0]["evidence_status"], "owner_reported_local");
        assert!(!out.to_string().contains("private-local-run-0001"));
        p["result"] = json!("Changed result");
        assert_eq!(
            act(&s, 0, "publish-local", p)["error"],
            "publication_frozen"
        );
        let missing = json!({"client_id":"another-local-0001","title":"T","mechanism":"M","task":"T","conditions":"C","result":"R","limitations":"L","license":"CC0-1.0"});
        assert_eq!(
            act(&s, 0, "publish-local", missing)["error"],
            "bad_research_request"
        );
        assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
    }
    #[test]
    fn snapshot_rejects_forecast_temporal_tampering_and_old_snapshots_default_safely() {
        let s = fixture();
        let c = setup(&s);
        assert_eq!(
            act(
                &s,
                0,
                "round-open",
                json!({"comparison_id":c,"participants":[wallet(&s,1)]})
            )["ok"],
            true
        );
        assert_eq!(forecast(&s, &c, 1)["ok"], true);
        assert_eq!(
            act(&s, 0, "round-close", json!({"comparison_id":c}))["ok"],
            true
        );
        assert_eq!(start(&s, &c)["ok"], true);
        let original = s.strategy_cycle.lock().unwrap().clone();
        assert!(super::super::validate(&original));
        for mutation in 0..4 {
            let mut bad = original.clone();
            let r = bad.research.rounds.get_mut(&c).unwrap();
            match mutation {
                0 => r.closed_at = None,
                1 => r.closed_at = Some(i64::MAX),
                2 => r.forecasts.values_mut().next().unwrap().submitted_at = i64::MAX,
                _ => r.participants.clear(),
            }
            assert!(!super::super::validate(&bad), "mutation {mutation}");
        }
        // A clock correction after persistence must not invalidate the whole
        // snapshot. Execution still waits until time reaches the saved cutoff.
        let mut clock_rollback = original.clone();
        clock_rollback
            .comparisons
            .get_mut(&c)
            .unwrap()
            .attempts
            .clear();
        clock_rollback
            .research
            .rounds
            .get_mut(&c)
            .unwrap()
            .closed_at = Some(now() + 60);
        assert!(super::super::validate(&clock_rollback));
        assert!(blocks_start(&clock_rollback, &c));
        let mut future_close = original.clone();
        future_close
            .comparisons
            .get_mut(&c)
            .unwrap()
            .attempts
            .clear();
        future_close.research.rounds.get_mut(&c).unwrap().closed_at = Some(i64::MAX);
        assert!(!super::super::validate(&future_close));
        assert!(blocks_start(&future_close, &c));
        let mut old = serde_json::to_value(StrategyState::default()).unwrap();
        old.as_object_mut().unwrap().remove("research");
        assert!(super::super::validate(
            &serde_json::from_value(old).unwrap()
        ));
        let mut malformed = serde_json::to_value(&original).unwrap();
        malformed["research"]
            .as_object_mut()
            .unwrap()
            .remove("rounds");
        assert!(serde_json::from_value::<StrategyState>(malformed).is_err());
    }
    #[test]
    fn inconclusive_decision_can_be_shared_but_cannot_select_a_version() {
        let s = fixture();
        let c = setup(&s);
        let a = start(&s, &c);
        let aid = a["attempt"]["id"].as_str().unwrap();
        complete(&s, &c, aid, "baseline");
        complete(&s, &c, aid, "candidate");
        decide(&s, &c, aid, "inconclusive");
        assert_eq!(
            act(&s, 0, "apply", json!({"comparison_id":c,"attempt_id":aid}))["error"],
            "comparison_decision_inconclusive"
        );
        assert!(view(&s, 0)["applied"].is_null());
        let mut p = publication();
        p["comparison_id"] = json!(c);
        p["attempt_id"] = json!(aid);
        p["result"] = json!("Inconclusive; no version selected");
        assert_eq!(act(&s, 0, "publish", p)["ok"], true);
        assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
    }
    #[test]
    fn closed_summary_reports_disagreement_and_forecast_input_bounds() {
        let s = fixture();
        let c = setup(&s);
        assert_eq!(
            act(
                &s,
                0,
                "round-open",
                json!({"comparison_id":c,"participants":[wallet(&s, 1),wallet(&s, 2)]})
            )["ok"],
            true
        );
        for bad in [json!(-1), json!(10001), json!(1.5), json!(null)] {
            assert_eq!(
                act(
                    &s,
                    1,
                    "forecast",
                    json!({"comparison_id":c,"probability_bps":bad,"reason":"Reason"})
                )["error"],
                "bad_forecast"
            );
        }
        assert_eq!(
            act(
                &s,
                1,
                "forecast",
                json!({"comparison_id":c,"probability_bps":0,"reason":"No"})
            )["ok"],
            true
        );
        assert_eq!(
            act(
                &s,
                2,
                "forecast",
                json!({"comparison_id":c,"probability_bps":9999,"reason":"Almost certain"})
            )["ok"],
            true
        );
        let out = act(&s, 0, "round-close", json!({"comparison_id":c}));
        let summary = &out["rounds"][0]["summary"];
        assert_eq!(summary["min_probability_bps"], 0);
        assert_eq!(summary["max_probability_bps"], 9999);
        assert_eq!(summary["mean_probability_bps"], 4999);
        assert_eq!(summary["mean_rounding"], "floor");
        assert_eq!(out["digest"]["items"][0]["summary"], *summary);
    }
    #[test]
    fn failed_persistence_rolls_back_private_state_and_publication() {
        let s = fixture();
        let saved = std::fs::read(&s.snapshot_path).unwrap();
        std::fs::remove_file(&s.snapshot_path).unwrap();
        std::fs::create_dir(&s.snapshot_path).unwrap();
        let mut p = publication();
        p["client_id"] = json!("failed-local-0001");
        assert_eq!(act(&s, 0, "publish-local", p)["error"], "storage_failed");
        let b = json!({"client_id":"failed-backlog-01","title":"Idea","hypothesis":"Question","priority":"now","comparison_id":null});
        assert_eq!(act(&s, 0, "backlog-upsert", b)["error"], "storage_failed");
        assert!(public(&s)["cards"].as_array().unwrap().is_empty());
        assert!(view(&s, 0)["backlog"].as_array().unwrap().is_empty());
        std::fs::remove_dir(&s.snapshot_path).unwrap();
        std::fs::write(&s.snapshot_path, saved).unwrap();
    }
    #[test]
    fn digest_timezone_and_same_day_new_information_are_explicit() {
        assert_eq!(day(0), "1970-01-01");
        assert_eq!(day(86400 - 5 * 3600 - 1), "1970-01-01");
        assert_eq!(day(86400 - 5 * 3600), "1970-01-02");
        let s = fixture();
        let first = json!({"client_id":"digest-backlog-01","title":"First idea","hypothesis":"Question","priority":"now","comparison_id":null});
        assert_eq!(
            act(&s, 0, "backlog-upsert", first)["digest"]["unread"],
            true
        );
        assert_eq!(
            act(
                &s,
                0,
                "digest-ack",
                json!({"day":day(now()),"fingerprint":view(&s,0)["digest"]["fingerprint"]})
            )["digest"]["unread"],
            false
        );
        assert_eq!(
            act(
                &s,
                0,
                "digest-ack",
                json!({"day":"1970-01-01","fingerprint":view(&s,0)["digest"]["fingerprint"]})
            )["error"],
            "digest_day_changed"
        );
        let acknowledged = view(&s, 0)["digest"].clone();
        let second = json!({"client_id":"digest-backlog-02","title":"Second idea","hypothesis":"Another question","priority":"now","comparison_id":null});
        assert_eq!(
            act(&s, 0, "backlog-upsert", second)["digest"]["unread"],
            true
        );
        assert_eq!(
            act(
                &s,
                0,
                "digest-ack",
                json!({"day":acknowledged["day"],"fingerprint":acknowledged["fingerprint"]})
            )["error"],
            "digest_changed"
        );
        assert_eq!(view(&s, 0)["digest"]["unread"], true);
    }
}
