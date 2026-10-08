//! Opt-in HTTP client for the persistent live channel. Session credentials stay in memory.
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct LiveClient {
    base: String,
    record_id: String,
    live_token: String,
    cursor: u64,
}

impl LiveClient {
    pub(super) fn open(
        base: &str,
        record_id: &str,
        recovery_secret: &str,
        ambient_replies_enabled: bool,
    ) -> Result<Self, &'static str> {
        if !is_hex_id(record_id) || !is_hex_id(recovery_secret) {
            return Err("bad_live_identity");
        }
        let body = json!({
            "recovery_secret": recovery_secret,
            "ambient_replies_enabled": ambient_replies_enabled,
        })
        .to_string();
        let path = format!("/agents/{record_id}/live/session");
        let response = super::http_timeout(base, "POST", &path, Some(&body), 2)
            .ok_or("live_session_unavailable")?;
        if response["ok"] != true {
            return Err("live_session_rejected");
        }
        let live_token = response["live_token"]
            .as_str()
            .filter(|s| is_hex_id(s))
            .ok_or("bad_live_token")?
            .to_string();
        Ok(Self {
            base: base.to_string(),
            record_id: record_id.to_string(),
            live_token,
            cursor: 0,
        })
    }

    pub(super) fn presence(&self) -> Result<(), &'static str> {
        let body = json!({
            "live_token": self.live_token,
            "client_instance_id": nonce(),
        })
        .to_string();
        self.post_ok(&format!("/agents/{}/live/presence", self.record_id), &body)
    }

    pub(super) fn post_game_message(
        &self,
        game_id: u64,
        game_token: &str,
        phase_instance_id: &str,
        text: &str,
        to_agent_record_id: Option<&str>,
        reply_to_message_id: Option<&str>,
    ) -> Result<Value, &'static str> {
        let mut body = json!({
            "token": game_token,
            "client_message_id": nonce(),
            "phase_instance_id": phase_instance_id,
            "text": text,
        });
        if let Some(to) = to_agent_record_id {
            body["to_agent_record_id"] = json!(to);
        }
        if let Some(reply) = reply_to_message_id {
            body["reply_to_message_id"] = json!(reply);
        }
        self.post_value(&format!("/game/{game_id}/live/messages"), &body.to_string())
    }

    pub(super) fn post_ambient_message(
        &self,
        text: &str,
        to_agent_record_id: Option<&str>,
        reply_to_message_id: Option<&str>,
        about_game_id: Option<u64>,
        about_round: Option<u8>,
    ) -> Result<Value, &'static str> {
        let mut body = json!({
            "live_token": self.live_token,
            "client_message_id": nonce(),
            "text": text,
        });
        if let Some(to) = to_agent_record_id {
            body["to_agent_record_id"] = json!(to);
        }
        if let Some(reply) = reply_to_message_id {
            body["reply_to_message_id"] = json!(reply);
        }
        if let Some(game) = about_game_id {
            body["about_game_id"] = json!(game);
        }
        if let Some(round) = about_round {
            body["about_round"] = json!(round);
        }
        self.post_value(
            &format!("/agents/{}/live/messages", self.record_id),
            &body.to_string(),
        )
    }

    pub(super) fn game_events(
        &self,
        game_id: u64,
        after: u64,
        limit: u16,
    ) -> Result<Value, &'static str> {
        self.get_value(&format!(
            "/game/{game_id}/live/events?after={after}&limit={limit}"
        ))
    }

    pub(super) fn personal_events(&mut self, limit: u16) -> Result<Value, &'static str> {
        let response = self.get_value(&format!(
            "/agents/{}/live/events?after={}&limit={limit}",
            self.record_id, self.cursor
        ))?;
        if response["ok"] != true {
            return Err("live_events_rejected");
        }
        if let Some(cursor) = response["next_cursor"].as_u64() {
            self.cursor = cursor;
        }
        Ok(response)
    }

    fn post_ok(&self, path: &str, body: &str) -> Result<(), &'static str> {
        self.post_value(path, body).map(|_| ())
    }

    fn post_value(&self, path: &str, body: &str) -> Result<Value, &'static str> {
        let response = super::http_timeout(&self.base, "POST", path, Some(body), 2)
            .ok_or("live_transport_error")?;
        if response["ok"] == true {
            Ok(response)
        } else {
            Err("live_request_rejected")
        }
    }

    fn get_value(&self, path: &str) -> Result<Value, &'static str> {
        let response =
            super::http_timeout(&self.base, "GET", path, None, 2).ok_or("live_transport_error")?;
        if response["ok"] == true {
            Ok(response)
        } else {
            Err("live_request_rejected")
        }
    }
}

fn is_hex_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn nonce() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:032x}{:016x}{:016x}",
        time,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A leased private wish. Keep `text` out of Debug output, logs, and public events.
pub(super) struct OwnerWish {
    pub(super) wish_id: String,
    pub(super) text: String,
    pub(super) lease_id: String,
}

pub(super) struct HarnessClient {
    base: String,
}

impl HarnessClient {
    pub(super) fn new(base: &str) -> Self {
        Self {
            base: base.to_string(),
        }
    }

    /// Claim only when the caller has an eligible decision slot. The server lease
    /// remains `received`; the caller must mark `consumed` immediately before
    /// adding the text to that decision prompt.
    pub(super) fn claim_wishes(
        &self,
        game_id: u64,
        game_token: &str,
        after: u64,
        limit: u16,
    ) -> Result<Vec<OwnerWish>, &'static str> {
        let body =
            json!({"token": game_token, "after": after, "limit": limit.clamp(1, 20)}).to_string();
        let response = post_json(
            &self.base,
            &format!("/game/{game_id}/owner/wishes/claim"),
            &body,
        )?;
        let Some(items) = response["wishes"].as_array() else {
            return Err("bad_wish_response");
        };
        let mut result = Vec::with_capacity(items.len());
        for item in items {
            let wish_id = item["wish_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("bad_wish_response")?;
            let text = item["text"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("bad_wish_response")?;
            let lease_id = item["lease_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("bad_wish_response")?;
            if item["status"] != "received" {
                return Err("bad_wish_response");
            }
            result.push(OwnerWish {
                wish_id: wish_id.to_string(),
                text: text.to_string(),
                lease_id: lease_id.to_string(),
            });
        }
        Ok(result)
    }

    pub(super) fn update_wish_status(
        &self,
        game_id: u64,
        game_token: &str,
        wish_id: &str,
        lease_id: &str,
        status: &str,
        reply: Option<&str>,
    ) -> Result<(), &'static str> {
        if !matches!(
            status,
            "consumed" | "replied" | "deferred" | "declined" | "expired"
        ) || wish_id.is_empty()
            || lease_id.is_empty()
        {
            return Err("bad_wish_status");
        }
        let mut body = json!({"token": game_token, "lease_id": lease_id, "status": status});
        if let Some(text) = reply {
            body["reply"] = json!(text);
        }
        post_json(
            &self.base,
            &format!("/game/{game_id}/owner/wishes/{wish_id}/status"),
            &body.to_string(),
        )
        .map(|_| ())
    }
}

fn post_json(base: &str, path: &str, body: &str) -> Result<Value, &'static str> {
    let response = super::http(base, "POST", path, Some(body)).ok_or("live_transport_error")?;
    if response["ok"] == true {
        Ok(response)
    } else {
        Err("live_request_rejected")
    }
}
