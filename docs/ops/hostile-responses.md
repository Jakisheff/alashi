# Pre-drafted responses to expected hostile comments

Written before any public posting, per the session-audit protocol.
Each response is short enough to post as-is, factual, and does not
engage with tone.

## "Self-verified is not verified. You wrote the verifier too."

The verifier was attacked by its own mutation test: ten controlled
corruptions of the data, including one that regenerates file hashes.
All ten are caught; the hash-regenerating attack is caught by the
logic layer alone. An independent Python reimplementation, reading
the raw journaled calls and bypassing the export pipeline entirely,
reproduces the three headline statistics bit-exactly. The verifier
and mutation journal ship in the repository; anyone can add mutations
and check. CI runs the verifier on every push to a public repository:
https://github.com/Jakisheff/world-100-grid/actions

## "Solo author, no institution, no co-authors. Why should I trust this?"

Every claim has a machine-checkable artifact behind it. You do not
need to trust the author; you need to trust that `python3 verify.py`
exits 0, which you can check yourself in under a minute. The
preregistrations were committed before the data (git timestamps
prove this). The weakest link (hand-designed attacker policy instead
of exhaustive search) is named in the paper itself, and the exhaustive
enumeration that replaced it is what found the HHI breach.

## "MIT license means there is nothing to sell or license."

Correct, and that is intentional. The engine is MIT for adoption; the
value is in the recorded run corpora (50 games of journaled prompts,
menus, decisions and verified replays), which are available as a
dataset under a separate data license. This is the standard
open-engine / closed-corpus split.

## "A custom political-economy engine is not generalizable."

The findings are not about the specific game; they are about the
protocol (bounded action menus), the measurement (concentration
metrics under volume variation), and the methodology (preregistered
adversary coverage with machine-verified claims). The compliance
cliff was tested across four model families from three organizations;
two failed. The volume-confound is a general statistical property of
behavioral metrics, not specific to bribery.

## "100 agents is impractical for evaluation."

The engine is parameterized: `--agents N` works for N from 2 to 100.
The 100-agent configuration is the stress test from the preregistered
experiments, not a requirement. For a demonstration or Kaggle
integration, 8-10 agents suffice to exercise all mechanics (markets,
bribes, law voting, veto).
