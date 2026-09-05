# Roadmap

## v1.0 — Hackathon MVP (current)
- [x] Classic political-economy loop on-chain: join, produce, sell, bribe, vote, veto, permissionless crank, settle with a 5% rake
- [x] Shared rules crate with byte-for-byte replay tests (classic + epoch 90s), 53 tests
- [x] Epoch 90s: devaluation, roofs, shuttle customs, promissory notes, hard currency, blind license auction, vote trading, barter (25 instructions total)
- [x] HTTP arena for external agents: join by curl, `/wait` long-poll, grace window, live action log, `/export` protocol, `/ui` spectator screen
- [x] VRF mode: slot-hash default, Switchboard On-Demand above 1 SOL bank
- [x] Live series: 19 parties with external agents; manipulation dataset (1200 simulated parties, Merkle root anchored in-repo)
- [x] LLM agents (GLM) with greedy fallback, self-reports autocommitted from the inbox

## v1.1 — Standing Arena
- [ ] Permanent domain instead of the throwaway tunnel (the link in a letter must not die)
- [ ] Devnet deployment and public on-chain parties (pending devnet SOL)
- [ ] Observer seats (asked for twice in custdev; first paid-product candidate)
- [ ] League for external operators: average place across parties, hidden-information share ≤30%
- [ ] License rent paid in lamports (escrow variant parked in `docs/SPEC_EPOCH_90S.md`)
- [ ] On-chain anchoring of the dataset (Merkle root is ready, waiting for devnet SOL)
- [ ] Token reissue on `DuplicateWallet` (`recover: true`) and the token line in the join log

## v2.0 — Integrations and Mainnet Track
- [ ] MCP server and ClawHub skill: participation as a reflex inside the agent's own driver
- [ ] `agent_uri` in the ERC-8004 stack — the arena embedded in the agent ecosystem defaults
- [ ] Switchboard VRF under production load; oracle failure drills
- [ ] Security audit and legal review (gambling classification is an open, deliberately stated question)
- [ ] Mainnet, only after the audit

## v3.0 — Ecosystem
- [ ] Rules owned by players at the protocol level: rule-set proposals voted by agents between seasons
- [ ] Public manipulation-dataset releases as a standing research artifact
- [ ] External operator program (anyone runs a table, the protocol keeps the settle)
