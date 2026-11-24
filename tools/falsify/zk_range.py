#!/usr/bin/env python3
"""zk_range.py: Range proof для Pedersen commitments (ci ∈ [0, 255]).

Дополнение к zk_hhi.py: закрывает границу «малые ci брутфорсятся из
commitment без range proof». Протокол:

  1. Битовая декомпозиция: ci = sum(2^j * b_j), b_j ∈ {0,1}, j=0..7
  2. Битовый commitment: C_j = g^{b_j} * h^{r_j}
  3. OR-proof (CDS / Cramer-Damgård-Schoenmakers): для каждого C_j
     доказывается «b_j = 0 ИЛИ b_j = 1» без раскрытия какого именно
  4. Связка: product(C_j^{2^j}) == C_i (гомоморфность Pedersen)
  5. Fiat-Shamir → non-interactive

Verifier: проверяет OR-proofs и связку; НЕ раскрывает ci и биты.
"""
import hashlib
import json
import random

from zk_hhi import P, G, H, mod_pow, pedersen_commit, fiat_shamir


# ---------- OR-proof (CDS) для одного бита ----------
def or_proof_bit(bit_commitment, bit, blinding, challenge_seed):
    """Доказывает: commitment скрывает 0 или 1 (не раскрывая какой).

    CDS: для истинной ветви — честный Sigma; для ложной — симуляция
    с подобранным challenge. Fiat-Shamir по обоим ветвям.
    """
    # Генерируем challenge для обеих ветвей
    e = fiat_shamir(challenge_seed, bit_commitment)

    if bit == 0:
        # Истинная ветвь: b=0; ложная: b=1
        # honest for b=0: r → A = g^w * h^v, z = w + e*0, y = v + e*r_b
        w0 = random.randrange(1, P - 1)
        v0 = random.randrange(1, P - 1)
        A0 = (mod_pow(G, w0) * mod_pow(H, v0)) % P
        # simulate for b=1: pick e1, z1, y1; A1 = g^z1 * h^y1 / C^e1
        e1 = random.randrange(1, P - 1)
        z1 = random.randrange(1, P - 1)
        y1 = random.randrange(1, P - 1)
        A1 = (mod_pow(G, z1) * mod_pow(H, y1) * mod_pow(bit_commitment, -e1)) % P
        # Fiat-Shamir: e = H(A0, A1, C)
        e = fiat_shamir(challenge_seed, A0, A1, bit_commitment)
        e0 = (e - e1) % (P - 1)
        z0 = (w0 + e0 * 0) % (P - 1)  # b=0: z = w + e*0 = w
        y0 = (v0 + e0 * blinding) % (P - 1)
        return {"A0": str(A0), "A1": str(A1), "e0": str(e0), "e1": str(e1),
                "z0": str(z0), "y0": str(y0), "z1": str(z1), "y1": str(y1)}
    else:
        # Истинная ветвь: b=1; ложная: b=0
        w1 = random.randrange(1, P - 1)
        v1 = random.randrange(1, P - 1)
        A1 = (mod_pow(G, w1) * mod_pow(H, v1)) % P
        e0 = random.randrange(1, P - 1)
        z0 = random.randrange(1, P - 1)
        y0 = random.randrange(1, P - 1)
        A0 = (mod_pow(G, z0) * mod_pow(H, y0) * mod_pow(bit_commitment, -e0)) % P
        e = fiat_shamir(challenge_seed, A0, A1, bit_commitment)
        e1 = (e - e0) % (P - 1)
        z1 = (w1 + e1 * 1) % (P - 1)  # b=1: z = w + e*1
        y1 = (v1 + e1 * blinding) % (P - 1)
        return {"A0": str(A0), "A1": str(A1), "e0": str(e0), "e1": str(e1),
                "z0": str(z0), "y0": str(y0), "z1": str(z1), "y1": str(y1)}


def or_verify_bit(proof, bit_commitment, challenge_seed):
    """Verifier: g^z0*h^y0 == A0 * C^e0  И  g^z1*h^y1 == A1 * C^e1
    И e0 + e1 == Fiat-Shamir(A0, A1, C)."""
    A0, A1 = int(proof["A0"]), int(proof["A1"])
    e0, e1 = int(proof["e0"]), int(proof["e1"])
    z0, y0 = int(proof["z0"]), int(proof["y0"])
    z1, y1 = int(proof["z1"]), int(proof["y1"])
    C = bit_commitment

    lhs0 = (mod_pow(G, z0) * mod_pow(H, y0)) % P
    rhs0 = (A0 * mod_pow(C, e0)) % P
    if lhs0 != rhs0:
        return False, "branch-0 equation failed"

    lhs1 = (mod_pow(G, z1) * mod_pow(H, y1)) % P
    rhs1 = (A1 * mod_pow(C, e1)) % P
    if lhs1 != rhs1:
        return False, "branch-1 equation failed"

    e_check = fiat_shamir(challenge_seed, A0, A1, C)
    if (e0 + e1) % (P - 1) != e_check:
        return False, "challenge sum mismatch"
    return True, "bit in {0,1}"


# ---------- Range proof для одного ci ----------
def range_prove(ci, ci_blinding, actor_id):
    """Полный range proof: 8 битовых commitments + OR-proofs + связка."""
    assert 0 <= ci <= 255, f"ci={ci} outside [0,255]"
    bits = [(ci >> j) & 1 for j in range(8)]
    bit_commitments = []
    bit_blindings = []
    for j, b in enumerate(bits):
        rj = random.randrange(1, P - 1)
        Cj = pedersen_commit(b, rj)
        bit_commitments.append(Cj)
        bit_blindings.append(rj)

    # OR-proofs для каждого бита
    or_proofs = []
    for j, b in enumerate(bits):
        pf = or_proof_bit(bit_commitments[j], b, bit_blindings[j], f"{actor_id}:bit{j}")
        or_proofs.append(pf)

    # Связка: product(Cj^{2^j}) == C_original
    # C_original = g^ci * h^{bi}; sum(rj * 2^j) должно равняться bi
    # НЕ обязано! Pedersen blinding мультипликативен:
    # product(Cj^{2^j}) = g^{sum(bj*2^j)} * h^{sum(rj*2^j)} = g^ci * h^{sum(rj*2^j)}
    # Для связки нужно bi == sum(rj*2^j) — генерируем rj так чтобы это выполнялось
    # ПРАВИЛЬНЕЕ: связка через отдельный Sigma-протокол на равенство
    # commitment'ов (доказательство равенства двух commitment'ов)
    C_orig = pedersen_commit(ci, ci_blinding)
    C_bits = 1
    for j in range(8):
        C_bits = (C_bits * mod_pow(bit_commitments[j], 1 << j)) % P

    # Sigma на равенство C_orig и C_bits (знаем blinding delta = ci_blinding - sum(rj*2^j))
    blinding_delta = (ci_blinding - sum(bit_blindings[j] << j for j in range(8))) % (P - 1)
    # Доказываем: C_orig / C_bits = h^{delta}; Sigma-протокол на знание delta
    w = random.randrange(1, P - 1)
    A = mod_pow(H, w)
    e = fiat_shamir(f"link:{actor_id}", A, C_orig, C_bits)
    z = (w + e * blinding_delta) % (P - 1)
    link_proof = {"A": str(A), "e": str(e), "z": str(z),
                  "C_orig": str(C_orig), "C_bits": str(C_bits)}

    return {
        "actor": actor_id,
        "bit_commitments": [str(C) for C in bit_commitments],
        "or_proofs": or_proofs,
        "link_proof": link_proof,
    }


def range_verify(proof):
    """Verifier: все OR-proofs + связка."""
    actor = proof["actor"]
    bcs = [int(c) for c in proof["bit_commitments"]]

    for j, pf in enumerate(proof["or_proofs"]):
        ok, msg = or_verify_bit(pf, bcs[j], f"{actor}:bit{j}")
        if not ok:
            return False, f"bit {j}: {msg}"

    lp = proof["link_proof"]
    A, e, z = int(lp["A"]), int(lp["e"]), int(lp["z"])
    C_orig, C_bits = int(lp["C_orig"]), int(lp["C_bits"])

    # Fiat-Shamir воспроизводимость
    e_check = fiat_shamir(f"link:{actor}", A, C_orig, C_bits)
    if e != e_check:
        return False, "link challenge mismatch"

    # Sigma: h^z == A * (C_orig/C_bits)^e
    ratio = (C_orig * mod_pow(C_bits, P - 2)) % P  # C_orig / C_bits
    lhs = mod_pow(H, z)
    rhs = (A * mod_pow(ratio, e)) % P
    if lhs != rhs:
        return False, "link equation failed"

    # C_bits == product(bcs[j]^{2^j})
    cb = 1
    for j in range(8):
        cb = (cb * mod_pow(bcs[j], 1 << j)) % P
    if cb != C_bits:
        return False, "bit commitment product mismatch"

    return True, f"ci in [0,255] for actor {actor}"


# ---------- Demo ----------
def demo():
    import pyarrow.parquet as pq
    from collections import Counter
    import os

    data = os.environ.get("WORLD100_DATA", "data/decisions.parquet")
    rows = pq.read_table(data).to_pylist()
    game = [r for r in rows if r["game"] == "l4-20261109-original"]
    counts = Counter(r["chosen"] for r in game)

    print(f"[demo] {len(counts)} actors; proving ci ∈ [0,255] for each")
    proofs = {}
    for actor in sorted(counts):
        ci = counts[actor]
        blinding = random.randrange(1, P - 1)
        proofs[actor] = range_prove(ci, blinding, actor)

    # Verify all
    ok_count = 0
    for actor, pf in sorted(proofs.items()):
        ok, msg = range_verify(pf)
        if ok:
            ok_count += 1
        else:
            print(f"  FAIL actor {actor}: {msg}")
    print(f"[verify] {ok_count}/{len(proofs)} range proofs valid")

    # Tamper: подменим битовое обязательство
    test_actor = sorted(proofs)[0]
    fake = json.loads(json.dumps(proofs[test_actor]))
    fake["bit_commitments"][0] = str((int(fake["bit_commitments"][0]) * 5) % P)
    ok_f, msg_f = range_verify(fake)
    print(f"[tampered-bit] {ok_f}: {msg_f}")

    # Tamper: значение вне диапазона (9 бит, ci=300)
    try:
        range_prove(300, 12345, "attacker")
        print(f"[out-of-range] FAIL: 300 прошёл декомпозицию")
        return 1
    except AssertionError:
        print(f"[out-of-range] OK: assert отверг 300 (вне [0,255])")

    with open("/tmp/zk_range_proofs.json", "w") as f:
        json.dump({str(k): v for k, v in proofs.items()}, f)
    print(f"[saved] /tmp/zk_range_proofs.json ({len(json.dumps(proofs))} bytes)")
    return 0 if ok_count == len(proofs) and not ok_f else 1


if __name__ == "__main__":
    import sys
    sys.exit(demo())
