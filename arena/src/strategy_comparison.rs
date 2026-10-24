//! Private, frozen single-instruction comparisons. These runs never enter duel standings.
use super::*;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub source: String,
    pub provider: String,
    pub model: String,
    pub temperature: u8,
    pub max_tokens: u16,
}
impl Profile {
    fn valid(&self) -> bool {
        matches!(self.source.as_str(), "provider_reported" | "test_provider")
            && text(&self.provider, 128)
            && text(&self.model, 128)
            && self.temperature == 0
            && self.max_tokens == 256
    }
    fn matches(&self, spec: &TaskSpec, source: &str, provider: &str, model: &str) -> bool {
        if spec.kind == "tool" {
            source == "local_tool" && provider == "alashi-local-tools" && model == "none"
        } else {
            source == self.source && provider == self.provider && model == self.model
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub record: String,
    pub owner: String,
    pub challenge_id: String,
    pub baseline_version: String,
    pub candidate_version: String,
    pub hypothesis: String,
    pub prediction: String,
    pub decision_criterion: String,
    pub changed_task: String,
    pub profile: Profile,
    pub attempt_limit: u8,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Diff {
    pub task: String,
    pub field: String,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub outcome: String,
    pub reason: String,
    pub at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub id: String,
    pub number: u8,
    pub client_id: String,
    pub baseline: Run,
    pub candidate: Run,
    pub decision: Option<Decision>,
    pub created_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub id: String,
    pub content: Content,
    pub actual_diff: Diff,
    pub attempts: Vec<Attempt>,
    pub created_at: i64,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn prose(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.contains('\0')
}
fn client_id(s: &str) -> bool {
    (8..=80).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn diff(data: &StrategyState, c: &Content) -> Result<Diff> {
    if !hex32(&c.record)
        || !text(&c.owner, 100)
        || !prose(&c.hypothesis, 2000)
        || !prose(&c.prediction, 2000)
        || !prose(&c.decision_criterion, 2000)
        || !c.profile.valid()
        || !(1..=5).contains(&c.attempt_limit)
    {
        return Err("bad_comparison");
    }
    let brief = data
        .challenges
        .get(&c.challenge_id)
        .ok_or("unknown_challenge")?;
    if brief.content.trading.is_some() {
        return Err("comparison_requires_static_ad_brief");
    }
    let baseline = data
        .versions
        .get(&c.baseline_version)
        .ok_or("unknown_strategy")?;
    let candidate = data
        .versions
        .get(&c.candidate_version)
        .ok_or("unknown_strategy")?;
    if baseline.content.owner != c.owner || candidate.content.owner != c.owner {
        return Err("comparison_strategy_not_owned");
    }
    let a = &baseline.content.plan;
    let b = &candidate.content.plan;
    if !valid_plan(a) || !valid_plan(b) || a.len() != b.len() {
        return Err("comparison_plan_structure_changed");
    }
    if a.iter().chain(b).any(|t| {
        t.kind == "tool" && !matches!(t.tool.as_deref(), Some("brief_extract" | "copy_check"))
    }) {
        return Err("comparison_tool_not_allowed");
    }
    let mut changed = None;
    for (before, after) in a.iter().zip(b) {
        let mut expected = before.clone();
        expected.instruction = after.instruction.clone();
        if &expected != after {
            return Err("comparison_plan_structure_changed");
        }
        if before.instruction != after.instruction {
            if before.kind != "model" {
                return Err("comparison_requires_model_instruction_change");
            }
            if changed.is_some() || before.id != c.changed_task {
                return Err("comparison_requires_one_instruction_change");
            }
            changed = Some(Diff {
                task: before.id.clone(),
                field: "instruction".into(),
                before: before.instruction.clone(),
                after: after.instruction.clone(),
            });
        }
    }
    changed.ok_or("comparison_requires_one_instruction_change")
}
fn attempt_id(c: &str, client: &str) -> String {
    sha256_hex(&format!("strategy-comparison-attempt-v1:{c}:{client}"))
}
fn run_id(a: &str, label: &str) -> String {
    sha256_hex(&format!("strategy-comparison-run-v1:{a}:{label}"))
}
fn brief_for(data: &StrategyState, c: &Content, a: &Attempt) -> Challenge {
    let mut brief = data.challenges[&c.challenge_id].clone();
    brief.started_at = Some(a.created_at);
    brief
}
fn run_view(data: &StrategyState, c: &Content, a: &Attempt, label: &str) -> Value {
    let r = if label == "baseline" {
        &a.baseline
    } else {
        &a.candidate
    };
    let mut out = view(
        r,
        &data.versions[&r.version],
        &brief_for(data, c, a),
        true,
        true,
    );
    out["label"] = json!(label);
    out["attempt"] = json!(a.number);
    out
}
fn attempt_view(data: &StrategyState, c: &Content, a: &Attempt) -> Value {
    json!({"id":a.id,"number":a.number,"client_id":a.client_id,"baseline":run_view(data,c,a,"baseline"),"candidate":run_view(data,c,a,"candidate"),"decision":a.decision,"created_at":a.created_at})
}
fn comparison_view(data: &StrategyState, c: &Comparison) -> Value {
    json!({"id":c.id,"content":c.content,"actual_diff":c.actual_diff,"attempts":c.attempts.iter().map(|a|attempt_view(data,&c.content,a)).collect::<Vec<_>>(),"created_at":c.created_at})
}
fn permitted<'a>(
    data: &'a StrategyState,
    id: &str,
    owner: &str,
    record: &str,
) -> Result<&'a Comparison> {
    data.comparisons
        .get(id)
        .filter(|c| c.content.owner == owner && c.content.record == record)
        .ok_or("comparison_forbidden")
}
pub(super) fn validate(
    data: &StrategyState,
    ids: &mut BTreeSet<(String, String, String, String)>,
) -> bool {
    if data.comparisons.len() > MAX_ITEMS {
        return false;
    }
    for (id, c) in &data.comparisons {
        if id != &c.id
            || *id != digest(&c.content)
            || diff(data, &c.content).ok().as_ref() != Some(&c.actual_diff)
            || c.attempts.len() > c.content.attempt_limit as usize
            || data.challenges[&c.content.challenge_id].created_at > c.created_at
            || [&c.content.baseline_version, &c.content.candidate_version]
                .iter()
                .any(|id| data.versions[*id].created_at > c.created_at)
        {
            return false;
        }
        let mut clients = BTreeSet::new();
        for (i, a) in c.attempts.iter().enumerate() {
            if a.number as usize != i + 1
                || !client_id(&a.client_id)
                || !clients.insert(&a.client_id)
                || a.id != attempt_id(id, &a.client_id)
                || a.created_at < c.created_at
                || a.created_at
                    .checked_add(
                        data.challenges[&c.content.challenge_id]
                            .content
                            .round_seconds as i64,
                    )
                    .is_none()
            {
                return false;
            }
            let brief = brief_for(data, &c.content, a);
            for (seat, label, r, version) in [
                (0, "baseline", &a.baseline, &c.content.baseline_version),
                (1, "candidate", &a.candidate, &c.content.candidate_version),
            ] {
                let v = &data.versions[version];
                if r.id != run_id(&a.id, label)
                    || r.seat != seat
                    || &r.version != version
                    || r.owner != c.content.owner
                    || r.record != c.content.record
                    || r.challenge != c.content.challenge_id
                    || r.client_id != format!("{}:{label}", a.client_id)
                    || r.created_at != a.created_at
                    || r.budget != brief.content.budget
                    || r.review.is_some()
                    || r.events
                        .first()
                        .is_none_or(|e| e.kind != "comparison_run_started" || e.task != "chief")
                    || r.events.iter().any(|e| e.at < a.created_at)
                    || r.events.windows(2).any(|pair| pair[0].at > pair[1].at)
                    || !valid_run(r, v, &brief, ids)
                    || r.calls.iter().any(|(task, call)| {
                        let spec = &v.content.plan.iter().find(|t| &t.id == task).unwrap();
                        !c.content
                            .profile
                            .matches(spec, &call.source, &call.provider, &call.model)
                            || call.started_at < a.created_at
                            || call.started_at >= brief.deadline().unwrap()
                            || call.finished_at.is_some_and(|t| t < call.started_at)
                            || call.status == "failed" && call.quality_passed != Some(false)
                            || spec.depends_on.iter().any(|d| {
                                r.calls.get(d).is_none_or(|dep| {
                                    !r.passed(d)
                                        || dep.finished_at.is_none_or(|end| end > call.started_at)
                                })
                            })
                    })
                {
                    return false;
                }
                let expected = if r.known() > r.budget
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
                };
                if r.status != "completed" && r.status != expected
                    || expected == "running" && r.known().saturating_add(r.held()) > r.budget
                {
                    return false;
                }
                if let Some(result) = &r.result {
                    if result["artifact"] != json!(r.calls["deliver"].report)
                        || result["checks"] != checks(&brief.content, &result["artifact"])
                        || result["finished_at"].as_i64().is_none_or(|t| {
                            t < r.created_at
                                || r.calls
                                    .values()
                                    .any(|c| c.finished_at.is_none_or(|at| at > t))
                        })
                    {
                        return false;
                    }
                }
            }
            if a.decision.as_ref().is_some_and(|d| {
                !valid_decision(d)
                    || !decision_allowed(a, brief.deadline().unwrap(), &d.outcome, d.at)
            }) {
                return false;
            }
        }
    }
    true
}
fn decision_allowed(a: &Attempt, deadline: i64, outcome: &str, at: i64) -> bool {
    let runs = [&a.baseline, &a.candidate];
    if at < a.created_at
        || runs.iter().any(|r| {
            r.calls
                .values()
                .any(|c| c.status == "reserved" || c.finished_at.is_some_and(|t| t > at))
                || r.status == "running" && at < deadline
                || r.result
                    .as_ref()
                    .is_some_and(|v| v["finished_at"].as_i64().is_none_or(|t| t > at))
        })
    {
        return false;
    }
    if runs.iter().any(|r| r.uncertain()) {
        return outcome == "inconclusive";
    }
    match outcome {
        "adopt_candidate" => a.candidate.status == "completed",
        "keep_baseline" => a.baseline.status == "completed",
        "inconclusive" => true,
        _ => false,
    }
}
fn valid_decision(d: &Decision) -> bool {
    matches!(
        d.outcome.as_str(),
        "keep_baseline" | "adopt_candidate" | "inconclusive"
    ) && prose(&d.reason, 2000)
}
pub fn owned(state: &AppState, record: &str, bearer: &str) -> Value {
    let owner = match owner_auth::owner_session_for(state, record, bearer) {
        Ok(s) => s.wallet,
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
    json!({"ok":true,"comparisons":data.comparisons.values().filter(|c|c.content.owner==owner && c.content.record==record).map(|c|comparison_view(&data,c)).collect::<Vec<_>>()})
}
pub fn create(state: &AppState, record: &str, origin: &str, bearer: &str, body: &Value) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(
        body,
        &[
            "challenge_id",
            "baseline_version",
            "candidate_version",
            "hypothesis",
            "prediction",
            "decision_criterion",
            "changed_task",
            "profile",
            "attempt_limit",
        ],
    ) {
        return err(c);
    }
    let mut supplied = body.clone();
    supplied["owner"] = json!(owner);
    supplied["record"] = json!(record);
    let content: Content = match serde_json::from_value(supplied) {
        Ok(c) => c,
        Err(_) => return err("bad_comparison"),
    };
    mutate(state, |data| {
        let actual_diff = diff(data, &content)?;
        let id = digest(&content);
        if let Some(c) = data.comparisons.get(&id) {
            return Ok(json!({"ok":true,"comparison":comparison_view(data,c)}));
        }
        if data.comparisons.len() >= MAX_ITEMS {
            return Err("strategy_capacity");
        }
        let c = Comparison {
            id: id.clone(),
            content,
            actual_diff,
            attempts: vec![],
            created_at: now(),
        };
        let out = comparison_view(data, &c);
        data.comparisons.insert(id, c);
        Ok(json!({"ok":true,"comparison":out}))
    })
}
pub fn start(
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
    if let Err(c) = fields(body, &["client_id"]) {
        return err(c);
    }
    let client = body["client_id"].as_str().unwrap_or("");
    if !client_id(client) {
        return err("bad_client_id");
    }
    mutate(state, |data| {
        let c = permitted(data, id, &owner, record)?;
        if let Some(a) = c.attempts.iter().find(|a| a.client_id == client) {
            return Ok(
                json!({"ok":true,"comparison":comparison_view(data,c),"attempt":attempt_view(data,&c.content,a)}),
            );
        }
        if c.attempts.len() >= c.content.attempt_limit as usize {
            return Err("comparison_attempt_limit");
        }
        let aid = attempt_id(id, client);
        let at = now();
        let make_run = |seat, label, version: &str| {
            let mut r = Run {
                seat,
                id: run_id(&aid, label),
                owner: owner.clone(),
                record: record.into(),
                client_id: format!("{client}:{label}"),
                version: version.into(),
                challenge: c.content.challenge_id.clone(),
                budget: data.challenges[&c.content.challenge_id].content.budget,
                calls: BTreeMap::new(),
                events: vec![],
                status: "running".into(),
                result: None,
                review: None,
                created_at: at,
            };
            r.event("chief", "comparison_run_started");
            r
        };
        let a = Attempt {
            id: aid.clone(),
            number: c.attempts.len() as u8 + 1,
            client_id: client.into(),
            baseline: make_run(0, "baseline", &c.content.baseline_version),
            candidate: make_run(1, "candidate", &c.content.candidate_version),
            decision: None,
            created_at: at,
        };
        data.comparisons.get_mut(id).unwrap().attempts.push(a);
        let c = &data.comparisons[id];
        Ok(
            json!({"ok":true,"comparison":comparison_view(data,c),"attempt":attempt_view(data,&c.content,c.attempts.last().unwrap())}),
        )
    })
}
pub fn action(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    aid: &str,
    label: &str,
    action: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if !matches!(label, "baseline" | "candidate") {
        return err("bad_comparison_label");
    }
    mutate(state, |data| {
        let c = permitted(data, id, &owner, record)?;
        let a = c
            .attempts
            .iter()
            .find(|a| a.id == aid)
            .ok_or("comparison_attempt_forbidden")?;
        let r = if label == "baseline" {
            &a.baseline
        } else {
            &a.candidate
        };
        if a.decision.is_some()
            && !(action == "finish" && r.status == "completed"
                || action == "report"
                    && body["task"]
                        .as_str()
                        .and_then(|t| r.calls.get(t))
                        .is_some_and(|c| c.status != "reserved"))
        {
            return Err("comparison_decision_frozen");
        }
        let v = data.versions[&r.version].clone();
        let brief = brief_for(data, &c.content, a);
        if action == "report" && duplicate_receipt(data, &r.id, body) {
            return Err("duplicate_provider_receipt");
        }
        if action == "reserve" {
            let spec = v
                .content
                .plan
                .iter()
                .find(|t| Some(t.id.as_str()) == body["task"].as_str())
                .ok_or("bad_task")?;
            if !c.content.profile.matches(
                spec,
                body["source"].as_str().unwrap_or(""),
                body["provider"].as_str().unwrap_or(""),
                body["model"].as_str().unwrap_or(""),
            ) {
                return Err("comparison_provider_mismatch");
            }
        }
        let c = data.comparisons.get_mut(id).unwrap();
        let a = c.attempts.iter_mut().find(|a| a.id == aid).unwrap();
        let r = if label == "baseline" {
            &mut a.baseline
        } else {
            &mut a.candidate
        };
        match action {
            "reserve" => reserve_run(r, &v, &brief, body)?,
            "report" => report_run(r, &brief, body)?,
            "finish" => {
                fields(body, &[])?;
                finish_run(r, &v, &brief)?;
            }
            _ => return Err("bad_strategy_request"),
        }
        let c = &data.comparisons[id];
        let a = c.attempts.iter().find(|a| a.id == aid).unwrap();
        Ok(json!({"ok":true,"run":run_view(data,&c.content,a,label)}))
    })
}
pub fn decision(
    state: &AppState,
    record: &str,
    origin: &str,
    bearer: &str,
    id: &str,
    aid: &str,
    body: &Value,
) -> Value {
    let owner = match own(state, record, origin, bearer) {
        Ok(o) => o,
        Err(c) => return err(c),
    };
    if let Err(c) = fields(body, &["outcome", "reason"]) {
        return err(c);
    }
    let d = Decision {
        outcome: body["outcome"].as_str().unwrap_or("").into(),
        reason: body["reason"].as_str().unwrap_or("").trim().into(),
        at: now(),
    };
    if !valid_decision(&d) {
        return err("bad_comparison_decision");
    }
    mutate(state, |data| {
        let c = permitted(data, id, &owner, record)?;
        let a = c
            .attempts
            .iter()
            .find(|a| a.id == aid)
            .ok_or("comparison_attempt_forbidden")?;
        let deadline = brief_for(data, &c.content, a).deadline().unwrap();
        let a = data
            .comparisons
            .get_mut(id)
            .unwrap()
            .attempts
            .iter_mut()
            .find(|a| a.id == aid)
            .ok_or("comparison_attempt_forbidden")?;
        if !decision_allowed(a, deadline, &d.outcome, d.at) {
            return Err("comparison_pair_incomplete");
        }
        if let Some(old) = &a.decision {
            if old.outcome != d.outcome || old.reason != d.reason {
                return Err("idempotency_conflict");
            }
        } else {
            a.decision = Some(d);
        }
        let c = &data.comparisons[id];
        let a = c.attempts.iter().find(|a| a.id == aid).unwrap();
        Ok(
            json!({"ok":true,"comparison":comparison_view(data,c),"attempt":attempt_view(data,&c.content,a)}),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{actor, brief, fixture, plan};
    use super::*;
    use crate::api::{load_snapshot, new_state_with_files};
    const ORIGIN: &str = "https://alashi.network";
    fn version(s: &AppState, i: usize, parent: Option<&str>, p: Value) -> String {
        let (record, bearer) = actor(i);
        let out = super::super::create(
            s,
            &record,
            ORIGIN,
            &bearer,
            &json!({"parent":parent,"title":"Private ad plan","plan":p,"change":if parent.is_some(){"Change audience instruction"}else{""},"published":false}),
        );
        assert_eq!(out["ok"], true, "{out}");
        out["version"]["id"].as_str().unwrap().into()
    }
    fn setup(s: &AppState) -> (String, Value) {
        let (record, bearer) = actor(0);
        let challenge = brief(s, "comparison-ad", 10_000);
        let baseline = version(s, 0, None, plan());
        let mut changed = plan();
        changed[0]["instruction"] =
            json!("Review facts specifically for independent agent owners.");
        let candidate = version(s, 0, Some(&baseline), changed);
        let body = json!({"challenge_id":challenge,"baseline_version":baseline,"candidate_version":candidate,"hypothesis":"PRIVATE-HYPOTHESIS: audience specificity helps this ad\nCheck the required terms.","prediction":"The candidate meets the format checks using fewer tokens","decision_criterion":"Adopt only if both formats pass and candidate uses fewer reported tokens","changed_task":"research","profile":{"source":"test_provider","provider":"local-fixture","model":"deterministic-test","temperature":0,"max_tokens":256},"attempt_limit":2});
        let out = create(s, &record, ORIGIN, &bearer, &body);
        assert_eq!(out["ok"], true, "{out}");
        (out["comparison"]["id"].as_str().unwrap().into(), body)
    }
    fn attempt(s: &AppState, c: &str, client: &str) -> Value {
        let (record, bearer) = actor(0);
        let out = start(s, &record, ORIGIN, &bearer, c, &json!({"client_id":client}));
        assert_eq!(out["ok"], true, "{out}");
        out["attempt"].clone()
    }
    fn action_for(s: &AppState, c: &str, a: &str, label: &str, op: &str, body: Value) -> Value {
        let (record, bearer) = actor(0);
        action(s, &record, ORIGIN, &bearer, c, a, label, op, &body)
    }
    fn reserve(s: &AppState, c: &str, a: &str, label: &str, task: &str) -> Value {
        action_for(
            s,
            c,
            a,
            label,
            "reserve",
            json!({"task":task,"max_total_tokens":100,"source":"test_provider","provider":"local-fixture","model":"deterministic-test"}),
        )
    }
    fn report(
        s: &AppState,
        c: &str,
        a: &str,
        label: &str,
        task: &str,
        usage: Value,
        failed: bool,
    ) -> Value {
        let artifact = if task == "deliver" {
            json!({"headline":"Build with ALASHI","body":"Bring your agents and tools to a shared challenge. Spend model tokens on your plan and compare the evidence.","cta":"Explore ALASHI"})
        } else {
            json!({"facts":"from fixed brief"})
        };
        action_for(
            s,
            c,
            a,
            label,
            "report",
            json!({"task":task,"response_id":format!("{a}-{label}-{task}"),"usage":usage,"report":if failed{Value::Null}else{artifact},"request_failed":failed}),
        )
    }
    fn usage() -> Value {
        json!({"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"cached_tokens":0})
    }
    fn complete(s: &AppState, c: &str, a: &str, label: &str) {
        for task in ["research", "deliver"] {
            assert_eq!(reserve(s, c, a, label, task)["ok"], true);
            let r = report(s, c, a, label, task, usage(), false);
            assert_eq!(r["ok"], true, "{r}");
        }
        let r = action_for(s, c, a, label, "finish", json!({}));
        assert_eq!(r["ok"], true, "{r}");
    }
    fn decide(s: &AppState, c: &str, a: &str, outcome: &str) -> Value {
        let (record, bearer) = actor(0);
        decision(
            s,
            &record,
            ORIGIN,
            &bearer,
            c,
            a,
            &json!({"outcome":outcome,"reason":"Human interpretation of this controlled format-only experiment"}),
        )
    }
    #[test]
    fn private_comparison_pair_is_frozen_atomic_idempotent_and_not_a_duel() {
        let s = fixture();
        let (c, body) = setup(&s);
        let (record, bearer) = actor(0);
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["comparison"]["id"],
            c
        );
        let a = attempt(&s, &c, "pair-attempt-001");
        let aid = a["id"].as_str().unwrap();
        assert_eq!(attempt(&s, &c, "pair-attempt-001"), a);
        assert_ne!(a["baseline"]["id"], a["candidate"]["id"]);
        assert_eq!(a["baseline"]["deadline_at"], a["candidate"]["deadline_at"]);
        assert_eq!(a["baseline"]["brief"], a["candidate"]["brief"]);
        assert_eq!(a["baseline"]["budget"], a["candidate"]["budget"]);
        assert_eq!(
            decide(&s, &c, aid, "adopt_candidate")["error"],
            "comparison_pair_incomplete"
        );
        let mismatch = action_for(
            &s,
            &c,
            aid,
            "baseline",
            "reserve",
            json!({"task":"research","max_total_tokens":100,"source":"test_provider","provider":"different","model":"deterministic-test"}),
        );
        assert_eq!(mismatch["error"], "comparison_provider_mismatch");
        let (other, token) = actor(1);
        let forbidden_fork = super::super::create(
            &s,
            &other,
            ORIGIN,
            &token,
            &json!({"parent":body["baseline_version"],"title":"Private parent theft","plan":plan(),"change":"Copy another owner","published":false}),
        );
        assert_eq!(forbidden_fork["error"], "public_parent_required");
        assert!(owned(&s, &other, &token)["comparisons"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            start(
                &s,
                &other,
                ORIGIN,
                &token,
                &c,
                &json!({"client_id":"forbidden-pair"})
            )["error"],
            "comparison_forbidden"
        );
        assert_eq!(
            action(
                &s,
                &other,
                ORIGIN,
                &token,
                &c,
                aid,
                "baseline",
                "reserve",
                &json!({})
            )["error"],
            "comparison_forbidden"
        );
        let public = super::super::public(&s);
        assert!(!public.to_string().contains("PRIVATE-HYPOTHESIS"));
        assert!(public["runs"].as_array().unwrap().is_empty());
        assert!(public["versions"].as_array().unwrap().is_empty());
        let duel = super::super::start(
            &s,
            &record,
            ORIGIN,
            &bearer,
            &json!({"version":body["candidate_version"],"challenge_id":body["challenge_id"],"client_id":"private-duel-01"}),
        );
        assert_eq!(duel["error"], "publish_derivative_first");
        complete(&s, &c, aid, "baseline");
        assert_eq!(reserve(&s, &c, aid, "candidate", "research")["ok"], true);
        let duplicate = action_for(
            &s,
            &c,
            aid,
            "candidate",
            "report",
            json!({"task":"research","response_id":format!("{aid}-baseline-research"),"usage":usage(),"report":{"facts":"fixed"},"request_failed":false}),
        );
        assert_eq!(duplicate["error"], "duplicate_provider_receipt");
        complete(&s, &c, aid, "candidate");
        assert_eq!(decide(&s, &c, aid, "inconclusive")["ok"], true);
        assert_eq!(decide(&s, &c, aid, "inconclusive")["ok"], true);
        assert_eq!(
            decide(&s, &c, aid, "adopt_candidate")["error"],
            "idempotency_conflict"
        );
        assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert_eq!(
            owned(&restored, &record, &bearer)["comparisons"][0]["attempts"][0]["decision"]
                ["outcome"],
            "inconclusive"
        );
        assert!(super::super::validate(
            &restored.strategy_cycle.lock().unwrap()
        ));
    }
    #[test]
    fn private_parent_lineage_must_be_public_before_child_publishing() {
        let s = fixture();
        let (_, body) = setup(&s);
        let parent = body["baseline_version"].as_str().unwrap();
        let child = body["candidate_version"].as_str().unwrap();
        let (record, bearer) = actor(0);
        let (other, other_bearer) = actor(1);
        let data = s.strategy_cycle.lock().unwrap().clone();
        assert!(super::super::validate(&data));
        let mut public_create = serde_json::to_value(&data.versions[child].content).unwrap();
        public_create.as_object_mut().unwrap().remove("owner");
        public_create["published"] = json!(true);
        assert_eq!(
            super::super::create(&s, &record, ORIGIN, &bearer, &public_create)["error"],
            "public_parent_required"
        );
        assert_eq!(
            super::super::publish(&s, &record, ORIGIN, &bearer, child)["error"],
            "public_parent_required"
        );
        assert_eq!(
            super::super::publish(&s, &other, ORIGIN, &other_bearer, child)["error"],
            "strategy_forbidden"
        );
        assert!(!s.strategy_cycle.lock().unwrap().versions[child].published);
        let mut invalid = data;
        invalid.versions.get_mut(child).unwrap().published = true;
        assert!(!super::super::validate(&invalid));
        assert_eq!(
            super::super::publish(&s, &record, ORIGIN, &bearer, parent)["ok"],
            true
        );
        assert_eq!(
            super::super::publish(&s, &record, ORIGIN, &bearer, child)["ok"],
            true
        );
        assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
        let public = super::super::public(&s);
        assert_eq!(public["versions"].as_array().unwrap().len(), 2);
        assert!(public["versions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == parent));
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert!(super::super::validate(
            &restored.strategy_cycle.lock().unwrap()
        ));
    }
    #[test]
    fn decision_freezes_expired_unfinished_run_and_preserves_reload() {
        let s = fixture();
        let (c, _) = setup(&s);
        let a = attempt(&s, &c, "expired-attempt-01");
        let aid = a["id"].as_str().unwrap();
        for label in ["baseline", "candidate"] {
            for task in ["research", "deliver"] {
                assert_eq!(reserve(&s, &c, aid, label, task)["ok"], true);
                assert_eq!(report(&s, &c, aid, label, task, usage(), false)["ok"], true);
            }
        }
        {
            let mut data = s.strategy_cycle.lock().unwrap();
            let cmp = data.comparisons.get_mut(&c).unwrap();
            cmp.created_at -= 181;
            let a = &mut cmp.attempts[0];
            a.created_at -= 181;
            for r in [&mut a.baseline, &mut a.candidate] {
                r.created_at -= 181;
                for event in &mut r.events {
                    event.at -= 181;
                }
                for call in r.calls.values_mut() {
                    call.started_at -= 181;
                    call.finished_at = call.finished_at.map(|t| t - 181);
                }
            }
            // Immutable source versions/brief must already exist before the comparison.
            for v in data.versions.values_mut() {
                v.created_at -= 181;
            }
            for b in data.challenges.values_mut() {
                b.created_at -= 181;
            }
        }
        assert_eq!(decide(&s, &c, aid, "inconclusive")["ok"], true);
        assert_eq!(
            action_for(&s, &c, aid, "baseline", "finish", json!({}))["error"],
            "comparison_decision_frozen"
        );
        assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
        let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
        load_snapshot(&restored).unwrap();
        assert!(super::super::validate(
            &restored.strategy_cycle.lock().unwrap()
        ));
    }
    #[test]
    fn comparison_rejects_multiple_deltas_structure_tools_and_wrong_owner() {
        let s = fixture();
        let (_, mut body) = setup(&s);
        let (record, bearer) = actor(0);
        let base = body["baseline_version"].as_str().unwrap().to_owned();
        let mut p = plan();
        p[0]["instruction"] = json!("Different");
        p[1]["instruction"] = json!("Also different");
        body["candidate_version"] = json!(version(&s, 0, Some(&base), p));
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["error"],
            "comparison_requires_one_instruction_change"
        );
        let mut p = plan();
        p[0]["instruction"] = json!("Different");
        p[0]["id"] = json!("other");
        p[1]["depends_on"] = json!(["other"]);
        body["candidate_version"] = json!(version(&s, 0, Some(&base), p));
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["error"],
            "comparison_plan_structure_changed"
        );
        let mut p = plan();
        p[0]["kind"] = json!("tool");
        p[0]["tool"] = json!("custom-executable");
        let toolbase = version(&s, 0, None, p.clone());
        p[1]["instruction"] = json!("Different instruction");
        body["baseline_version"] = json!(toolbase);
        body["candidate_version"] = json!(version(&s, 0, None, p));
        body["changed_task"] = json!("deliver");
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["error"],
            "comparison_tool_not_allowed"
        );
        body["candidate_version"] = json!(version(&s, 1, None, plan()));
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["error"],
            "comparison_strategy_not_owned"
        );
        body["profile"]["temperature"] = json!(1);
        assert_eq!(
            create(&s, &record, ORIGIN, &bearer, &body)["error"],
            "bad_comparison"
        );
    }
    #[test]
    fn failed_baseline_allows_human_choice_but_unknown_usage_only_inconclusive() {
        for unknown in [false, true] {
            let s = fixture();
            let (c, _) = setup(&s);
            let a = attempt(&s, &c, "failure-attempt-01");
            let aid = a["id"].as_str().unwrap();
            assert_eq!(reserve(&s, &c, aid, "baseline", "research")["ok"], true);
            let r = report(
                &s,
                &c,
                aid,
                "baseline",
                "research",
                if unknown { Value::Null } else { usage() },
                true,
            );
            assert_eq!(r["ok"], true, "{r}");
            complete(&s, &c, aid, "candidate");
            let d = decide(&s, &c, aid, "adopt_candidate");
            if unknown {
                assert_eq!(d["error"], "comparison_pair_incomplete");
                assert_eq!(decide(&s, &c, aid, "inconclusive")["ok"], true);
            } else {
                assert_eq!(d["ok"], true, "{d}");
            }
            assert!(super::super::validate(&s.strategy_cycle.lock().unwrap()));
            let restored = new_state_with_files(s.snapshot_path.clone(), s.sequence_path.clone());
            load_snapshot(&restored).unwrap();
            assert!(super::super::validate(
                &restored.strategy_cycle.lock().unwrap()
            ));
        }
    }
    #[test]
    fn comparison_reload_rejects_tampered_bindings_diff_profile_receipts_and_decision() {
        let s = fixture();
        let (c, _) = setup(&s);
        let a = attempt(&s, &c, "tamper-attempt-01");
        let aid = a["id"].as_str().unwrap();
        complete(&s, &c, aid, "baseline");
        complete(&s, &c, aid, "candidate");
        let data = s.strategy_cycle.lock().unwrap().clone();
        assert!(super::super::validate(&data));
        let mut bad = data.clone();
        bad.comparisons.get_mut(&c).unwrap().actual_diff.after = "forged".into();
        assert!(!super::super::validate(&bad));
        let mut bad = data.clone();
        bad.comparisons.get_mut(&c).unwrap().attempts[0]
            .candidate
            .version = bad.comparisons[&c].content.baseline_version.clone();
        assert!(!super::super::validate(&bad));
        let mut bad = data.clone();
        bad.comparisons.get_mut(&c).unwrap().attempts[0]
            .candidate
            .calls
            .get_mut("research")
            .unwrap()
            .provider = "other".into();
        assert!(!super::super::validate(&bad));
        let mut bad = data.clone();
        let response = bad.comparisons[&c].attempts[0].baseline.calls["research"]
            .response_id
            .clone();
        bad.comparisons.get_mut(&c).unwrap().attempts[0]
            .candidate
            .calls
            .get_mut("research")
            .unwrap()
            .response_id = response;
        assert!(!super::super::validate(&bad));
        let mut bad = data.clone();
        bad.comparisons.get_mut(&c).unwrap().attempts[0].decision = Some(Decision {
            outcome: "adopt_candidate".into(),
            reason: "Too early".into(),
            at: 0,
        });
        assert!(!super::super::validate(&bad));
        let mut bad = data.clone();
        bad.comparisons.get_mut(&c).unwrap().attempts[0].created_at = i64::MAX;
        assert!(!super::super::validate(&bad));
        let mut legacy = serde_json::to_value(StrategyState::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("comparisons");
        let legacy: StrategyState = serde_json::from_value(legacy).unwrap();
        assert!(super::super::validate(&legacy));
    }
}
