use solana_secp256k1_recover::{secp256k1_recover, Secp256k1RecoverError};
fn bytes<const N: usize>(s: &str) -> [u8; N] {
    assert_eq!(s.len(), N * 2);
    core::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
const HASH: &str = "ad84cd0b10fc028738971b078124aec2a0e7c6d986a381be0b386f32bee887af";
#[test]
fn upstream_recovery_vector_0() {
    let key = secp256k1_recover(&bytes::<32>(HASH), 0, &bytes::<64>("ce53abb3721bafc561408ce8ff99c909f7f0b18a2f788649d6470162ab1aa0323971edc523a6d6453f3fb6128d318d9db1a5ff3386feb1047d9816e780039d52")).unwrap();
    let compressed =
        bytes::<33>("021a7a569e91dbf60581509c7fc946d1003b60c7dee85299538db6353538d59574");
    assert_eq!(&key.0[..32], &compressed[1..]);
    assert_eq!(key.0[63] & 1, compressed[0] & 1);
}
#[test]
fn upstream_recovery_vector_1() {
    let key = secp256k1_recover(&bytes::<32>(HASH), 1, &bytes::<64>("46c05b6368a44b8810d79859441d819b8e7cdc8bfd371e35c53196f4bcacdb5135c7facce2a97b95eacba8a586d87b7958aaf8368ab29cee481f76e871dbd9cb")).unwrap();
    let compressed =
        bytes::<33>("036d6caac248af96f6afa7f904f550253a0f3ef3f5aa2fe6838a95b216691468e2");
    assert_eq!(&key.0[..32], &compressed[1..]);
    assert_eq!(key.0[63] & 1, compressed[0] & 1);
}
#[test]
fn high_s_keeps_upstream_host_rejection() {
    assert_eq!(secp256k1_recover(&bytes::<32>(HASH),1,&bytes::<64>("ce53abb3721bafc561408ce8ff99c909f7f0b18a2f788649d6470162ab1aa032c68e123adc5929bac0c049ed72ce72610908ddb32849ef37423a47a55032a3ef")).map(|key| key.0), Err(Secp256k1RecoverError::InvalidSignature));
}
#[test]
fn invalid_inputs_keep_error_codes() {
    let hash = bytes::<32>(HASH);
    let sig=bytes::<64>("ce53abb3721bafc561408ce8ff99c909f7f0b18a2f788649d6470162ab1aa0323971edc523a6d6453f3fb6128d318d9db1a5ff3386feb1047d9816e780039d52");
    assert_eq!(
        secp256k1_recover(&hash[..31], 0, &sig).map(|key| key.0),
        Err(Secp256k1RecoverError::InvalidHash)
    );
    for id in [4, 27, 255] {
        assert_eq!(
            secp256k1_recover(&hash, id, &sig).map(|key| key.0),
            Err(Secp256k1RecoverError::InvalidRecoveryId)
        );
    }
    for bad in [&sig[..63], &[0u8; 64][..], &[255u8; 64][..]] {
        assert_eq!(
            secp256k1_recover(&hash, 0, bad).map(|key| key.0),
            Err(Secp256k1RecoverError::InvalidSignature)
        );
    }
    for id in [2, 3] {
        assert_eq!(
            secp256k1_recover(&hash, id, &sig).map(|key| key.0),
            Err(Secp256k1RecoverError::InvalidSignature)
        );
    }
    for (e, c) in [
        (Secp256k1RecoverError::InvalidHash, 1),
        (Secp256k1RecoverError::InvalidRecoveryId, 2),
        (Secp256k1RecoverError::InvalidSignature, 3),
    ] {
        assert_eq!(u64::from(e.clone()), c);
        assert_eq!(Secp256k1RecoverError::from(c), e);
    }
}
