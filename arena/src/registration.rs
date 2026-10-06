//! One-time devnet identity receipt. Gameplay and balances remain offchain.
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const MEMO_PROGRAM_ID: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
pub const DEVNET_GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const RPC_URL: &str = "https://api.devnet.solana.com";
const MAX_RPC_BYTES: u64 = 262_144;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub wallet: String,
    pub signature: String,
}

impl Proof {
    pub fn parse(value: &Value) -> Result<Self, &'static str> {
        let proof: Self =
            serde_json::from_value(value.clone()).map_err(|_| "invalid_registration")?;
        validate_wallet(&proof.wallet)?;
        // RPC resolves and verifies the signature; bound and validate its base58 spelling first.
        if !(64..=88).contains(&proof.signature.len())
            || !proof
                .signature
                .bytes()
                .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
        {
            return Err("invalid_registration_signature");
        }
        Ok(proof)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub mode: String,
    pub network: String,
    pub wallet: String,
    pub signature: String,
    pub slot: u64,
    pub fee_lamports: String,
    pub commitment: String,
}

pub fn validate_wallet(wallet: &str) -> Result<(), &'static str> {
    wallet
        .parse::<Pubkey>()
        .map(|_| ())
        .map_err(|_| "invalid_registration_wallet")
}

pub fn memo(game_id: u64, party_no: u64, character_id: &str, agent_id: &str) -> String {
    format!("alashi:agent-start:v1:devnet:{game_id}:{party_no}:{character_id}:{agent_id}")
}

pub fn lifecycle_memo(owner_id: &str, agent_record_id: &str, character_id: &str, challenge: &str) -> String {
    format!("alashi:agent-lifecycle:v2:devnet:alashi.network:{owner_id}:{agent_record_id}:{character_id}:{challenge}")
}

pub fn validate_genesis(value: &Value) -> Result<(), &'static str> {
    if value.as_str() == Some(DEVNET_GENESIS) {
        Ok(())
    } else {
        Err("not_solana_devnet")
    }
}

/// Only a confirmed/finalized successful transaction with an exact top-level
/// Memo instruction and an actual transaction signer may register a session.
pub fn validate_receipt(
    proof: &Proof,
    expected_memo: &str,
    status: &Value,
    tx: &Value,
) -> Result<Receipt, &'static str> {
    let status = status["value"]
        .as_array()
        .and_then(|v| if v.len() == 1 { v.first() } else { None })
        .filter(|v| v.is_object())
        .ok_or("registration_not_confirmed")?;
    if !matches!(
        status["confirmationStatus"].as_str(),
        Some("confirmed" | "finalized")
    ) {
        return Err("registration_not_confirmed");
    }
    if status.get("err") != Some(&Value::Null) {
        return Err("registration_transaction_failed");
    }
    if !tx.is_object() {
        return Err("registration_transaction_unavailable");
    }
    let meta = tx
        .get("meta")
        .filter(|v| v.is_object())
        .ok_or("registration_transaction_unavailable")?;
    if meta.get("err") != Some(&Value::Null) {
        return Err("registration_transaction_failed");
    }
    let slot = tx["slot"]
        .as_u64()
        .ok_or("invalid_registration_transaction")?;
    if status["slot"].as_u64() != Some(slot) {
        return Err("registration_slot_mismatch");
    }
    if tx["transaction"]["signatures"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        != Some(&proof.signature)
    {
        return Err("registration_signature_mismatch");
    }
    let message = &tx["transaction"]["message"];
    let signed = message["accountKeys"].as_array().is_some_and(|keys| {
        keys.iter()
            .any(|key| key["pubkey"].as_str() == Some(&proof.wallet) && key["signer"] == true)
    });
    if !signed {
        return Err("registration_wallet_not_signer");
    }
    let exact_memo = message["instructions"].as_array().is_some_and(|ixs| {
        ixs.iter().any(|ix| {
            ix["programId"].as_str() == Some(MEMO_PROGRAM_ID)
                && ix["parsed"].as_str() == Some(expected_memo)
        })
    });
    if !exact_memo {
        return Err("registration_memo_mismatch");
    }
    let fee = meta["fee"].as_u64().ok_or("invalid_registration_fee")?;
    Ok(Receipt {
        mode: "agent_start_v1".into(),
        network: "devnet".into(),
        wallet: proof.wallet.clone(),
        signature: proof.signature.clone(),
        slot,
        fee_lamports: fee.to_string(),
        commitment: "confirmed".into(),
    })
}

/// Fixed HTTPS endpoint, no redirects, retries, curl config, or caller-supplied URL.
/// One deadline covers all three read-only calls. Both curl and the pipe reader
/// bound the response; detailed transport output is never exposed to clients.
fn rpc(method: &str, params: Value, deadline: Instant) -> Result<Value, &'static str> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("registration_rpc_timeout")?;
    if remaining < Duration::from_millis(1) {
        return Err("registration_rpc_timeout");
    }
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--fail",
            "--proto",
            "=https",
            "--connect-timeout",
            "3",
            "--max-time",
            &format!("{:.3}", remaining.as_secs_f64()),
            "--max-filesize",
            &MAX_RPC_BYTES.to_string(),
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            &body,
            RPC_URL,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "registration_rpc_unavailable")?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("registration_rpc_unavailable")?
        .take(MAX_RPC_BYTES + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_RPC_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err("registration_rpc_response_too_large");
    }
    let code = child.wait().map_err(|_| "registration_rpc_unavailable")?;
    if !code.success() {
        return Err(if Instant::now() >= deadline {
            "registration_rpc_timeout"
        } else {
            "registration_rpc_unavailable"
        });
    }
    if Instant::now() >= deadline {
        return Err("registration_rpc_timeout");
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "registration_rpc_invalid_json")?;
    if value["jsonrpc"] != "2.0" || value["id"] != 1 || value.get("error").is_some() {
        return Err("registration_rpc_failed");
    }
    value
        .get("result")
        .cloned()
        .ok_or("registration_rpc_invalid_json")
}

pub fn verify(proof: &Proof, expected_memo: &str) -> Result<Receipt, &'static str> {
    let deadline = Instant::now() + Duration::from_secs(10);
    validate_genesis(&rpc("getGenesisHash", json!([]), deadline)?)?;
    let status = rpc(
        "getSignatureStatuses",
        json!([[proof.signature], {"searchTransactionHistory":true}]),
        deadline,
    )?;
    let tx = rpc(
        "getTransaction",
        json!([proof.signature, {
            "encoding":"jsonParsed", "commitment":"confirmed", "maxSupportedTransactionVersion":0
        }]),
        deadline,
    )?;
    validate_receipt(proof, expected_memo, &status, &tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Proof, String, Value, Value) {
        let proof = Proof {
            wallet: Pubkey::new_unique().to_string(),
            signature: "2".repeat(88),
        };
        let memo = memo(7, 81, &"c".repeat(64), &"a".repeat(64));
        let status = json!({"value":[{"slot":99,"err":null,"confirmationStatus":"confirmed"}]});
        let tx = json!({"slot":99,"meta":{"err":null,"fee":5000},
        "transaction":{"signatures":[proof.signature],"message":{
            "accountKeys":[{"pubkey":proof.wallet,"signer":true}],
            "instructions":[{"programId":MEMO_PROGRAM_ID,"parsed":memo}]
        }}});
        (proof, memo, status, tx)
    }
    #[test]
    fn confirmed_receipt_has_exact_units_and_network() {
        let (proof, memo, status, mut tx) = fixture();
        tx["meta"]["fee"] = json!(u64::MAX);
        let receipt = validate_receipt(&proof, &memo, &status, &tx).unwrap();
        assert_eq!(receipt.fee_lamports, u64::MAX.to_string());
        assert_eq!(receipt.network, "devnet");
        assert!(validate_genesis(&json!(DEVNET_GENESIS)).is_ok());
        assert_eq!(
            validate_genesis(&json!("mainnet")),
            Err("not_solana_devnet")
        );
    }
    #[test]
    fn status_failed_missing_unconfirmed_and_slot_mismatch_fail_closed() {
        let (proof, memo, status, tx) = fixture();
        for status in [
            json!({"value":[null]}),
            json!({"value":[{"slot":99,"err":null,"confirmationStatus":"processed"}]}),
            json!({"value":[{"slot":99,"err":"failed","confirmationStatus":"confirmed"}]}),
            json!({"value":[]}),
        ] {
            assert!(validate_receipt(&proof, &memo, &status, &tx).is_err());
        }
        for path in ["/meta/err", "/meta", "/transaction/signatures/0", "/slot"] {
            let mut bad = tx.clone();
            *bad.pointer_mut(path).unwrap() = json!("wrong");
            assert!(
                validate_receipt(&proof, &memo, &status, &bad).is_err(),
                "{path}"
            );
        }
        assert!(validate_receipt(&proof, &memo, &status, &Value::Null).is_err());
    }
    #[test]
    fn signer_and_exact_top_level_memo_are_required() {
        let (proof, memo, status, tx) = fixture();
        for path in [
            "/transaction/message/accountKeys/0/signer",
            "/transaction/message/accountKeys/0/pubkey",
            "/transaction/message/instructions/0/programId",
            "/transaction/message/instructions/0/parsed",
        ] {
            let mut bad = tx.clone();
            *bad.pointer_mut(path).unwrap() = json!("spoof");
            assert!(
                validate_receipt(&proof, &memo, &status, &bad).is_err(),
                "{path}"
            );
        }
        let mut inner = tx.clone();
        inner["meta"]["innerInstructions"] =
            json!([{"instructions":tx["transaction"]["message"]["instructions"]}]);
        inner["transaction"]["message"]["instructions"] = json!([]);
        assert!(validate_receipt(&proof, &memo, &status, &inner).is_err());
        assert!(validate_receipt(&proof, &format!("{memo}:extra"), &status, &tx).is_err());
    }
    #[test]
    fn lifecycle_memo_has_stable_global_scope() {
        let owner="d8d041d59e9d55c61790d37a8e2bc3f17b9c8f4d350062a090ea8b5d64a086fa";
        let agent="cd".repeat(32);
        let character="6d92bd091fb2d69e295fe5bba10caa3628abf2cac55bc80f7c74a018c4465c71";
        let challenge="ef".repeat(32);
        assert_eq!(lifecycle_memo(owner,&agent,character,&challenge),
            format!("alashi:agent-lifecycle:v2:devnet:alashi.network:{owner}:{agent}:{character}:{challenge}"));
        assert!(!lifecycle_memo(owner,&agent,character,&challenge).contains(":42:"));
    }
    #[test]
    fn request_is_bounded_and_memo_scoped() {
        let (proof, _, _, _) = fixture();
        assert!(Proof::parse(&json!({"wallet":proof.wallet,"signature":proof.signature})).is_ok());
        for value in [
            json!({"wallet":"bad","signature":"2".repeat(88)}),
            json!({"wallet":proof.wallet,"signature":"0".repeat(88)}),
            json!({"wallet":proof.wallet,"signature":"2".repeat(1000)}),
            json!({"wallet":proof.wallet,"signature":proof.signature,"ok":true}),
        ] {
            assert!(Proof::parse(&value).is_err());
        }
        assert_ne!(memo(1, 2, "c", "a"), memo(1, 3, "c", "a"));
        assert_ne!(memo(1, 2, "c", "a"), memo(2, 2, "c", "a"));
        assert_ne!(memo(1, 2, "c", "a"), memo(1, 2, "other", "a"));
    }
}
