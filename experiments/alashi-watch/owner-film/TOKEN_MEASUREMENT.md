# Measured Degenie token usage

Date: 2026-10-16. Status: observed for provider usage; not_validated for savings.
The owner stopped further model requests after reviewing the first experiment.
No second live experiment was run. This report does not change the video.

## Result

33 requests were sent to `glm-4.5-flash`. 31 returned usage receipts:

| Provider field | Observed tokens |
|---|---:|
| Input | 9,908 |
| Output | 570 |
| Total | 10,478 |
| Cached input, already included in input | 5,725 |

Two requests returned no usage: one concurrency rejection and one transport
timeout. Their token use is unknown. 10,478 is the confirmed total, not a
complete total for all attempts. Cached tokens must not be added again.
The historical 28-token access check is outside this experiment.

GLM-4.5-Flash input and output are free in the provider's
[published tariff](https://docs.z.ai/guides/overview/pricing), checked 2026-10-16.
The tariff-based model cost is $0. This is not a billing-statement reconciliation.
There is no monetary saving to claim on this free model.

## Recorded workflows

| Fixture | Full context, known tokens | Shared scout, known tokens | Comparison |
|---|---:|---:|---|
| market-1 | 1,802 plus unknown rejected request | 1,427 | Invalid |
| market-2 | 2,637 | 549 plus unknown timed-out request | Invalid |
| market-3 | 2,634 | 1,429 | Invalid |

All pairs failed the predeclared quality gate. Lower observed use is not evidence
of efficiency: after a scout parsing failure, workers received null instead of
the snapshot. Those requests were smaller and performed the wrong work.

In market-3, the full workflow used 2,574 input and 60 output tokens across five
calls. Its final sale matched the intended action, but its market specialist
reported 50,000,000 revenue instead of the canonical 13,600,000. The gate
requires correct specialist reports as well as the final action. It was not
weakened after seeing the result.

The shared workflow used six calls, including an 880-token scout. The scout's
data was correct but wrapped in a Markdown code block. The experiment's strict
JSON parser rejected that wrapper, and the downstream workers received null.
This was an experiment implementation defect, not proof that shared scouting
fails as a strategy. The flawed downstream calls remain in the expense ledger.

## Corrections and remaining limits

`shared_scout.py` now accepts strict JSON or a single JSON code block. It stops
dependent requests when their input is unavailable or invalid. The market
instruction now specifies price indexing, conversion to cash units and tax
arithmetic identically in both versions. These corrections were checked locally;
their effects on live token use and task quality are not measured.

The original and resumed measured source files are retained under
`evidence/shared-scout-live/source/`. Their hashes match the corresponding
manifests. The current corrected script is not the script that produced those
receipts. Oracle-filled request plans are explicitly marked as plans and do
not contain fabricated provider usage.

These are frozen final-market fixtures, not live matches. After an initial
concurrency rejection, remaining requests were sequential. No latency or
parallel execution advantage is established. Strategy development, this Codex
conversation, adaptation by another owner and independent owner reproduction
are outside the measured expense ledger. Previous illustrative figures of
12,000 / 8,000 per run, 4,000 for authoring and 1,000 for adoption are withdrawn
as estimates of real costs.

For the product, crystal spending can be linked to the sum of provider
`total_tokens` across every attempted call with known usage. Failed calls count;
missing usage stays unknown. A usable strategy comparison additionally requires
the quality gate. This accounting rule has not been integrated into the game UI.

## Evidence and reproduction

- [Provider requests and responses](evidence/shared-scout-live/requests.jsonl)
- [Independent audit](evidence/shared-scout-live/audit.json)
- [Initial manifest](evidence/shared-scout-live/manifest.json)
- [Resume manifest](evidence/shared-scout-live/resume-manifest.json)
- [Canonical fixtures](evidence/shared-scout-live/cases.json)
- [Corrected experiment](shared_scout.py)
- [Offline receipt auditor](audit_shared_scout.py)

The initial three requests were read from saved receipts when execution resumed.
They were not sent again. The combined ledger contains exactly 33 unique
workflow/role pairs. There were no automatic retries or model substitutions.

Offline verification, with no model requests:

```sh
cargo build --locked -p alashi-rules --example owner_film_oracle
python3 experiments/alashi-watch/owner-film/audit_shared_scout.py experiments/alashi-watch/owner-film/evidence/shared-scout-live
cargo test --locked -p alashi-rules
```

Validation: all 40 rules tests passed. The offline auditor independently
recalculated usage and checked responses against the canonical Rust fixtures.
The parsing regression was reproduced and corrected using both saved scout
responses. No new live call was made to validate the correction.

Two offline regression tests passed: saved fenced JSON responses parse correctly,
and a failed dependency produces no downstream model request. Run with:

```sh
python3 -m unittest discover -s experiments/alashi-watch/owner-film -p "test_shared_scout.py"
```
