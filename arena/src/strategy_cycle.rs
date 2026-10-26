//! Owner task graphs, usage receipts and reusable strategies for a shared challenge.
use super::{
    hex32, now, owner_auth, phase_name, save_snapshot_locked, sha256_hex, strategy_trade, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, &'static str>;
const MAX_BUDGET: u64 = 1_000_000;
// shortcut: bounded pilot journal; add receipt-preserving archival before raising this cap.
const MAX_ITEMS: usize = 500;
#[path = "strategy_comparison.rs"]
pub mod comparison;
#[path = "strategy_research.rs"]
pub mod research;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub id: String,
    pub kind: String,
    pub depends_on: Vec<String>,
    pub instruction: String,
    pub tool: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StrategyContent {
    pub owner: String,
    pub parent: Option<String>,
    pub title: String,
    pub plan: Vec<TaskSpec>,
    pub change: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub id: String,
    pub content: StrategyContent,
    pub published: bool,
    pub created_at: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Brief {
    pub owner: String,
    pub match_key: String,
    pub round_seconds: u16,
    pub title: String,
    pub audience: String,
    pub task: String,
    pub facts: Vec<String>,
    pub required_terms: Vec<String>,
    pub cta: String,
    pub max_words: u16,
    pub budget: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trading: Option<strategy_trade::Context>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub id: String,
    pub content: Brief,
    pub created_at: i64,
    pub started_at: Option<i64>,
}
impl Challenge {
    fn deadline(&self) -> Option<i64> {
        self.started_at.map(|t| {
            (t + self.content.round_seconds as i64).min(
                self.content
                    .trading
                    .as_ref()
                    .map_or(i64::MAX, |c| c.phase_ends_at),
            )
        })
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
}
impl Usage {
    fn valid(&self) -> bool {
        self.prompt_tokens.checked_add(self.completion_tokens) == Some(self.total_tokens)
            && self.cached_tokens <= self.prompt_tokens
            && self.total_tokens <= 10 * MAX_BUDGET
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub task: String,
    // Persist the executor claim, but never expose it in a run view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    pub reserved: u64,
    pub status: String,
    pub source: String,
    pub provider: String,
    pub model: String,
    pub response_id: Option<String>,
    pub usage: Option<Usage>,
    pub report: Option<Value>,
    pub report_hash: Option<String>,
    pub quality_passed: Option<bool>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub seq: u64,
    pub at: i64,
    pub task: String,
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub reviewer: String,
    pub accepted: bool,
    pub reason: String,
    pub at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub seat: u8,
    pub id: String,
    pub owner: String,
    pub record: String,
    pub client_id: String,
    pub version: String,
    pub challenge: String,
    pub budget: u64,
    pub calls: BTreeMap<String, Call>,
    pub events: Vec<Event>,
    pub status: String,
    pub result: Option<Value>,
    pub review: Option<Review>,
    pub created_at: i64,
}
impl Run {
    fn event(&mut self, task: &str, kind: &str) {
        self.events.push(Event {
            seq: self.events.len() as u64 + 1,
            at: now(),
            task: task.into(),
            kind: kind.into(),
        });
    }
    fn known(&self) -> u64 {
        self.calls
            .values()
            .filter_map(|c| c.usage.as_ref())
            .map(|u| u.total_tokens)
            .sum()
    }
    fn held(&self) -> u64 {
        self.calls
            .values()
            .filter(|c| c.usage.is_none())
            .map(|c| c.reserved)
            .sum()
    }
    fn uncertain(&self) -> bool {
        self.calls
            .values()
            .any(|c| c.status != "reserved" && c.usage.is_none())
    }
    fn test(&self) -> bool {
        self.calls.values().any(|c| c.source == "test_provider")
    }
    fn passed(&self, id: &str) -> bool {
        self.calls
            .get(id)
            .is_some_and(|c| c.quality_passed == Some(true) && c.usage.is_some())
    }
    fn approved(&self) -> bool {
        self.status == "completed" && self.review.as_ref().is_some_and(|r| r.accepted)
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyState {
    pub versions: BTreeMap<String, Version>,
    pub challenges: BTreeMap<String, Challenge>,
    pub runs: BTreeMap<String, Run>,
    #[serde(default)]
    pub comparisons: BTreeMap<String, comparison::Comparison>,
    #[serde(default)]
    pub research: research::ResearchState,
}
fn digest(v: &impl Serialize) -> String {
    sha256_hex(&serde_json::to_string(v).expect("serializable content"))
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
fn valid_plan(plan: &[TaskSpec]) -> bool {
    if plan.is_empty()
        || plan.len() > 8
        || plan
            .last()
            .is_none_or(|t| t.id != "deliver" || t.kind != "model")
    {
        return false;
    }
    let mut seen = BTreeSet::new();
    for t in plan {
        if !identifier(&t.id)
            || seen.contains(&t.id)
            || t.instruction.len() > 2000
            || t.instruction.contains('\0')
            || t.depends_on.len() > 8
            || t.depends_on.iter().any(|d| !seen.contains(d))
            || !matches!(t.kind.as_str(), "model" | "tool")
            || (t.kind == "tool" && !t.tool.as_deref().is_some_and(identifier))
            || (t.kind == "model" && t.tool.is_some())
        {
            return false;
        }
        seen.insert(t.id.clone());
    }
    let mut needed = BTreeSet::from(["deliver".to_string()]);
    for t in plan.iter().rev() {
        if needed.contains(&t.id) {
            needed.extend(t.depends_on.iter().cloned());
        }
    }
    needed.len() == plan.len()
}
fn valid_content(c: &StrategyContent) -> bool {
    !c.owner.is_empty()
        && !c.title.trim().is_empty()
        && c.title.len() <= 100
        && !c.title.chars().any(char::is_control)
        && c.change.len() <= 1000
        && (c.parent.is_none() || !c.change.trim().is_empty())
        && valid_plan(&c.plan)
}
fn valid_brief(c: &Brief) -> bool {
    identifier(&c.match_key)
        && (60..=900).contains(&c.round_seconds)
        && !c.title.trim().is_empty()
        && c.title.len() <= 100
        && !c.audience.trim().is_empty()
        && c.audience.len() <= 500
        && !c.task.trim().is_empty()
        && c.task.len() <= 2000
        && (1..=12).contains(&c.facts.len())
        && c.facts
            .iter()
            .all(|f| !f.trim().is_empty() && f.len() <= 500)
        && (if let Some(trading) = &c.trading {
            trading.valid() && c.required_terms.is_empty() && c.cta.is_empty() && c.max_words == 0
        } else {
            (1..=8).contains(&c.required_terms.len())
                && c.required_terms
                    .iter()
                    .all(|t| !t.trim().is_empty() && t.len() <= 80 && t.is_ascii())
                && !c.cta.trim().is_empty()
                && c.cta.len() <= 120
                && c.cta.is_ascii()
                && (20..=200).contains(&c.max_words)
        })
        && c.budget > 0
        && c.budget <= MAX_BUDGET
}
fn checks(brief: &Brief, artifact: &Value) -> Value {
    if let Some(context) = &brief.trading {
        return strategy_trade::checks(context, artifact);
    }
    let keys = artifact.as_object().is_some_and(|o| {
        o.len() == 3
            && ["headline", "body", "cta"]
                .iter()
                .all(|k| o.get(*k).is_some_and(Value::is_string))
    });
    let parts = ["headline", "body", "cta"].map(|k| artifact[k].as_str().unwrap_or(""));
    let text = parts.join(" ");
    let lower = text.to_ascii_lowercase();
    let words = text.split_whitespace().count();
    let language = keys && text.is_ascii() && parts.iter().all(|s| !s.trim().is_empty());
    let length = (10..=brief.max_words as usize).contains(&words)
        && parts[0].split_whitespace().count() <= 12;
    let terms = brief
        .required_terms
        .iter()
        .all(|t| lower.contains(&t.to_ascii_lowercase()));
    let cta = parts[2].trim() == brief.cta;
    let claims = !text.contains('%')
        && !text.contains('$')
        && !["guaranteed", "always wins", "risk-free", "zero cost"]
            .iter()
            .any(|s| lower.contains(s));
    json!({"schema":keys,"english_ascii":language,"length":length,"required_terms":terms,"cta":cta,"no_unsubstantiated_numeric_claims":claims,"word_count":words,"passed":keys && language && length && terms && cta && claims,
        "limit":"format_checks_do_not_establish_creative_quality_or_ad_performance"})
}
pub fn validate(s: &StrategyState) -> bool {
    if s.versions.len() > MAX_ITEMS || s.runs.len() > MAX_ITEMS || s.challenges.len() > MAX_ITEMS {
        return false;
    }
    for (id, v) in &s.versions {
        if id != &v.id
            || *id != digest(&v.content)
            || !valid_content(&v.content)
            || v.content.parent.as_ref().is_some_and(|p| {
                !s.versions.get(p).is_some_and(|parent| {
                    (parent.published || !v.published && parent.content.owner == v.content.owner)
                        && parent.created_at <= v.created_at
                })
            })
        {
            return false;
        }
    }
    for (id, c) in &s.challenges {
        if id != &c.id || *id != digest(&c.content) || !valid_brief(&c.content) {
            return false;
        }
    }
    for challenge in s.challenges.values() {
        let participants: Vec<_> = s
            .runs
            .values()
            .filter(|r| r.challenge == challenge.id)
            .collect();
        let owners: BTreeSet<_> = participants.iter().map(|r| &r.owner).collect();
        let seats: BTreeSet<_> = participants.iter().map(|r| r.seat).collect();
        if participants.len() > 2
            || owners.len() != participants.len()
            || seats != (0..participants.len() as u8).collect()
            || challenge.started_at.is_some() != (participants.len() == 2)
        {
            return false;
        }
    }
    let mut ids = BTreeSet::new();
    for (id, r) in &s.runs {
        let (Some(v), Some(c)) = (s.versions.get(&r.version), s.challenges.get(&r.challenge))
        else {
            return false;
        };
        if id != &r.id
            || *id != sha256_hex(&format!("strategy-run-v2:{}:{}", r.owner, r.client_id))
            || !hex32(&r.record)
            || r.budget != c.content.budget
            || r.owner == c.content.owner && c.content.trading.is_none()
            || (v.content.parent.is_some() && !v.published)
            || (r.owner != v.content.owner && !v.published)
            || !valid_run(r, v, c, &mut ids)
        {
            return false;
        }
        if r.review.as_ref().is_some_and(|review| {
            review.reviewer != c.content.owner
                || review.reviewer == r.owner
                || review.reason.is_empty()
                || review.reason.len() > 1000
        }) {
            return false;
        }
    }
    comparison::validate(s, &mut ids) && research::validate(s)
}
fn valid_run(
    r: &Run,
    v: &Version,
    c: &Challenge,
    ids: &mut BTreeSet<(String, String, String, String)>,
) -> bool {
    if r.calls.len() > v.content.plan.len()
        || r.events.len() > 40
        || !matches!(
            r.status.as_str(),
            "running" | "completed" | "quality_failed" | "usage_unknown" | "budget_overrun"
        )
        || r.events
            .iter()
            .enumerate()
            .any(|(i, e)| e.seq != i as u64 + 1)
    {
        return false;
    }
    for (task, call) in &r.calls {
        let Some(spec) = v.content.plan.iter().find(|t| &t.id == task) else {
            return false;
        };
        if &call.task != task
            || call.execution_id.as_deref().is_some_and(|id| !hex32(id))
            || !valid_call(
                spec,
                call.reserved,
                &call.source,
                &call.provider,
                &call.model,
            )
            || call
                .usage
                .as_ref()
                .is_some_and(|u| !u.valid() || call.source == "local_tool" && u.total_tokens != 0)
        {
            return false;
        }
        if call.status == "reserved" {
            if call.usage.is_some()
                || call.report.is_some()
                || call.finished_at.is_some()
                || call.quality_passed.is_some()
                || call.report_hash.is_some()
                || call.response_id.is_some()
            {
                return false;
            }
        } else if !matches!(call.status.as_str(), "reported" | "failed")
            || call.finished_at.is_none()
            || call.quality_passed.is_none()
            || !call.report_hash.as_deref().is_some_and(hex32)
        {
            return false;
        }
        if call.quality_passed == Some(true)
            && (call
                .report
                .as_ref()
                .is_none_or(|p| !p.is_object() || p.to_string().len() > 8192)
                || call.report_hash.as_deref() != call.report.as_ref().map(digest).as_deref())
        {
            return false;
        }
        if call.quality_passed == Some(false) && call.report.is_some() {
            return false;
        }
        if let Some(response) = &call.response_id {
            if response.is_empty()
                || response.len() > 200
                || !ids.insert((
                    call.source.clone(),
                    call.provider.clone(),
                    call.model.clone(),
                    response.clone(),
                ))
            {
                return false;
            }
        }
    }
    if r.status == "completed" {
        let Some(result) = &r.result else {
            return false;
        };
        if !v.content.plan.iter().all(|t| r.passed(&t.id))
            || checks(&c.content, &result["artifact"])["passed"] != true
            || r.known() > r.budget
            || r.calls.values().any(|call| {
                call.usage
                    .as_ref()
                    .is_some_and(|u| u.total_tokens > call.reserved)
            })
        {
            return false;
        }
        if let Some(context) = &c.content.trading {
            if !strategy_trade::valid_receipt(
                context,
                &r.owner,
                &result["artifact"],
                &result["trade_receipt"],
            ) {
                return false;
            }
        } else if result.get("trade_receipt").is_some() {
            return false;
        }
    } else if r.result.is_some() || r.review.is_some() {
        return false;
    }
    true
}
fn all_runs(s: &StrategyState) -> impl Iterator<Item = &Run> {
    s.runs.values().chain(
        s.comparisons
            .values()
            .flat_map(|c| c.attempts.iter().flat_map(|a| [&a.baseline, &a.candidate])),
    )
}
fn duplicate_receipt(data: &StrategyState, id: &str, body: &Value) -> bool {
    let task = body["task"].as_str().unwrap_or("");
    let Some(reservation) = all_runs(data)
        .find(|r| r.id == id)
        .and_then(|r| r.calls.get(task))
    else {
        return false;
    };
    body["response_id"].as_str().is_some_and(|response| {
        all_runs(data).any(|r| {
            r.calls.iter().any(|(other_task, c)| {
                (r.id != id || other_task != task)
                    && c.source == reservation.source
                    && c.provider == reservation.provider
                    && c.model == reservation.model
                    && c.response_id.as_deref() == Some(response)
            })
        })
    })
}
fn err(code: &str) -> Value {
    json!({"ok":false,"error":code})
}
fn own(state: &AppState, record: &str, origin: &str, bearer: &str) -> Result<String> {
    if !owner_auth::origin_ok(origin) {
        return Err("owner_origin_forbidden");
    }
    Ok(owner_auth::owner_session_for(state, record, bearer)?.wallet)
}
fn fields(body: &Value, allowed: &[&str]) -> Result<()> {
    if body
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        Err("bad_strategy_request")
    } else {
        Ok(())
    }
}
fn mutate(state: &AppState, f: impl FnOnce(&mut StrategyState) -> Result<Value>) -> Value {
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let before = data.clone();
    let result = match f(&mut data) {
        Ok(v) => v,
        Err(code) => {
            *data = before;
            return err(code);
        }
    };
    drop(data);
    if save_snapshot_locked(state).is_err() {
        *state
            .strategy_cycle
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    result
}
fn revealed(c: &Challenge, data: &StrategyState) -> bool {
    let runs: Vec<_> = data.runs.values().filter(|r| r.challenge == c.id).collect();
    runs.len() == 2 && runs.iter().all(|r| r.status == "completed")
        || c.deadline().is_some_and(|end| now() >= end)
}
fn challenge_view(c: &Challenge, data: &StrategyState) -> Value {
    let mut v = serde_json::to_value(c).unwrap();
    v["revealed"] = json!(revealed(c, data));
    v["deadline_at"] = json!(c.deadline());
    v
}
fn view(r: &Run, v: &Version, c: &Challenge, private: bool, reveal: bool) -> Value {
    let conceal = !private && !v.published;
    let aliases: BTreeMap<_, _> = v
        .content
        .plan
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.clone(), format!("task-{}", i + 1)))
        .collect();
    let task_id = |id: &str| {
        if conceal {
            aliases.get(id).cloned().unwrap_or_else(|| id.into())
        } else {
            id.into()
        }
    };
    let plan: Vec<_> = v
        .content
        .plan
        .iter()
        .map(|t| {
            json!({
                "id":task_id(&t.id), "kind":t.kind,
                "depends_on":t.depends_on.iter().map(|d|task_id(d)).collect::<Vec<_>>(),
                "tool":if conceal {None} else {t.tool.clone()}
            })
        })
        .collect();
    let mut calls = serde_json::Map::new();
    for (id, call) in &r.calls {
        let mut out = serde_json::to_value(call).unwrap();
        out.as_object_mut().unwrap().remove("execution_id");
        out["execution_claimed"] = json!(call.execution_id.is_some());
        if !private {
            out.as_object_mut().unwrap().remove("report");
        }
        if conceal {
            for field in ["provider", "response_id", "report_hash"] {
                out.as_object_mut().unwrap().remove(field);
            }
            out["task"] = json!(task_id(id));
            out["model"] = json!(if call.source == "local_tool" {
                "Private tool"
            } else {
                "Private agent"
            });
        }
        calls.insert(task_id(id), out);
    }
    let mut events = r.events.clone();
    if conceal {
        for event in &mut events {
            event.task = task_id(&event.task);
        }
    }
    json!({"seat":r.seat,"id":r.id,"record":r.record,"owner":r.owner,"version":r.version,"challenge":r.challenge,"brief":c.content,"plan":plan,"budget":r.budget,"known_tokens":r.known(),"reserved_tokens":r.held(),"unknown_usage":r.uncertain() || c.deadline().is_some_and(|end|now()>=end) && r.calls.values().any(|c|c.usage.is_none()),"test_provider":r.test(),"usage_trust":"owner_submitted_provider_receipts","status":r.status,"calls":calls,"events":events,"result":if private || reveal {r.result.clone()} else {None},"review":if reveal {r.review.clone()} else {None},"reveal_pending":!reveal,"deadline_at":c.deadline(),"created_at":r.created_at,"monetary_cost":null,"ad_performance":"not_measured"})
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
    let versions: Vec<_> = data
        .versions
        .values()
        .filter(|v| v.published)
        .map(|v| {
            let mut out = serde_json::to_value(v).unwrap();
            let owners: BTreeSet<_> = data
                .runs
                .values()
                .filter(|r| {
                    r.version == v.id && r.status == "completed" && r.owner != v.content.owner
                })
                .map(|r| &r.owner)
                .collect();
            out["reuse_owners"] = json!(owners.len());
            out
        })
        .collect();
    let runs: Vec<_> = data
        .runs
        .values()
        .filter_map(|r| {
            data.versions.get(&r.version).map(|v| {
                view(
                    r,
                    v,
                    &data.challenges[&r.challenge],
                    false,
                    revealed(&data.challenges[&r.challenge], &data),
                )
            })
        })
        .collect();
    let standings:Vec<_>=data.challenges.values().map(|c| {
        // Test and provider receipts are ranked separately; this is token use, not money or conversions.
        let participants:Vec<_>=data.runs.values().filter(|r|r.challenge==c.id).collect();
        let ended=c.deadline().is_some_and(|end|now()>=end);
        let reviews_complete=participants.len()==2 && participants.iter().all(|r|c.content.trading.is_some() && r.status=="completed" || r.review.is_some() || ended && r.status!="completed");
        let leaders:Vec<_>=[false,true].into_iter().map(|test| {
            let eligible:Vec<_>=data.runs.values().filter(|r|r.challenge==c.id && (r.approved() || c.content.trading.is_some() && r.status=="completed") && revealed(c,&data) && r.result.as_ref().is_some_and(|result|c.deadline().is_some_and(|end|result["finished_at"].as_i64().is_some_and(|finished|finished<=end))) && r.test()==test).collect();
            let comparable = c.content.trading.is_none() || eligible.len()==2 && eligible.windows(2).all(|pair| { let a=pair[0].result.as_ref().unwrap(); let b=pair[1].result.as_ref().unwrap(); a["artifact"]==b["artifact"] && a["trade_receipt"]["cash_delta"]==b["trade_receipt"]["cash_delta"] && a["trade_receipt"]["goods_delta"]==b["trade_receipt"]["goods_delta"] });
            let best=if reviews_complete && comparable {eligible.iter().map(|r|r.known()).min()} else {None};
            json!({"test_provider":test,"accepted_runs":eligible.len(),"token_leaders":eligible.into_iter().filter(|r|Some(r.known())==best).map(|r|&r.id).collect::<Vec<_>>(),"tokens":best})
        }).collect();json!({"challenge":c.id,"reviews_complete":reviews_complete,"groups":leaders,"meaning":if c.content.trading.is_some(){"token_leader_only_for_equal_confirmed_action_and_outcome"}else{"lowest_reported_token_use_among_host_accepted_ads"},"monetary_winner":null,"conversion_winner":null})
    }).collect();
    json!({"ok":true,"execution":"agent_challenges","versions":versions,"challenges":data.challenges.values().map(|c|challenge_view(c,&data)).collect::<Vec<_>>(),"runs":runs,"standings":standings,"server_now":now()})
}
pub fn owned(state: &AppState, record: &str, bearer: &str) -> Value {
    let owner = match owner_auth::owner_session_for(state, record, bearer) {
        Ok(o) => o.wallet,
        Err(c) => return err(c),
    };
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    json!({"ok":true,"versions":data.versions.values().filter(|v|v.content.owner==owner).collect::<Vec<_>>(),"challenges":data.challenges.values().map(|c|challenge_view(c,&data)).collect::<Vec<_>>(),"runs":data.runs.values().filter(|r|r.owner==owner).map(|r|view(r,&data.versions[&r.version],&data.challenges[&r.challenge],true,revealed(&data.challenges[&r.challenge],&data))).collect::<Vec<_>>()})
}
pub fn create_challenge(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(
        body,
        &[
            "match_key",
            "round_seconds",
            "title",
            "audience",
            "task",
            "facts",
            "required_terms",
            "cta",
            "max_words",
            "budget",
            "trading",
        ],
    ) {
        return err(c);
    }
    let mut content = body.clone();
    content["owner"] = json!(owner);
    if let Some(request) = body.get("trading").filter(|v| !v.is_null()) {
        content["trading"] = match strategy_trade::capture(state, request) {
            Ok(context) => json!(context),
            Err(code) => return err(code),
        };
    }
    let brief: Brief = match serde_json::from_value(content) {
        Ok(b) if valid_brief(&b) => b,
        _ => return err("bad_brief"),
    };
    mutate(state, |data| {
        let id = digest(&brief);
        if let Some(c) = data.challenges.get(&id) {
            return Ok(json!({"ok":true,"challenge":c}));
        }
        if data.challenges.len() >= MAX_ITEMS {
            return Err("strategy_capacity");
        }
        let c = Challenge {
            id: id.clone(),
            content: brief,
            created_at: now(),
            started_at: None,
        };
        data.challenges.insert(id, c.clone());
        Ok(json!({"ok":true,"challenge":c}))
    })
}
pub fn create(state: &AppState, record: &str, origin: &str, bearer: &str, body: &Value) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(body, &["parent", "title", "plan", "change", "published"]) {
        return err(c);
    }
    let plan = match serde_json::from_value::<Vec<TaskSpec>>(body["plan"].clone()) {
        Ok(v) => v,
        _ => return err("bad_plan"),
    };
    let content = StrategyContent {
        owner,
        parent: match body.get("parent") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if hex32(s) => Some(s.clone()),
            _ => return err("bad_parent"),
        },
        title: body["title"].as_str().unwrap_or("").trim().into(),
        plan,
        change: body["change"].as_str().unwrap_or("").trim().into(),
    };
    if !valid_content(&content) || !body["published"].is_boolean() {
        return err("bad_strategy");
    }
    mutate(state, |data| {
        if content.parent.as_ref().is_some_and(|p| {
            !data.versions.get(p).is_some_and(|v| {
                v.published || body["published"] != true && v.content.owner == content.owner
            })
        }) {
            return Err("public_parent_required");
        }
        let id = digest(&content);
        if let Some(v) = data.versions.get(&id) {
            return Ok(json!({"ok":true,"version":v}));
        }
        if data.versions.len() >= MAX_ITEMS {
            return Err("strategy_capacity");
        }
        let v = Version {
            id: id.clone(),
            content,
            published: body["published"] == true,
            created_at: now(),
        };
        data.versions.insert(id, v.clone());
        Ok(json!({"ok":true,"version":v}))
    })
}
pub fn publish(state: &AppState, record: &str, origin: &str, bearer: &str, id: &str) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    mutate(state, |data| {
        let v = data
            .versions
            .get(id)
            .filter(|v| v.content.owner == owner)
            .ok_or("strategy_forbidden")?;
        if v.content
            .parent
            .as_ref()
            .is_some_and(|parent| !data.versions.get(parent).is_some_and(|v| v.published))
        {
            return Err("public_parent_required");
        }
        let v = data.versions.get_mut(id).unwrap();
        v.published = true;
        Ok(json!({"ok":true,"version":v}))
    })
}
pub fn start(state: &AppState, record: &str, origin: &str, bearer: &str, body: &Value) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(body, &["version", "challenge_id", "client_id"]) {
        return err(c);
    }
    let version = body["version"].as_str().unwrap_or("");
    let challenge = body["challenge_id"].as_str().unwrap_or("");
    let client = body["client_id"].as_str().unwrap_or("");
    if client.len() < 8
        || client.len() > 80
        || !client
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return err("bad_client_id");
    }
    mutate(state, |data| {
        let id = sha256_hex(&format!("strategy-run-v2:{owner}:{client}"));
        if let Some(r) = data.runs.get(&id) {
            if r.record != record || (version != "selected" && r.version != version) || r.challenge != challenge {
                return Err("idempotency_conflict");
            }
            return Ok(
                json!({"ok":true,"run":view(r,&data.versions[&r.version],&data.challenges[challenge],true,false)}),
            );
        }
        // Resolve the owner's selection once when creating a new run. A retry
        // keeps the version already recorded above, even after a later selection.
        let version = if version == "selected" {
            research::selected_version(data, record, &owner).ok_or("selected_strategy_missing")?
        } else {
            version
        };
        let v = data.versions.get(version).ok_or("unknown_strategy")?;
        let c = data.challenges.get(challenge).ok_or("unknown_challenge")?;
        if let Some(trading) = &c.content.trading {
            if !trading.participants.iter().any(|p| p.wallet == owner) {
                return Err("trading_owner_forbidden");
            }
            if now() >= trading.phase_ends_at {
                return Err("round_ended");
            }
        }
        if v.content.owner != owner && !v.published {
            return Err("strategy_forbidden");
        }
        if v.content.parent.is_some() && !v.published {
            return Err("publish_derivative_first");
        }
        if c.content.owner == owner && c.content.trading.is_none() {
            return Err("host_cannot_compete");
        }
        if data.runs.len() >= MAX_ITEMS {
            return Err("strategy_capacity");
        }
        let participants: Vec<_> = data
            .runs
            .values()
            .filter(|r| r.challenge == challenge)
            .collect();
        if participants.len() >= 2 || participants.iter().any(|r| r.owner == owner) {
            return Err("duel_seat_unavailable");
        }
        let starts = participants.len() == 1;
        let mut r = Run {
            seat: participants.len() as u8,
            id: id.clone(),
            owner,
            record: record.into(),
            client_id: client.into(),
            version: version.into(),
            challenge: challenge.into(),
            budget: c.content.budget,
            calls: BTreeMap::new(),
            events: vec![],
            status: "running".into(),
            result: None,
            review: None,
            created_at: now(),
        };
        r.event("chief", "run_started");
        let out = view(&r, v, c, true, false);
        data.runs.insert(id, r);
        if starts {
            data.challenges.get_mut(challenge).unwrap().started_at = Some(now());
        }
        Ok(json!({"ok":true,"run":out}))
    })
}
fn valid_call(spec: &TaskSpec, cap: u64, source: &str, provider: &str, model: &str) -> bool {
    !provider.is_empty()
        && provider.len() <= 128
        && !model.is_empty()
        && model.len() <= 128
        && cap <= MAX_BUDGET
        && (matches!(source, "provider_reported" | "test_provider") && cap > 0
            || source == "local_tool" && spec.kind == "tool" && cap == 0)
}
pub fn reserve(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    mutate(state, |data| {
        let r = data
            .runs
            .get_mut(id)
            .filter(|r| r.owner == owner && r.record == record)
            .ok_or("run_forbidden")?;
        let v = &data.versions[&r.version];
        let brief = &data.challenges[&r.challenge];
        reserve_run(r, v, brief, body)?;
        Ok(json!({"ok":true,"run":view(r,v,brief,true,false)}))
    })
}
fn reserve_run(r: &mut Run, v: &Version, brief: &Challenge, body: &Value) -> Result<()> {
    fields(
        body,
        &[
            "task",
            "max_total_tokens",
            "source",
            "provider",
            "model",
            "execution_id",
        ],
    )?;
    let task = body["task"].as_str().unwrap_or("");
    let cap = match body["max_total_tokens"].as_u64() {
        Some(v) => v,
        None => return Err("bad_reservation"),
    };
    let source = body["source"].as_str().unwrap_or("");
    let provider = body["provider"].as_str().unwrap_or("");
    let model = body["model"].as_str().unwrap_or("");
    let spec = v
        .content
        .plan
        .iter()
        .find(|t| t.id == task)
        .ok_or("bad_task")?;
    if !valid_call(spec, cap, source, provider, model) {
        return Err("bad_provider");
    }
    let execution_id = body["execution_id"]
        .as_str()
        .filter(|s| hex32(s))
        .ok_or("execution_id_required")?;
    if let Some(c) = r.calls.get(task) {
        if c.execution_id.as_deref() != Some(execution_id) {
            return Err("execution_conflict");
        }
        if c.reserved != cap || c.source != source || c.provider != provider || c.model != model {
            return Err("idempotency_conflict");
        }
        return Ok(());
    }
    let deadline = brief.deadline().ok_or("waiting_for_opponent")?;
    if now() >= deadline {
        return Err("round_ended");
    }
    if r.status != "running" || r.uncertain() {
        return Err("run_stopped");
    }
    if !spec.depends_on.iter().all(|d| r.passed(d)) {
        return Err("dependencies_not_complete");
    }
    if r.known().saturating_add(r.held()).saturating_add(cap) > r.budget {
        return Err("budget_exhausted");
    }
    r.calls.insert(
        task.into(),
        Call {
            task: task.into(),
            execution_id: Some(execution_id.into()),
            reserved: cap,
            status: "reserved".into(),
            source: source.into(),
            provider: provider.into(),
            model: model.into(),
            response_id: None,
            usage: None,
            report: None,
            report_hash: None,
            quality_passed: None,
            started_at: now(),
            finished_at: None,
        },
    );
    r.event(task, "task_started");
    Ok(())
}
pub fn report(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    mutate(state, |data| {
        let r = data
            .runs
            .get(id)
            .filter(|r| r.owner == owner && r.record == record)
            .ok_or("run_forbidden")?;
        if duplicate_receipt(data, &r.id, body) {
            return Err("duplicate_provider_receipt");
        }
        let r = data.runs.get_mut(id).unwrap();
        let v = &data.versions[&r.version];
        let brief = &data.challenges[&r.challenge];
        report_run(r, brief, body)?;
        Ok(json!({"ok":true,"run":view(r,v,brief,true,false)}))
    })
}
fn report_run(r: &mut Run, brief: &Challenge, body: &Value) -> Result<()> {
    fields(
        body,
        &[
            "task",
            "response_id",
            "usage",
            "report",
            "request_failed",
            "execution_id",
        ],
    )?;
    let task = body["task"].as_str().unwrap_or("");
    let reservation = r.calls.get(task).ok_or("reservation_required")?;
    // Old snapshots can recover saved receipts, but cannot grant a new execution claim.
    match (&reservation.execution_id, body.get("execution_id")) {
        (Some(expected), Some(Value::String(actual))) if expected == actual => {}
        (None, None) => {}
        _ => return Err("execution_conflict"),
    }
    let response = match body.get("response_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.is_empty() && s.len() <= 200 => Some(s.clone()),
        _ => return Err("bad_response_id"),
    };
    let usage = match body.get("usage") {
        None | Some(Value::Null) => None,
        Some(u) => match serde_json::from_value::<Usage>(u.clone()) {
            Ok(u) if u.valid() => Some(u),
            _ => return Err("bad_usage"),
        },
    };
    let supplied = body.get("report").cloned().unwrap_or(Value::Null);
    if supplied.to_string().len() > 8192 || !body["request_failed"].is_boolean() {
        return Err("bad_report");
    }
    let hash = digest(&supplied);
    let reservation = r.calls.get(task).ok_or("reservation_required")?;
    if reservation.source == "local_tool" && usage.as_ref().is_some_and(|u| u.total_tokens != 0) {
        return Err("bad_usage");
    }
    let quality = body["request_failed"] == false
        && supplied.as_object().is_some_and(|o| !o.is_empty())
        && (task != "deliver" || checks(&brief.content, &supplied)["passed"] == true);
    let c = r.calls.get_mut(task).unwrap();
    if c.status != "reserved" {
        return if c.response_id == response
            && c.usage == usage
            && c.report_hash.as_ref() == Some(&hash)
            && c.quality_passed == Some(quality)
        {
            Ok(())
        } else {
            Err("idempotency_conflict")
        };
    }
    c.status = if body["request_failed"] == true {
        "failed"
    } else {
        "reported"
    }
    .into();
    c.response_id = response;
    c.usage = usage;
    c.report_hash = Some(hash);
    c.quality_passed = Some(quality);
    c.finished_at = Some(now());
    c.report = if quality { Some(supplied) } else { None };
    r.event(
        task,
        if quality {
            "report_checked"
        } else {
            "quality_failed"
        },
    );
    r.status = if r.known() > r.budget
        || r.calls.values().any(|c| {
            c.usage
                .as_ref()
                .is_some_and(|u| u.total_tokens > c.reserved)
        }) {
        "budget_overrun"
    } else if r.uncertain() {
        "usage_unknown"
    } else if r.calls.values().any(|c| c.quality_passed == Some(false)) {
        "quality_failed"
    } else {
        "running"
    }
    .into();
    Ok(())
}
pub fn finish(state: &AppState, record: &str, origin: &str, bearer: &str, id: &str) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    mutate(state, |data| {
        let r = data
            .runs
            .get_mut(id)
            .filter(|r| r.owner == owner && r.record == record)
            .ok_or("run_forbidden")?;
        let v = &data.versions[&r.version];
        let brief = &data.challenges[&r.challenge];
        finish_run(r, v, brief)?;
        Ok(json!({"ok":true,"run":view(r,v,brief,true,false)}))
    })
}
fn finish_run(r: &mut Run, v: &Version, brief: &Challenge) -> Result<()> {
    if r.status == "completed" {
        return Ok(());
    }
    if r.status != "running" || !v.content.plan.iter().all(|t| r.passed(&t.id)) {
        return Err("verification_required");
    }
    let artifact = r.calls["deliver"].report.clone().unwrap();
    if brief.content.trading.is_some() {
        return Err("trade_receipt_required");
    }
    r.result = Some(
        json!({"checks":checks(&brief.content,&artifact),"artifact":artifact,"finished_at":now()}),
    );
    r.status = "completed".into();
    r.event("chief", "brief_checks_passed");
    Ok(())
}
pub fn review(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(body, &["accepted", "reason"]) {
        return err(c);
    }
    let Some(accepted) = body["accepted"].as_bool() else {
        return err("bad_review");
    };
    let reason = body["reason"].as_str().unwrap_or("").trim();
    if reason.is_empty() || reason.len() > 1000 {
        return err("bad_review");
    }
    mutate(state, |data| {
        let candidate = data.runs.get(id).ok_or("unknown_run")?;
        if !revealed(&data.challenges[&candidate.challenge], data) {
            return Err("reveal_pending");
        }
        let r = data.runs.get_mut(id).unwrap();
        let v = &data.versions[&r.version];
        let brief = &data.challenges[&r.challenge];
        if brief.content.trading.is_some() {
            return Err("trading_uses_game_receipts");
        }
        if owner != brief.content.owner || owner == r.owner {
            return Err("reviewer_forbidden");
        }
        if r.status != "completed" {
            return Err("verification_required");
        }
        if let Some(old) = &r.review {
            if old.accepted != accepted || old.reason != reason {
                return Err("idempotency_conflict");
            }
        } else {
            r.review = Some(Review {
                reviewer: owner,
                accepted,
                reason: reason.into(),
                at: now(),
            });
            r.event("reviewer", "host_review_recorded");
        }
        Ok(json!({"ok":true,"run":view(r,v,brief,false,true)}))
    })
}

fn trade_ready(data: &StrategyState, id: &str, record: &str) -> Result<()> {
    let r = data
        .runs
        .get(id)
        .filter(|r| r.record == record)
        .ok_or("run_forbidden")?;
    let c = &data.challenges[&r.challenge];
    if c.content.trading.is_none() {
        return Err("trading_challenge_required");
    }
    if r.status != "running"
        || !data.versions[&r.version]
            .content
            .plan
            .iter()
            .all(|t| r.passed(&t.id))
    {
        return Err("verification_required");
    }
    if c.deadline().is_none_or(|end| now() >= end) {
        return Err("round_ended");
    }
    Ok(())
}
pub fn prepare_trade(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if data.runs.get(id).is_none_or(|r| r.owner != owner) {
        return err("run_forbidden");
    }
    if let Err(code) = trade_ready(&data, id, record) {
        return err(code);
    }
    let run = &data.runs[id];
    json!({"ok":true,"decision":run.calls["deliver"].report,"trading":data.challenges[&run.challenge].content.trading})
}
// Called while h_act holds the transaction lock. The game bearer and strategy
// owner must identify the same registration; an untrusted report cannot sign.
pub(super) fn check_http_trade(
    state: &AppState,
    game: u64,
    record: &str,
    body: &Value,
) -> Result<()> {
    let id = body["strategy_run"]
        .as_str()
        .filter(|s| hex32(s))
        .ok_or("bad_strategy_run")?;
    let data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    trade_ready(&data, id, record)?;
    let r = &data.runs[id];
    let context = data.challenges[&r.challenge]
        .content
        .trading
        .as_ref()
        .unwrap();
    let decision = json!({"action":body["action"],"params":body["params"]});
    if context.mode != "http"
        || context.game != game.to_string()
        || r.calls["deliver"].report.as_ref() != Some(&decision)
    {
        return Err("trade_decision_mismatch");
    }
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let entry = games.get(&game).ok_or("unknown_game")?;
    if entry.sim.game.round != context.round || phase_name(entry.sim.game.phase) != context.phase {
        return Err("trading_window_changed");
    }
    Ok(())
}
fn complete_trade(data: &mut StrategyState, id: &str, receipt: Value) -> Result<()> {
    if data.runs.values().any(|r| {
        r.id != id
            && r.result.as_ref().is_some_and(|v| {
                v["trade_receipt"]["mode"] == receipt["mode"]
                    && v["trade_receipt"]["game"] == receipt["game"]
                    && v["trade_receipt"]["event_id"] == receipt["event_id"]
            })
    }) {
        return Err("trade_receipt_reused");
    }
    let r = data.runs.get_mut(id).ok_or("unknown_run")?;
    let c = &data.challenges[&r.challenge];
    if r.status == "completed" {
        return if r
            .result
            .as_ref()
            .is_some_and(|v| v["trade_receipt"] == receipt)
        {
            Ok(())
        } else {
            Err("idempotency_conflict")
        };
    }
    let at = receipt["confirmed_at"]
        .as_i64()
        .ok_or("trade_receipt_mismatch")?;
    if !data.versions[&r.version]
        .content
        .plan
        .iter()
        .all(|t| r.passed(&t.id))
        || r.status != "running"
        || c.deadline().is_none_or(|end| at > end)
        || r.calls["deliver"].finished_at.is_none_or(|t| at < t)
    {
        return Err("trade_receipt_mismatch");
    }
    let artifact = r.calls["deliver"].report.clone().unwrap();
    r.result = Some(
        json!({"artifact":artifact,"checks":checks(&c.content,&artifact),"finished_at":at,"trade_receipt":receipt}),
    );
    r.status = "completed".into();
    r.event("chief", "trade_confirmed");
    Ok(())
}
pub(super) fn confirm_http_trade(state: &AppState, id: &str, event: &Value) -> Result<()> {
    let mut data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let r = data.runs.get(id).ok_or("unknown_run")?;
    let context = data.challenges[&r.challenge]
        .content
        .trading
        .as_ref()
        .ok_or("trading_challenge_required")?;
    let receipt = strategy_trade::http_receipt(
        context,
        &r.owner,
        r.calls["deliver"]
            .report
            .as_ref()
            .ok_or("verification_required")?,
        event,
    )?;
    complete_trade(&mut data, id, receipt)
}
pub(super) fn reject_http_trade(state: &AppState, id: &str) {
    let mut data = state
        .strategy_cycle
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(r) = data.runs.get_mut(id) {
        r.status = "quality_failed".into();
        r.event("chief", "trade_rejected");
    }
}
pub fn confirm_chain_trade(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if fields(body, &["signature"]).is_err() {
        return err("bad_trade_receipt");
    }
    let Some(signature) = body["signature"].as_str().filter(|s| {
        (80..=100).contains(&s.len())
            && s.bytes()
                .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
    }) else {
        return err("bad_trade_receipt");
    };
    let (context, decision) = {
        let data = state
            .strategy_cycle
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(r) = data
            .runs
            .get(id)
            .filter(|r| r.owner == owner && r.record == record)
        else {
            return err("run_forbidden");
        };
        if r.status == "completed" {
            return if r
                .result
                .as_ref()
                .is_some_and(|v| v["trade_receipt"]["signature"] == signature)
            {
                json!({"ok":true,"run":view(r,&data.versions[&r.version],&data.challenges[&r.challenge],true,false)})
            } else {
                err("idempotency_conflict")
            };
        }
        let Some(context) = data.challenges[&r.challenge]
            .content
            .trading
            .clone()
            .filter(|c| c.mode == "solana")
        else {
            return err("trading_challenge_required");
        };
        let Some(decision) = r.calls.get("deliver").and_then(|c| c.report.clone()) else {
            return err("verification_required");
        };
        (context, decision)
    };
    let snapshot = match super::chain_wishes::public_chain(&context.game) {
        Ok(v) => v,
        Err(c) => return err(c),
    };
    let receipt =
        match strategy_trade::chain_receipt(&context, &owner, &decision, &snapshot, signature) {
            Ok(v) => v,
            Err(c) => return err(c),
        };
    mutate(state, |data| {
        complete_trade(data, id, receipt)?;
        let r = &data.runs[id];
        Ok(
            json!({"ok":true,"run":view(r,&data.versions[&r.version],&data.challenges[&r.challenge],true,false)}),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{load_snapshot, new_state_with_files, save_snapshot, RegisteredAgent};
    use crate::registration::Receipt;
    use alashi_rules::anchor_lang::prelude::Pubkey;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    const ORIGIN: &str = "https://alashi.network";
    pub(super) fn actor(i: usize) -> (String, String) {
        (
            ["a", "b", "e", "f"][i].repeat(64),
            ["c", "d", "f", "1"][i].repeat(64),
        )
    }
    pub(super) fn fixture() -> Arc<AppState> {
        static N: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "alashi_duel_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let state = new_state_with_files(root.join("state.json"), root.join("seq"));
        for i in 0..4 {
            let (record, raw) = actor(i);
            let key = solana_ed25519::ed_sigs::SigningKey::from([i as u8 + 1; 32]);
            let vk: [u8; 32] = solana_ed25519::ed_sigs::VerificationKey::from(&key).into();
            let wallet = Pubkey::new_from_array(vk).to_string();
            state.registrations.lock().unwrap().insert(
                record.clone(),
                RegisteredAgent {
                    wallet: wallet.clone(),
                    owner_id: format!("{i:064x}"),
                    character_id: record.clone(),
                    recovery_hash: "e".repeat(64),
                    challenge: "f".repeat(64),
                    receipt: Some(Receipt {
                        mode: "agent_lifecycle_v2".into(),
                        network: "devnet".into(),
                        wallet: wallet.clone(),
                        signature: "offline-fixture".into(),
                        slot: 0,
                        fee_lamports: "0".into(),
                        commitment: "confirmed".into(),
                    }),
                    created_at: 1,
                },
            );
            let hash = sha256_hex(&format!("alashi-owner-session-v1:{raw}"));
            state.owner_auth.lock().unwrap().sessions.insert(
                hash.clone(),
                owner_auth::OwnerSession {
                    agent_record_id: record,
                    wallet,
                    session_hash: hash,
                    issued_at: now(),
                    expires_at: now() + 3600,
                    revoked_at: None,
                },
            );
        }
        save_snapshot(&state).unwrap();
        state
    }
    pub(super) fn plan() -> Value {
        json!([{"id":"research","kind":"model","depends_on":[],"instruction":"Review the brief facts.","tool":null},{"id":"deliver","kind":"model","depends_on":["research"],"instruction":"Write the ad.","tool":null}])
    }
    #[test]
    fn trading_challenge_captures_equal_game_conditions() {
        let s = fixture();
        let game = super::super::h_new_game(
            &s,
            &json!({"entry_fee":1,"phase_duration":300,"lobby_duration":300,"grace_s":0}),
        )["game_id"]
            .as_u64()
            .unwrap();
        let mut tokens = Vec::new();
        for i in [1, 2] {
            let (record, _) = actor(i);
            let secret = format!("{i:064x}");
            s.registrations
                .lock()
                .unwrap()
                .get_mut(&record)
                .unwrap()
                .recovery_hash = super::super::recovery_hash(&secret).unwrap();
            let joined = super::super::h_join_v2(
                &s,
                game,
                &json!({"agent_record_id":record,"recovery_secret":secret,"name":format!("Trader{i}"),"model":"TEST","strategy_hash":"a".repeat(64)}),
            );
            assert_eq!(joined["ok"], true, "{joined}");
            tokens.push(joined["token"].as_str().unwrap().to_string());
        }
        s.games
            .lock()
            .unwrap()
            .get_mut(&game)
            .unwrap()
            .sim
            .game
            .phase_ends_at = 0;
        super::super::crank_once(&s);
        s.games
            .lock()
            .unwrap()
            .get_mut(&game)
            .unwrap()
            .sim
            .game
            .phase_ends_at = 0;
        super::super::crank_once(&s);
        let (a, token) = actor(0);
        let created = create_challenge(
            &s,
            &a,
            ORIGIN,
            &token,
            &json!({"match_key":"trade-one","round_seconds":180,"title":"Equal trading decision","audience":"Agent owners","task":"Choose a legal trade from the shared market state.","facts":["One alashi is 1000000 game units."],"required_terms":[],"cta":"","max_words":0,"budget":20000,"trading":{"mode":"http","game":game.to_string()}}),
        );
        assert_eq!(created["ok"], true, "{created}");
        assert_eq!(
            created["challenge"]["content"]["trading"]["participants"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let c = created["challenge"]["id"].as_str().unwrap();
        let versions = [version(&s, 1, None, true), version(&s, 2, None, true)];
        let runs = [
            start_run(&s, 1, &versions[0], c, "trade-player-one"),
            start_run(&s, 2, &versions[1], c, "trade-player-two"),
        ];
        for i in 1..=2 {
            let (record, bearer) = actor(i);
            let r = &runs[i - 1];
            assert_eq!(reserve_call(&s, i, r, "research", 100)["ok"], true);
            assert_eq!(report_call(&s, i, r, "research", usage(), true)["ok"], true);
            assert_eq!(reserve_call(&s, i, r, "deliver", 100)["ok"], true);
            let v = report(
                &s,
                &record,
                ORIGIN,
                &bearer,
                r,
                &json!({"task":"deliver","execution_id":"a".repeat(64),"response_id":format!("trade-{i}"),"usage":if i==1{usage()}else{Value::Null},"report":{"action":"produce","params":{}},"request_failed":false}),
            );
            assert_eq!(v["ok"], true, "{v}");
        }
        let (record, bearer) = actor(1);
        assert_eq!(
            finish(&s, &record, ORIGIN, &bearer, &runs[0])["error"],
            "trade_receipt_required"
        );
        let intent = json!({"token":tokens[0],"op_id":1,"action":"produce","params":{},"by":"llm","strategy_run":runs[0]});
        assert_eq!(
            check_http_trade(&s, game + 1, &record, &intent),
            Err("trade_decision_mismatch")
        );
        let mut wrong = intent.clone();
        wrong["token"] = json!(tokens[1]);
        assert_eq!(
            super::super::h_act(&s, game, &wrong)["error"],
            "run_forbidden"
        );
        wrong = intent.clone();
        wrong["action"] = json!("buy");
        wrong["params"] = json!({"units":1});
        assert_eq!(
            super::super::h_act(&s, game, &wrong)["error"],
            "trade_decision_mismatch"
        );
        let before_events = s.live.lock().unwrap().events.len();
        let path = s.snapshot_path.clone();
        let saved = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            super::super::h_act(&s, game, &intent)["error"],
            "storage_failed"
        );
        assert_eq!(s.games.lock().unwrap()[&game].sim.factions[0].goods, 0);
        assert_eq!(s.live.lock().unwrap().events.len(), before_events);
        assert_eq!(
            s.strategy_cycle.lock().unwrap().runs[&runs[0]].status,
            "running"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, saved).unwrap();
        let accepted = super::super::h_act(&s, game, &intent);
        assert_eq!(accepted["ok"], true, "{accepted}");
        assert_eq!(super::super::h_act(&s, game, &intent), accepted);
        assert_eq!(s.live.lock().unwrap().events.len(), before_events + 1);
        let ledger = s.strategy_cycle.lock().unwrap();
        assert_eq!(
            ledger.runs[&runs[0]].result.as_ref().unwrap()["trade_receipt"]["goods_delta"],
            2
        );
        assert_eq!(ledger.runs[&runs[1]].known(), 15);
        assert_eq!(ledger.runs[&runs[1]].held(), 100);
        assert!(validate(&ledger));
        let receipt = ledger.runs[&runs[0]].result.as_ref().unwrap()["trade_receipt"].clone();
        assert_eq!(
            complete_trade(&mut ledger.clone(), &runs[1], receipt),
            Err("trade_receipt_reused")
        );
        drop(ledger);
        wrong = intent.clone();
        wrong["token"] = json!(tokens[1]);
        wrong["strategy_run"] = json!(runs[1]);
        assert_eq!(
            super::super::h_act(&s, game, &wrong)["error"],
            "verification_required"
        );
    }
    pub(super) fn brief(s: &AppState, key: &str, budget: u64) -> String {
        let (a, t) = actor(0);
        let v = create_challenge(
            s,
            &a,
            ORIGIN,
            &t,
            &json!({"match_key":key,"round_seconds":180,"title":"ALASHI ad","audience":"AI agent owners","task":"Write an English ad for ALASHI to attract agent owners.","facts":["Owners use their own AI agents and tools.","Crystals represent model tokens.","Public strategies can be reused and improved."],"required_terms":["ALASHI","agents","tokens"],"cta":"Explore ALASHI","max_words":80,"budget":budget}),
        );
        assert_eq!(v["ok"], true, "{v}");
        v["challenge"]["id"].as_str().unwrap().into()
    }
    fn version(s: &AppState, i: usize, parent: Option<&str>, published: bool) -> String {
        let (a, t) = actor(i);
        let mut work_plan = plan();
        if !published {
            work_plan[0]["kind"] = json!("tool");
            work_plan[0]["tool"] = json!("confidential-tool");
        }
        let v = create(
            s,
            &a,
            ORIGIN,
            &t,
            &json!({"parent":parent,"title":format!("Team {i}"),"plan":work_plan,"change":if parent.is_some(){"Shorten the work plan"}else{""},"published":published}),
        );
        assert_eq!(v["ok"], true, "{v}");
        v["version"]["id"].as_str().unwrap().into()
    }
    fn start_run(s: &AppState, i: usize, v: &str, c: &str, key: &str) -> String {
        let (a, t) = actor(i);
        let r = start(
            s,
            &a,
            ORIGIN,
            &t,
            &json!({"version":v,"challenge_id":c,"client_id":key}),
        );
        assert_eq!(r["ok"], true, "{r}");
        r["run"]["id"].as_str().unwrap().into()
    }
    fn reserve_call(s: &AppState, i: usize, id: &str, task: &str, cap: u64) -> Value {
        let (a, t) = actor(i);
        reserve(
            s,
            &a,
            ORIGIN,
            &t,
            id,
            &json!({"task":task,"execution_id":"a".repeat(64),"max_total_tokens":cap,"source":"test_provider","provider":"local-fixture","model":"deterministic-test"}),
        )
    }
    fn ad() -> Value {
        json!({"headline":"Build with ALASHI","body":"Bring your agents and tools to a shared challenge. Spend model tokens on your plan, then share a useful strategy so the next owner can build on it.","cta":"Explore ALASHI"})
    }
    fn report_call(s: &AppState, i: usize, id: &str, task: &str, u: Value, good: bool) -> Value {
        let (a, t) = actor(i);
        report(
            s,
            &a,
            ORIGIN,
            &t,
            id,
            &json!({"task":task,"execution_id":"a".repeat(64),"response_id":format!("{id}-{task}"),"usage":u,"report":if good {if task=="deliver" {ad()}else{json!({"facts":"from shared brief"})}} else {json!({})},"request_failed":false}),
        )
    }
    fn usage() -> Value {
        json!({"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"cached_tokens":0})
    }
    fn complete(s: &AppState, i: usize, id: &str) -> Value {
        for task in ["research", "deliver"] {
            assert_eq!(reserve_call(s, i, id, task, 100)["ok"], true);
            assert_eq!(report_call(s, i, id, task, usage(), true)["ok"], true);
        }
        let (a, t) = actor(i);
        finish(s, &a, ORIGIN, &t, id)
    }
    #[test]
    fn blind_reveal_independent_review_and_private_original_choice() {
        let s = fixture();
        let c = brief(&s, "blind-duel", 1000);
        let v = version(&s, 1, None, false);
        let b = version(&s, 2, None, true);
        let first = start_run(&s, 1, &v, &c, "first-owner-01");
        assert_eq!(
            reserve_call(&s, 1, &first, "research", 100)["error"],
            "waiting_for_opponent"
        );
        let second = start_run(&s, 2, &b, &c, "second-owner-01");
        let done = complete(&s, 1, &first);
        assert_eq!(done["ok"], true, "{done}");
        let snapshot = public(&s);
        let r = snapshot["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == first)
            .unwrap();
        assert!(r["result"].is_null());
        assert!(r["calls"]["deliver"].get("report").is_none());
        assert!(
            !r["plan"].to_string().contains("research"),
            "private task names leaked in the public execution trace"
        );
        assert!(
            r["calls"].get("research").is_none(),
            "private call keys must be opaque"
        );
        assert!(!r.to_string().contains("confidential-tool"));
        assert!(r["calls"]["task-1"].get("provider").is_none());
        assert!(r["calls"]["task-1"].get("response_id").is_none());
        assert_eq!(r["calls"]["task-1"]["model"], "Private agent");
        let (private_record, private_token) = actor(1);
        let own = owned(&s, &private_record, &private_token);
        let own_run = own["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == first)
            .unwrap();
        assert!(own_run["calls"].get("research").is_some());
        assert_eq!(own_run["plan"][0]["tool"], "confidential-tool");
        assert!(snapshot["versions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["id"] != s.strategy_cycle.lock().unwrap().runs[&first].version));
        let (host, token) = actor(0);
        assert_eq!(
            review(
                &s,
                &host,
                ORIGIN,
                &token,
                &first,
                &json!({"accepted":true,"reason":"Not yet"})
            )["error"],
            "reveal_pending"
        );
        assert_eq!(complete(&s, 2, &second)["ok"], true);
        let (a, t) = actor(1);
        assert_eq!(
            review(
                &s,
                &a,
                ORIGIN,
                &t,
                &first,
                &json!({"accepted":true,"reason":"Self"})
            )["error"],
            "reviewer_forbidden"
        );
        for id in [&first, &second] {
            assert_eq!(
                review(
                    &s,
                    &host,
                    ORIGIN,
                    &token,
                    id,
                    &json!({"accepted":true,"reason":"Offline fixture review"})
                )["ok"],
                true
            );
        }
        assert_eq!(
            public(&s)["standings"][0]["groups"][1]["token_leaders"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            finish(&s, &a, ORIGIN, &t, &first)["run"]["known_tokens"],
            30
        );
        assert!(validate(&s.strategy_cycle.lock().unwrap()));
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
    }
    #[test]
    fn executor_claim_is_exclusive_persisted_and_hidden() {
        let s = fixture();
        let c = brief(&s, "claim-duel", 1000);
        let v = version(&s, 1, None, true);
        let w = version(&s, 2, None, true);
        let run = start_run(&s, 1, &v, &c, "claim-owner-01");
        start_run(&s, 2, &w, &c, "claim-owner-02");
        let (record, bearer) = actor(1);
        let body = json!({"task":"research","max_total_tokens":100,"source":"test_provider","provider":"local-fixture","model":"deterministic-test"});
        assert_eq!(
            reserve(&s, &record, ORIGIN, &bearer, &run, &body)["error"],
            "execution_id_required"
        );
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|key| {
                let (s, barrier, mut body, run) =
                    (s.clone(), barrier.clone(), body.clone(), run.clone());
                std::thread::spawn(move || {
                    let (record, bearer) = actor(1);
                    body["execution_id"] = json!(key.repeat(64));
                    barrier.wait();
                    (
                        body.clone(),
                        reserve(&s, &record, ORIGIN, &bearer, &run, &body),
                    )
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|(_, r)| r["ok"] == true).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|(_, r)| r["error"] == "execution_conflict")
                .count(),
            1
        );
        let (winner, response) = results.iter().find(|(_, r)| r["ok"] == true).unwrap();
        assert_eq!(
            response["run"]["calls"]["research"]["execution_claimed"],
            true
        );
        for output in [response.clone(), public(&s), owned(&s, &record, &bearer)] {
            assert!(!output.to_string().contains("execution_id"));
        }
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert!(validate(&restored.strategy_cycle.lock().unwrap()));
        {
            let mut data = restored.strategy_cycle.lock().unwrap();
            data.challenges.get_mut(&c).unwrap().started_at = Some(now() - 1000);
        }
        assert_eq!(
            reserve(&restored, &record, ORIGIN, &bearer, &run, winner)["ok"],
            true,
            "retry of owned reservation does not start new work after expiry"
        );
        let mut receipt = json!({"task":"research","response_id":null,"usage":{"prompt_tokens":0,"completion_tokens":0,"total_tokens":0,"cached_tokens":0},"report":null,"request_failed":true});
        assert_eq!(
            report(&restored, &record, ORIGIN, &bearer, &run, &receipt)["error"],
            "execution_conflict"
        );
        receipt["execution_id"] = results
            .iter()
            .find(|(_, r)| r["error"] == "execution_conflict")
            .unwrap()
            .0["execution_id"]
            .clone();
        assert_eq!(
            report(&restored, &record, ORIGIN, &bearer, &run, &receipt)["error"],
            "execution_conflict"
        );
        receipt["execution_id"] = winner["execution_id"].clone();
        let done = report(&restored, &record, ORIGIN, &bearer, &run, &receipt);
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(done["run"]["reserved_tokens"], 0);
        assert_eq!(done["run"]["unknown_usage"], false);
        assert_eq!(done["run"]["status"], "quality_failed");
        assert_eq!(
            report(&restored, &record, ORIGIN, &bearer, &run, &receipt),
            done
        );
    }
    #[test]
    fn legacy_reservation_can_report_but_cannot_grant_new_execution() {
        let s = fixture();
        let c = brief(&s, "legacy-claim", 1000);
        let v = version(&s, 1, None, true);
        let w = version(&s, 2, None, true);
        let run = start_run(&s, 1, &v, &c, "legacy-owner-01");
        start_run(&s, 2, &w, &c, "legacy-owner-02");
        assert_eq!(reserve_call(&s, 1, &run, "research", 100)["ok"], true);
        let mut snapshot = serde_json::to_value(&*s.strategy_cycle.lock().unwrap()).unwrap();
        snapshot["runs"][&run]["calls"]["research"]
            .as_object_mut()
            .unwrap()
            .remove("execution_id");
        *s.strategy_cycle.lock().unwrap() = serde_json::from_value(snapshot).unwrap();
        assert!(validate(&s.strategy_cycle.lock().unwrap()));
        assert_eq!(
            reserve_call(&s, 1, &run, "research", 100)["error"],
            "execution_conflict"
        );
        let (record, bearer) = actor(1);
        let receipt = json!({"task":"research","response_id":"legacy-saved-response","usage":usage(),"report":{"facts":"saved before upgrade"},"request_failed":false});
        assert_eq!(
            report(&s, &record, ORIGIN, &bearer, &run, &receipt)["ok"],
            true
        );
        let mut data = s.strategy_cycle.lock().unwrap();
        data.runs
            .get_mut(&run)
            .unwrap()
            .calls
            .get_mut("research")
            .unwrap()
            .execution_id = Some("bad".into());
        assert!(!validate(&data));
    }
    #[test]
    fn paid_failures_unknown_usage_and_shared_budget_are_not_zeroed() {
        let s = fixture();
        let c = brief(&s, "unknown-duel", 1000);
        let v = version(&s, 1, None, true);
        let w = version(&s, 2, None, true);
        let a = start_run(&s, 1, &v, &c, "unknown-owner-01");
        let b = start_run(&s, 2, &w, &c, "quality-owner-01");
        reserve_call(&s, 1, &a, "research", 100);
        let r = report_call(&s, 1, &a, "research", Value::Null, true);
        assert_eq!(r["run"]["status"], "usage_unknown");
        assert_eq!(r["run"]["reserved_tokens"], 100);
        assert_eq!(
            reserve_call(&s, 1, &a, "deliver", 100)["error"],
            "run_stopped"
        );
        reserve_call(&s, 2, &b, "research", 100);
        let r = report_call(&s, 2, &b, "research", usage(), false);
        assert_eq!(r["run"]["known_tokens"], 15);
        assert_eq!(r["run"]["status"], "quality_failed");
        assert!(valid_plan(
            &serde_json::from_value::<Vec<TaskSpec>>(plan()).unwrap()
        ));
        let mut bad: Vec<TaskSpec> = serde_json::from_value(plan()).unwrap();
        bad[0].depends_on.push("deliver".into());
        assert!(!valid_plan(&bad));
    }
    #[test]
    fn provenance_deadline_and_storage_boundaries() {
        let s = fixture();
        let c = brief(&s, "provenance-duel", 200);
        let root = version(&s, 1, None, true);
        let child = version(&s, 2, Some(&root), false);
        let (a, t) = actor(2);
        assert_eq!(
            start(
                &s,
                &a,
                ORIGIN,
                &t,
                &json!({"version":child,"challenge_id":c,"client_id":"child-owner-01"})
            )["error"],
            "publish_derivative_first"
        );
        publish(&s, &a, ORIGIN, &t, &child);
        let first = start_run(&s, 1, &root, &c, "root-owner-01");
        let second = start_run(&s, 2, &child, &c, "child-owner-01");
        assert_eq!(
            reserve_call(&s, 1, &first, "research", 201)["error"],
            "budget_exhausted"
        );
        reserve_call(&s, 1, &first, "research", 100);
        s.strategy_cycle
            .lock()
            .unwrap()
            .challenges
            .get_mut(&c)
            .unwrap()
            .started_at = Some(now() - 181);
        assert_eq!(
            reserve_call(&s, 2, &second, "research", 100)["error"],
            "round_ended"
        );
        assert_eq!(
            report_call(&s, 1, &first, "research", usage(), true)["run"]["known_tokens"],
            15
        );
        let path = s.snapshot_path.clone();
        let saved = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            create(
                s.as_ref(),
                &a,
                ORIGIN,
                &t,
                &json!({"parent":null,"title":"new","plan":plan(),"change":"","published":true})
            )["error"],
            "storage_failed"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(path, saved).unwrap();
        assert_eq!(s.strategy_cycle.lock().unwrap().versions.len(), 2);
        let mut malformed = serde_json::to_value(&*s.strategy_cycle.lock().unwrap()).unwrap();
        malformed.as_object_mut().unwrap().remove("runs");
        assert!(serde_json::from_value::<StrategyState>(malformed).is_err());
    }
    #[test]
    fn signed_marketing_duel_http() {
        let s = fixture();
        let addr = crate::api::serve_on(s.clone(), "127.0.0.1:0", 60_000).unwrap();
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/agent-bootstrap/strategy-http-check.mjs");
        let output = std::process::Command::new("node")
            .arg(script)
            .arg(format!("http://{addr}"))
            .arg(s.snapshot_path.parent().unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(validate(&s.strategy_cycle.lock().unwrap()));
    }
    #[test]
    fn signed_human_comparison_http() {
        let s = fixture();
        let addr = crate::api::serve_on(s.clone(), "127.0.0.1:0", 60_000).unwrap();
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/agent-bootstrap/comparison-http-check.mjs");
        let output = std::process::Command::new("node")
            .arg(script)
            .arg(format!("http://{addr}"))
            .arg(s.snapshot_path.parent().unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(validate(&s.strategy_cycle.lock().unwrap()));
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert!(validate(&restored.strategy_cycle.lock().unwrap()));
        assert!(!restored
            .strategy_cycle
            .lock()
            .unwrap()
            .comparisons
            .is_empty());
    }
}
