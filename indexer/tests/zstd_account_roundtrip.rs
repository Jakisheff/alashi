use solana_account::{Account, AccountSharedData};
use solana_account_decoder::{encode_ui_account, UiAccountData, UiAccountEncoding};
use solana_pubkey::Pubkey;

#[test]
fn compressed_rpc_account_round_trips_without_changing_data() {
    let data: Vec<u8> = (0..4096).map(|n| (n % 251) as u8).collect();
    let account = Account {
        data: data.clone(),
        ..Account::default()
    };
    let encoded = encode_ui_account(
        &Pubkey::default(),
        &AccountSharedData::from(account),
        UiAccountEncoding::Base64Zstd,
        None,
        None,
    );
    assert!(matches!(
        encoded.data,
        UiAccountData::Binary(_, UiAccountEncoding::Base64Zstd)
    ));
    let decoded = encoded
        .decode::<Account>()
        .expect("compressed account must decode");
    assert_eq!(decoded.data, data);
}
