# Research workflow from an idea to a reusable result

Implementation date: 26 October 2026. This extends the existing private comparison; it does not copy Family & Friends Store source code. The transferred patterns are server-enforced visibility, quiet collection, a decision inbox and a recorded owner choice.

```text
Private idea (now/later) → frozen question and one instruction change
→ optional independent forecasts → close collection → run the pair
→ compare evidence → save a reasoned decision → select the next baseline
                                          → review a public card → another owner's idea
```

## What a person can do

The owner stores an idea for now or later and prepares the existing comparison. The owner can invite registered wallets to forecast whether the candidate will meet the frozen decision rule. Each participant submits one probability and explanation. The owner cannot read other participants' forecasts before closing collection. Closure discloses the submitted forecasts to invitees and permanently stops further submissions. The response includes the number of missing participants. An open collection blocks the first comparison attempt; an existing attempt can still be retrieved through an exact retry.

The workspace shows an in-app daily digest in Almaty time. It includes the range and mean of disclosed probabilities, completed comparisons and decisions requiring action. The mean is descriptive, with basis-point rounding down; it is not a calibrated prediction or a measurement of business effects. Acknowledging the digest requires its current fingerprint, so an intervening update cannot be silently marked as read. There is no email delivery or background reminder.

After reviewing the pair, the owner records `keep_baseline`, `adopt_candidate` or `inconclusive` with a reason. Applying a conclusive decision saves the selected version for subsequent work. The comparison form uses it as the next baseline. A new duel request may use `version: "selected"`; the server resolves that version when it creates the run. Retrying an existing run retains its original version after a later selection. Active runs are unchanged. Private derivatives still require explicit publication before entering a public duel.

## Roles and data

| Participant | Before forecast closure | After forecast closure |
| --- | --- | --- |
| Comparison owner | Own private comparison, roster and response count; own forecast if participating | Submitted forecasts, disagreement summary and private experiment evidence |
| Invited wallet | Shared question and decision criterion, roster, count and its own forecast | Submitted forecasts; no access to private instructions, experiment reports or owner decision |
| Other authenticated owner | Own workspace and reviewed public cards | Same boundary |
| Public visitor | Only published cards and existing public duel data | Same boundary |

Cloud comparisons store their instructions and task reports on the ALASHI server. Provider credentials stay outside ALASHI. Declaring a comparison private protects it from other users; it does not remove server storage.

For private source material, [local experiments](../../tools/agent-bootstrap/LOCAL_EXPERIMENTS.md) keep the manifest, reports and decision in a local directory. That runner makes no ALASHI HTTP request. An explicitly configured remote model provider still receives the brief and instructions; a loopback model endpoint keeps that request local. File permissions do not provide encryption. Copied journals or experiment directories must not run concurrently on different machines.

## Public Goods publication

The owner writes and previews a separate card containing a mechanism, task, conditions, result and limitations. The server publishes only its allowed fields after explicit CC0-1.0 consent. It does not automatically copy the private question, prompts, reports, forecasts, wallet or decision reason into the card. User-authored text can still contain sensitive information, so this is reviewed publication, not automatic anonymization.

A cloud card requires a recorded decision, including an inconclusive one. Its `model` status describes the source of the comparison, not an independently verified effect. A local export receives `owner_reported_local`; the server has not observed that local execution. No card can claim server-verified real-world effects through a client-supplied status. Another owner can add the card to a private backlog while retaining its source identifier.

This is a library of reusable mechanism cards. General executable scenario packages and a cross-domain knowledge graph are not implemented by this change. The experiment still checks a static English advertisement brief; actual customer behavior remains outside its model.

## Storage and recovery

Research state uses the existing snapshot transaction. The decision, selected default and reviewed publications use the same lock and persistence path. Failed writes restore in-memory state; public library reads take that lock and cannot expose a publication before its save completes. This is one-process snapshot coordination, not a distributed transaction with an external model provider.

Old snapshots load with empty research state. Once the new field is saved, older binaries that reject unknown fields cannot read it. Preserve a matching binary and snapshot before deployment; restoring an earlier snapshot requires reconciling subsequent owner records. Do not discard new decisions to make an old binary start.

## Verification

The acceptance path is exercised through the real signed HTTP API in [research-http-check.mjs](../../tools/agent-bootstrap/research-http-check.mjs), called by the existing comparison integration test. Actual server responses are parsed by the frontend's [contract check](../../frontend/scripts/research-contract-check.ts). The tests cover sealed owner access, closure before execution, immutable forecasts, selected-version retries, explicit publication and reuse by another owner. Local runner tests exercise a loopback provider, partial execution, input changes and public export boundaries. No paid provider call or Solana transaction is needed for these checks.

[Acceptance evidence](FFS_WORKFLOW_20261026.json) records the signed HTTP checks and browser actions. The [workspace screenshot](ffs-20261026/workspace-digest.jpg) uses synthetic data. The [nginx fragment](ffs-research-nginx.conf) supplies the exact new routes; test it inside the active TLS server configuration before reload. Final test and release status are recorded in PROGRESS.md. FFS is an engineering reference; its commit count is not a cost estimate for ALASHI, and neither its code nor these tests establish demand from external teams.
