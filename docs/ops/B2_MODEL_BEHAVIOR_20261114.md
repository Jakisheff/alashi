# B2: model behavior in political-economy games, 14 November 2026

Partial result from bet B2 of the portfolio
([20261114-license-hire-bets](../../200_ZETTEL/20261114-license-hire-bets.md)):
two model families (Google Gemma-2-9B, Microsoft Phi-3.5) were driven
through the world-100 game via the real world-run driver against
Ollama on an NVIDIA L40S. Infrastructure instability (SSH-tunnel
timeouts to the remote GPU, model-loading contention) stopped both games
short of full completion; the Gemma-2 partial run (1,064 calls, 957
accepted actions, 4.6 percent skip rate, reached step 13 of 18) is
reported here as a behavioral snapshot. Phi-3.5 could not sustain a
connection and produced no accepted actions.

## Gemma-2-9B strategic profile (partial game, step 13/18)

| Action | Count | Share |
|---|---:|---:|
| produce | 304 | 32.4% |
| sell | 280 | 29.8% |
| vote_yes | 149 | 15.9% |
| pass | 149 | 15.9% |
| vote_abstain | 109 | 11.6% |
| produce_goods (hallucinated) | 45 | 4.8% |
| bribe | 12 | 1.3% |
| veto | 3 | 0.3% |
| buy | 2 | 0.2% |

The model is an **economic producer-trader**: 62 percent of actions are
production and selling, with near-total political abstention (1
accepted bribe in the entire partial run, 3 vetoes). This is a
fundamentally different strategic profile from the scripted arms and
the recorded qwen games (72 accepted bribes), and it is invisible to
fixed-rule games: there is no bribery, law-voting or veto dimension in
chess, Go or Werewolf.

The same hallucinated action (`produce_goods`) that broke Llama appears
in 45 of 1,064 Gemma-2 answers (4.2 percent), but the model recovers
and continues playing — it does not collapse the way Llama did (0
accepted actions).

## Phi-3.5

Phi-3.5 passed the compliance smoke (both menus, 3/3 stable) but the
game could not sustain a connection to the remote GPU endpoint. Six
calls were made, all timed out. This is recorded as an infrastructure
failure, not a model failure.

## The rank-correlation question

The original bet asked whether Alashi rankings of models correlate
poorly with Game Arena rankings. With two data points (one partial
game, one no-game) this cannot be tested. The partial result does show
that the game surfaces a strategic-behavior dimension (bribery appetite,
law participation, economic vs political orientation) that has no
counterpart in fixed-rule games. A full multi-model study remains open.

## Infrastructure note

Running full 100-agent games (1,806 calls, 30-60 minutes) through an
SSH-tunnelled Ollama endpoint on a remote GPU is fragile: model-loading
contention between concurrent games, tunnel dropouts and 60-second call
timeouts compound. A self-contained runner on the GPU host (Linux
world_worker binary plus a local driver) would remove the tunnel
dependency; that is the concrete engineering fix for the follow-up.

Artifacts: [gemma2 summary](b2-model-behavior-20261114/summary.json).
B3 (Kaggle issue) is live at
https://github.com/Kaggle/kaggle-environments/issues/1451.
