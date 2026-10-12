//! Receipt-level proof for the epoch-1 deterministic barter demonstration.
//! This module never sends transactions or HTTP writes.

use {
    alashi::{
        events::{BarterAccepted, BarterProposed, Produced, Sold},
        id,
    },
    anchor_lang::prelude::Pubkey,
    anchor_lang::AnchorDeserialize,
    serde_json::Value,
    sha2::{Digest, Sha256},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConfirmedReceipt {
    pub(super) signature: String,
    pub(super) slot: u64,
}

fn event_disc(name: &str) -> [u8; 8] {
    let mut hash = Sha256::new();
    hash.update(name.as_bytes());
    let digest = hash.finalize();
    let mut out = [0; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

fn b64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Strict standard Base64 parser for Anchor Program data log payload.
fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let final_chunk = chunk_index + 1 == bytes.len() / 4;
        let a = b64_value(chunk[0])?;
        let b = b64_value(chunk[1])?;
        let c_pad = chunk[2] == b'=';
        let d_pad = chunk[3] == b'=';
        if (c_pad && (!d_pad || !final_chunk)) || (d_pad && !final_chunk) {
            return None;
        }
        let c = if c_pad { 0 } else { b64_value(chunk[2])? };
        let d = if d_pad { 0 } else { b64_value(chunk[3])? };
        result.push((a << 2) | (b >> 4));
        if !c_pad {
            result.push((b << 4) | (c >> 2));
        }
        if !d_pad {
            result.push((c << 6) | d);
        }
    }
    Some(result)
}

fn receipt_identity(receipt: &Value) -> Option<ConfirmedReceipt> {
    if receipt["status"] != "confirmed" {
        return None;
    }
    let signature = receipt["signature"].as_str()?;
    if signature.is_empty() || signature.len() > 128 {
        return None;
    }
    Some(ConfirmedReceipt {
        signature: signature.to_owned(),
        slot: receipt["slot"].as_u64()?,
    })
}

fn program_data(receipt: &Value) -> impl Iterator<Item = Vec<u8>> + '_ {
    receipt["log_messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|line| line.starts_with("Program data: "))
        .filter_map(|line| decode_base64(line.trim_start_matches("Program data: ").trim()))
        .filter(|raw| raw.len() >= 8)
}

/// Confirms the exact on-chain offer event in an already-confirmed transaction.
/// The caller must supply only log lines scoped to the Alashi program.
pub(super) fn verified_offer(
    receipt: &Value,
    game: Pubkey,
    proposer: Pubkey,
    offer_id: u64,
    goods: u16,
    price: u64,
) -> Option<ConfirmedReceipt> {
    let identity = receipt_identity(receipt)?;
    let matching = program_data(receipt)
        .filter_map(|raw| {
            raw.starts_with(&event_disc("event:BarterProposed"))
                .then(|| BarterProposed::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .any(|event| {
            event.game == game
                && event.from == proposer
                && event.offer == offer_id
                && event.goods == goods
                && event.price == price
        });
    matching.then_some(identity)
}

/// Confirms the exact on-chain acceptance event in an already-confirmed transaction.
pub(super) fn verified_accept(
    receipt: &Value,
    game: Pubkey,
    buyer: Pubkey,
    proposer: Pubkey,
    offer_id: u64,
) -> Option<ConfirmedReceipt> {
    let identity = receipt_identity(receipt)?;
    let matching = program_data(receipt)
        .filter_map(|raw| {
            raw.starts_with(&event_disc("event:BarterAccepted"))
                .then(|| BarterAccepted::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .any(|event| {
            event.game == game
                && event.by == buyer
                && event.from == proposer
                && event.offer == offer_id
        });
    matching.then_some(identity)
}

/// Requires exact inventory total from the confirmed, Alashi-scoped event.
pub(super) fn verified_produced(
    receipt: &Value,
    game: Pubkey,
    faction: Pubkey,
    goods: u16,
) -> Option<ConfirmedReceipt> {
    let identity = receipt_identity(receipt)?;
    program_data(receipt)
        .filter_map(|raw| {
            raw.starts_with(&event_disc("event:Produced"))
                .then(|| Produced::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .any(|e| e.game == game && e.faction == faction && e.goods == goods)
        .then_some(identity)
}

/// Requires the exact units and canonical net revenue, not merely a successful tx.
pub(super) fn verified_sold(
    receipt: &Value,
    game: Pubkey,
    faction: Pubkey,
    units: u16,
    revenue: u64,
) -> Option<ConfirmedReceipt> {
    let identity = receipt_identity(receipt)?;
    program_data(receipt)
        .filter_map(|raw| {
            raw.starts_with(&event_disc("event:Sold"))
                .then(|| Sold::try_from_slice(&raw[8..]).ok())
                .flatten()
        })
        .any(|e| e.game == game && e.faction == faction && e.units == units && e.revenue == revenue)
        .then_some(identity)
}

/// Filter a confirmed transaction's logs to the executing Alashi program.
/// A nested program cannot supply evidence for a conversation entry.
pub(super) fn with_alashi_logs(receipt: &Value) -> Value {
    let Some(logs) = receipt["log_messages"].as_array() else {
        return receipt.clone();
    };
    let program = id().to_string();
    let mut stack = Vec::new();
    let mut filtered = Vec::new();
    for value in logs {
        let Some(line) = value.as_str() else { continue };
        if let Some(rest) = line.strip_prefix("Program ") {
            if let Some((key, _)) = rest.split_once(" invoke [") {
                stack.push(key);
            } else if rest.ends_with(" success") || rest.contains(" failed:") {
                stack.pop();
            }
        }
        if line.starts_with("Program data: ") && stack.last().copied() == Some(program.as_str()) {
            filtered.push(Value::String(line.to_owned()));
        }
    }
    let mut filtered_receipt = receipt.clone();
    filtered_receipt["log_messages"] = Value::Array(filtered);
    filtered_receipt
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::Event;
    use serde_json::json;

    fn b64(data: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in data.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            output.push(TABLE[(a >> 2) as usize] as char);
            output.push(TABLE[((a & 0x03) << 4 | b >> 4) as usize] as char);
            output.push(if chunk.len() > 1 {
                TABLE[((b & 0x0f) << 2 | c >> 6) as usize] as char
            } else {
                '='
            });
            output.push(if chunk.len() > 2 {
                TABLE[(c & 0x3f) as usize] as char
            } else {
                '='
            });
        }
        output
    }

    fn receipt(event: Vec<u8>) -> Value {
        let program = id().to_string();
        json!({
            "status":"confirmed",
            "signature":"3zW2JtDLAkNNA4eQmTq5ZL9N8qBYXPSNCFNtc2M8De4wLr6LBoz8JFZgKX7fD8NP",
            "slot":42,
            "log_messages":[
                format!("Program {program} invoke [1]"),
                format!("Program data: {}", b64(&event)),
                format!("Program {program} success")
            ]
        })
    }

    #[test]
    fn exact_offer_receipt_is_required() {
        let game = Pubkey::new_unique();
        let proposer = Pubkey::new_unique();
        let event = BarterProposed {
            game,
            from: proposer,
            offer: 7,
            goods: 1,
            price: 3_000_000,
        };
        let receipt = with_alashi_logs(&receipt(event.data()));
        assert_eq!(
            verified_offer(&receipt, game, proposer, 7, 1, 3_000_000).map(|v| v.slot),
            Some(42)
        );
        assert!(verified_offer(&receipt, game, proposer, 7, 2, 3_000_000).is_none());
    }

    #[test]
    fn exact_accept_receipt_is_required() {
        let game = Pubkey::new_unique();
        let proposer = Pubkey::new_unique();
        let buyer = Pubkey::new_unique();
        let event = BarterAccepted {
            game,
            by: buyer,
            from: proposer,
            offer: 7,
        };
        let receipt = with_alashi_logs(&receipt(event.data()));
        assert!(verified_accept(&receipt, game, buyer, proposer, 7).is_some());
        assert!(verified_accept(&receipt, game, proposer, buyer, 7).is_none());
    }

    #[test]
    fn nested_non_alashi_program_data_is_not_evidence() {
        let game = Pubkey::new_unique();
        let proposer = Pubkey::new_unique();
        let event = BarterProposed {
            game,
            from: proposer,
            offer: 7,
            goods: 1,
            price: 3_000_000,
        };
        let outsider = Pubkey::new_unique();
        let receipt = json!({
            "status":"confirmed",
            "signature":"3zW2JtDLAkNNA4eQmTq5ZL9N8qBYXPSNCFNtc2M8De4wLr6LBoz8JFZgKX7fD8NP",
            "slot":42,
            "log_messages":[
                format!("Program {outsider} invoke [1]"),
                format!("Program data: {}", b64(&event.data())),
                format!("Program {outsider} success")
            ]
        });
        let filtered = with_alashi_logs(&receipt);
        assert!(verified_offer(&filtered, game, proposer, 7, 1, 3_000_000).is_none());
    }
    #[test]
    fn gameplay_receipts_require_exact_confirmed_events() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let stranger = Pubkey::new_unique();
        let mut produced = with_alashi_logs(&receipt(
            Produced {
                game,
                faction,
                goods: 4,
            }
            .data(),
        ));
        assert!(verified_produced(&produced, game, faction, 4).is_some());
        assert!(verified_produced(&produced, game, faction, 2).is_none());
        assert!(verified_produced(&produced, stranger, faction, 4).is_none());
        assert!(verified_produced(&produced, game, stranger, 4).is_none());
        produced["status"] = json!("failed");
        assert!(verified_produced(&produced, game, faction, 4).is_none());
        let sold = with_alashi_logs(&receipt(
            Sold {
                game,
                faction,
                units: 1,
                revenue: 5_000_000,
            }
            .data(),
        ));
        assert!(verified_sold(&sold, game, faction, 1, 5_000_000).is_some());
        assert!(verified_sold(&sold, game, faction, 2, 5_000_000).is_none());
        assert!(verified_sold(&sold, game, faction, 1, 6_000_000).is_none());
        assert!(verified_sold(&sold, stranger, faction, 1, 5_000_000).is_none());
        assert!(verified_sold(&sold, game, stranger, 1, 5_000_000).is_none());
    }

    #[test]
    fn nested_outsider_gameplay_events_do_not_count() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let outsider = Pubkey::new_unique();
        for event in [
            Produced {
                game,
                faction,
                goods: 2,
            }
            .data(),
            Sold {
                game,
                faction,
                units: 1,
                revenue: 5,
            }
            .data(),
        ] {
            let raw = json!({"status":"confirmed", "signature":"test", "slot":42,
                "log_messages":[format!("Program {} invoke [1]", id()), format!("Program {outsider} invoke [2]"),
                    format!("Program data: {}", b64(&event)), format!("Program {outsider} success"), format!("Program {} success", id())]});
            let scoped = with_alashi_logs(&raw);
            assert!(verified_produced(&scoped, game, faction, 2).is_none());
            assert!(verified_sold(&scoped, game, faction, 1, 5).is_none());
        }
    }
}
