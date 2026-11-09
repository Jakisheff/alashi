# First local experiment through BitLLM

Use the existing ALASHI local runner to compare one instruction change through
a BitLLM gateway. The example is an intentionally fictional Acme workshop
brief. It needs no ALASHI account, wallet, arena, NVIDIA instance or GPU rental.
This guide is a reproducible procedure, not evidence that a live run passed.

The question is narrow: does explicitly placing `Weekly workshop` first make
that supplied fact appear at the start of the headline? It tests request
compatibility and instruction-following for one pair. It does not test demand,
advertising lift or whether an owner prefers the candidate.

## Data and evidence boundary

BitLLM listens locally, but requests for a peer's model leave the machine. The
serving peer sees the brief, instructions and dependency reports. Use the
synthetic fixture below for this first check. A loopback URL does not make a
forwarding gateway a local-only model.

Set `source` to `provider_reported`, even though the gateway URL is loopback.
`test_provider` is for deterministic local mocks. Provider token counts are
reported usage, not independently measured usage. The runner leaves
`cost_usd: null`; zero NVIDIA expenditure does not establish a zero provider
price or a monetary value for BitLLM credits.

The current adapter records the requested model and gateway origin. It does
not retain the response's `X-Bitllm-Peer` header or prove the model weights.
BitLLM may select different peers under one model name. Unless execution
provenance is captured separately, an observed difference cannot be attributed
solely to the changed instruction. Describe it as a result from the named
BitLLM route, with the underlying weights unverified.

## Prepare the gateway

Use Node 20 or newer and a verified BitLLM release binary for your platform.
Download tools outside this repository; a portable binary does not require
`sudo`. Check the chosen release's checksum and record its version. Follow the
[BitLLM release instructions](https://github.com/bitllm-org/bitllm#install-and-run)
for installation; keep any node identity or account files outside Git.

If a gateway is already running, reuse it and inspect its configuration. Do
not start a second process on the same port or overwrite existing node state.
For a separate temporary gateway, replace the absolute path below with a
private directory and run the verified binary in the foreground:

```sh
umask 077
mkdir -p /absolute/private/bitllm-state
bitllm --data /absolute/private/bitllm-state \
  --auto-update=false --mdns=false --upnp=false --ipv6=false
```

These flags keep this run on the selected binary version and avoid LAN
discovery, router port mapping and IPv6 listening. No model is shared because
there is no `--upstream`. Joining still needs outbound network access and
reachable peers. A fresh state directory has a new node identity; existing
membership does not automatically transfer to it. Inspect the gateway's
dashboard at `http://127.0.0.1:7778` if account access needs attention.

In another terminal, list the currently advertised models:

```sh
curl --fail --silent --show-error --max-time 10 \
  http://127.0.0.1:7778/v1/models
```

Copy an exact supported Qwen model ID into the example manifest. An advertised
model is not proof of an available slot or a successful completion. The runner
keeps `temperature: 0` and `max_tokens: 256`. The example also requests:

```json
"chat_template_kwargs": {"enable_thinking": false}
```

This switch is part of the shared `profile`, not one variant's instruction.
BitLLM v0.5.0 lists this key for `qwen3.8-flash-next-fp4-alesha` and allows its
boolean value through the peer request boundary. It is a requested setting,
not proof that the remote model honors it. The same source lists
`{"thinking": false}` for its DeepSeek reference models; that source-defined
mapping alone is not evidence that an ALASHI experiment on DeepSeek passed.
See the [model mappings](https://github.com/bitllm-org/bitllm/blob/v0.5.0/internal/models/models.json)
and [request field boundary](https://github.com/bitllm-org/bitllm/blob/v0.5.0/internal/node/boundary_request.go#L280).

The optional object accepts exactly one boolean key, `enable_thinking` or
`thinking`; it does not accept arbitrary provider options or both keys together.
Choose the supported key before freezing. Omit the object for providers that
do not support either switch; absence preserves the previous request body.
Do not substitute `/no_think` in the prompt as a reliable equivalent: it can
still leave reasoning output and a truncated final answer. If the gateway
requires a key, set `ALASHI_MODEL_KEY` privately in the environment; do not
place a credential in the manifest or command history.

## Prepare and freeze the pair

Run from the repository root. Install the existing bootstrap dependencies:

```sh
npm ci --ignore-scripts --prefix tools/agent-bootstrap
node tools/agent-bootstrap/local-experiment.mjs init \
  --dir /absolute/private/bitllm-experiment-01
cp tools/agent-bootstrap/bitllm-manifest.example.json \
  /absolute/private/bitllm-experiment-01/manifest.json
chmod 600 /absolute/private/bitllm-experiment-01/manifest.json
```

The parent directory must already exist. The runner creates the experiment
directory with mode 700. Replace `profile.model` with the exact available Qwen
ID supporting `enable_thinking` before the first freeze. If your gateway uses a
different local port, update `profile.url` too. Keep the two plans and the
synthetic brief unchanged for this example. Credentials do not belong in the JSON.

The baseline asks for a clear advertisement using the supplied facts. The
candidate adds just one sentence: `Begin the headline with "Weekly workshop".`
Both run the same local fact-extraction tool before the final model task.
The manifest records the prediction, decision criterion and failure conditions
before any model request.

```sh
node tools/agent-bootstrap/local-experiment.mjs freeze \
  --dir /absolute/private/bitllm-experiment-01
```

Keep the printed digest with the run evidence. After freezing, do not edit the
manifest. A different model, thinking setting, question or instruction requires
a new directory. The thinking object is included in the digest and matched
against the provider before either variant can make a request.
The 30,000-token budget is a per-version reservation ceiling, not an expected
usage or a provider-enforced billing limit. Reservations include a conservative
allowance based on request byte length, so a very small budget can prevent a
request from starting.

## Run and inspect

The next command sends the fictional fixture to BitLLM peers. A successful
pair makes two model calls. The runner starts both variants concurrently and
uses the same frozen requested model profile:

```sh
node tools/agent-bootstrap/local-experiment.mjs run \
  --dir /absolute/private/bitllm-experiment-01 --allow-provider yes
node tools/agent-bootstrap/local-experiment.mjs report \
  --dir /absolute/private/bitllm-experiment-01
```

Each request has a 60-second timeout; the shared 300-second experiment window
does not extend it. The runner does not automatically retry failed or ambiguous
model calls. Reusing the same `run` command reconciles saved state and completed
work without duplicating those requests. Preserve an unsuccessful first attempt
and its unknown usage; do not delete journals to make a retry look successful.

An HTTP 200 response can still fail this contract: a reasoning model may spend
its 256 output tokens on `reasoning_content`, return `finish_reason: "length"`
and leave the JSON truncated. Known token usage does not make that output valid.
Increasing the manifest's reservation budget does not increase the adapter's
256-token output limit. Preserve that failed attempt. If another advertised
model is tried, freeze a new experiment directory and label it compatibility
exploration, not a successful rerun of the original frozen pair. Also record
any difference between the requested model ID and the response's `model`; an
apparent alias does not establish equivalent weights.

Record these observations alongside the source revision and BitLLM version:

- The exact requested model ID and frozen digest.
- Each version's completion status, output, format checks and reported usage.
- Whether each headline begins with `Weekly workshop`, ignoring case, and
  whether any claim goes beyond the fictional supplied facts.
- Any captured peer provenance and timing; explicitly mark them unavailable
  when they were not collected. A single pair cannot establish latency percentiles.

If both versions pass and only the candidate has the prefix, the narrow
instruction-following observation holds for this pair. If both have it, this
criterion shows no difference. Invalid output, incomplete responses or unknown
usage make the check inconclusive. Even a technical pass leaves the owner's
preference pending: `ready_for_owner_decision` is not an adoption decision.

Show both drafts to the owner before using the `decide` or `apply` commands in
[the local experiment guide](LOCAL_EXPERIMENTS.md#review-decide-and-apply).
Do not fabricate owner preference or publish a Public Goods card as part of
this smoke check. Keep node state, credentials and installed tools out of Git.
Stop a gateway started solely for this test with Ctrl-C after collecting the
evidence; preserve its state if the node identity is still needed.

## Local verification

```sh
node --test tools/agent-bootstrap/local-experiment.test.mjs
```

Those tests use deterministic providers and do not establish live BitLLM
availability. See [BitLLM access and model caveats](https://github.com/bitllm-org/bitllm#what-you-should-know)
for the distinction between advertised identity and verified execution.
