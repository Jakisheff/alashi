# BitLLM integration and first live local experiment

Observed on 9 November 2026. BitLLM can execute the ALASHI local comparison
without an NVIDIA instance. The final configuration completed both variants;
the candidate's wording still failed a manual factual review. No owner choice
or strategy adoption has been recorded.

## What changed

The existing provider adapter accepts an optional `chat_template_kwargs`
object with exactly one boolean key, `enable_thinking` or `thinking`. Local
experiments include it in their frozen input and check the executing provider
against it. Old manifests omit the object and keep their previous requests.
The cloud comparison path rejects an option absent from its frozen profile.
The output limit remains 256 tokens and the temperature remains 0.

This addition resolves an observed compatibility problem: both advertised
models consumed the short output budget on reasoning. A prompt-only
`/no_think` hint was insufficient. BitLLM v0.5.0 explicitly permits the request
object, and its reference Qwen configuration uses `enable_thinking: false`.
See the [request boundary](https://github.com/bitllm-org/bitllm/blob/v0.5.0/internal/node/boundary_request.go)
and [model registry](https://github.com/bitllm-org/bitllm/blob/v0.5.0/internal/models/models.json).

The [runbook](../../tools/agent-bootstrap/BITLLM.md) and example manifest make
the connection reproducible. The local guide now correctly explains that a
loopback forwarding gateway can send inputs to remote peers.

## Question and fixed conditions

The synthetic fixture says Acme offers a weekly workshop for teams. Both
variants use the same brief, model profile, local fact-extraction tool, shared
deadline and token limits. Only one model instruction differs: the candidate
adds `Begin the headline with "Weekly workshop".`

The predeclared criterion requires complete usage, valid output and no
invented claim. The prefix observation alone does not establish better copy,
advertising lift or owner preference. This is an integration experiment; a
real customer task remains a separate product test.

## All attempts, including failures

Each row is a separately frozen pair, not a retry that overwrites prior data.
Attempts 2–4 are compatibility exploration informed by earlier failures.

| Attempt | Configuration | Completed variants | Reported total tokens | Outcome |
| --- | --- | ---: | ---: | --- |
| 1 | Qwen, original profile | 0/2 | 1,100 | Both responses hit `finish_reason: length`; truncated JSON |
| 2 | DeepSeek, original profile | 0/2 | 1,084 | Both responses spent all 256 completion tokens on reasoning |
| 3 | Qwen, shared `/no_think` hint | 1/2 | 1,098 | Baseline truncated; candidate completed; pair inconclusive |
| 4 | Qwen, explicit `enable_thinking: false` | 2/2 | 597 | Complete JSON and usage; ready for human review |

There were **8 model calls and 3,879 reported tokens** in total. No additional
request occurred when resuming the completed fourth attempt. Provider monetary
cost remains `null`; no dollar value for BitLLM credits was assumed.
**NVIDIA expenditure was $0**, with no instance started. No ALASHI HTTP calls,
wallet linkage or blockchain transactions were made.

## Final pair

Requested model: `qwen3.8-flash-next-fp4-alesha`.
Returned model field: `qwen3.8-flash-next-fp4-local`.
Both responses identified the same serving peer. The differing names are
recorded as observations, not proof that the weights match either name.

| Variant | Measured request duration | Reported input / output tokens | Output checks |
| --- | ---: | ---: | --- |
| Baseline | 1,111 ms | 254 / 41 | All passed |
| Candidate | 1,944 ms | 262 / 40 | All passed |

Requests were concurrent and may have queued at the peer; two durations do not
establish a latency percentile or performance advantage.

Baseline:

```json
{"headline":"Acme Weekly Workshop for Teams","body":"Acme offers a weekly workshop for teams. This is a synthetic integration test, not a real commercial offer.","cta":"Explore Acme"}
```

Candidate:

```json
{"headline":"Weekly workshop by Acme for teams comparing product ideas","body":"Acme offers a weekly workshop for teams. Join us to compare product ideas effectively.","cta":"Explore Acme"}
```

The candidate alone follows the requested prefix. However, “effectively” adds
an unsupported effectiveness claim. Mechanical checks cover schema, ASCII,
word limits, required terms and CTA; they do not verify factual support. Under
the frozen failure criterion, the substantive comparison is **inconclusive**.
The runner status is `ready_for_owner_decision`; `decision` remains `null`.
No `decide`, `apply` or Public Goods publication was performed.

## Evidence and validation

- [All four frozen inputs, provider observations and results](bitllm-20261109/evidence.json).
- [Exact manifest of the technically successful pair](bitllm-20261109/successful-manifest.json).
- [JavaScript test summary](bitllm-20261109/tests-summary.txt): **95 passed, 0 failed**.
- Source snapshot: `e8860613a51f8db985a70409f939d789708c74ba`, plus the integration
  changes in this commit. The evidence includes SHA-256 hashes of both runtime
  source files. Runtime: Node v22.23.4, BitLLM v0.5.0, macOS arm64.

Tests cover legacy requests, bounded thinking options, caller mutation,
frozen-config mismatch, cloud-comparison rejection, local execution, recovery
and existing scheduler behavior. The first sandboxed test invocation could not
bind its loopback fixture; rerunning with local networking enabled passed.
Rust, SBF and frontend checks were not run: those components were unchanged.

Transport observations were captured locally without changing the model
request. Published evidence retains exact answer content and hashes of raw
responses, omits reasoning text, and hashes peer identifiers consistently.
Credentials, node identity keys, installed binaries and caches are excluded.
These are local observations and peer-reported receipts, not independent
attestation of model identity, token counts or output quality.

## Decision and next observation

Use the supported explicit thinking switch for this Qwen route. Increasing
the output cap or spending NVIDIA credit was unnecessary for connectivity and
would have changed the experiment's resource assumptions. Reconsider this
choice if the advertised route disappears, stops honoring the switch, or fails
to return complete usage and valid answers. The next product test needs an
owner's real task and their decision; this synthetic run cannot supply either.
