//! Exact receipt proof for epoch-2 donkey attempts.  It never chooses outcomes.
use {
    alashi::{
        events::{DonkeyAttempted, DonkeyResolved},
        id,
    },
    anchor_lang::{prelude::Pubkey, AnchorDeserialize},
    serde_json::Value,
    sha2::{Digest, Sha256},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AttemptReceipt {
    pub(super) signature: String,
    pub(super) slot: u64,
}

fn discriminator(name: &[u8]) -> [u8; 8] {
    let mut hash = Sha256::new();
    hash.update(name);
    let mut out = [0; 8];
    out.copy_from_slice(&hash.finalize()[..8]);
    out
}

fn b64(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        let final_chunk = i + 1 == bytes.len() / 4;
        let a = b64(chunk[0])?;
        let b = b64(chunk[1])?;
        let c_pad = chunk[2] == b'=';
        let d_pad = chunk[3] == b'=';
        if (c_pad && (!d_pad || !final_chunk)) || (d_pad && !final_chunk) {
            return None;
        }
        let c = if c_pad { 0 } else { b64(chunk[2])? };
        let d = if d_pad { 0 } else { b64(chunk[3])? };
        out.push((a << 2) | (b >> 4));
        if !c_pad {
            out.push((b << 4) | (c >> 2));
        }
        if !d_pad {
            out.push((c << 6) | d);
        }
    }
    Some(out)
}

fn scoped_data(receipt: &Value) -> Vec<Vec<u8>> {
    let Some(logs) = receipt["log_messages"].as_array() else {
        return Vec::new();
    };
    let program = id().to_string();
    let mut stack = Vec::new();
    let mut out = Vec::new();
    for value in logs {
        let Some(line) = value.as_str() else { continue };
        if let Some(rest) = line.strip_prefix("Program ") {
            if let Some((key, _)) = rest.split_once(" invoke [") {
                stack.push(key);
            } else if rest.ends_with(" success") || rest.contains(" failed:") {
                stack.pop();
            }
        }
        if stack.last().copied() == Some(program.as_str()) {
            if let Some(data) = line.strip_prefix("Program data: ").and_then(decode_base64) {
                out.push(data);
            }
        }
    }
    out
}

/// Returns a receipt only when the successful Alashi invocation emitted the
/// exact attempt event.  A successful tx alone never authorizes a risk claim.
pub(super) fn verified_attempt(
    receipt: &Value,
    game: Pubkey,
    faction: Pubkey,
    round: u8,
    fee: u64,
) -> Option<AttemptReceipt> {
    if receipt["status"] != "confirmed" {
        return None;
    }
    let signature = receipt["signature"].as_str()?.to_owned();
    let slot = receipt["slot"].as_u64()?;
    let disc = discriminator(b"event:DonkeyAttempted");
    scoped_data(receipt)
        .into_iter()
        .filter_map(|raw| {
            raw.starts_with(&disc)
                .then(|| DonkeyAttempted::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .any(|event| {
            event.game == game
                && event.faction == faction
                && event.round == round
                && event.fee == fee
        })
        .then_some(AttemptReceipt { signature, slot })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Resolution {
    Win,
    Empty,
    Abort,
}

pub(super) fn verified_resolution(
    receipt: &Value,
    game: Pubkey,
    faction: Pubkey,
    round: u8,
    fee: u64,
) -> Option<Resolution> {
    if receipt["status"] != "confirmed" {
        return None;
    }
    let disc = discriminator(b"event:DonkeyResolved");
    scoped_data(receipt)
        .into_iter()
        .filter_map(|raw| {
            raw.starts_with(&disc)
                .then(|| DonkeyResolved::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .find_map(|event| {
            (event.game == game
                && event.faction == faction
                && event.round == round
                && event.fee == fee)
                .then(|| match (event.reason, event.success, event.goods) {
                    (0, true, 5) => Some(Resolution::Win),
                    (0, false, 0) => Some(Resolution::Empty),
                    (1, false, 0) => Some(Resolution::Abort),
                    _ => None,
                })
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::Event;
    use serde_json::json;

    fn b64_encode(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let a = c[0];
            let b = *c.get(1).unwrap_or(&0);
            let d = *c.get(2).unwrap_or(&0);
            out.push(T[(a >> 2) as usize] as char);
            out.push(T[((a & 3) << 4 | b >> 4) as usize] as char);
            out.push(if c.len() > 1 {
                T[((b & 15) << 2 | d >> 6) as usize] as char
            } else {
                '='
            });
            out.push(if c.len() > 2 {
                T[(d & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    #[test]
    fn only_exact_scoped_attempt_event_proves_attempt() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let event = DonkeyAttempted {
            game,
            faction,
            round: 2,
            fee: 5_000_000,
        };
        let program = id().to_string();
        let receipt = json!({"status":"confirmed", "signature":"sig", "slot":7,
          "log_messages":[format!("Program {program} invoke [1]"), format!("Program data: {}", b64_encode(&event.data())), format!("Program {program} success")]});
        assert_eq!(
            verified_attempt(&receipt, game, faction, 2, 5_000_000).map(|v| v.slot),
            Some(7)
        );
        assert!(verified_attempt(&receipt, game, faction, 1, 5_000_000).is_none());
    }

    #[test]
    fn resolution_never_treats_abort_as_empty_draw() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let event = DonkeyResolved {
            game,
            faction,
            round: 2,
            success: false,
            goods: 0,
            fee: 5_000_000,
            reason: 1,
        };
        let program = id().to_string();
        let receipt = json!({"status":"confirmed", "signature":"sig", "slot":7,
          "log_messages":[format!("Program {program} invoke [1]"), format!("Program data: {}", b64_encode(&event.data())), format!("Program {program} success")]});
        assert_eq!(
            verified_resolution(&receipt, game, faction, 2, 5_000_000),
            Some(Resolution::Abort)
        );
    }
}
