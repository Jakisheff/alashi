# Next steps

## Current scope

See [README](../README.md) for working capabilities and [NUMBERS](NUMBERS.md) for
dated evidence. The HTTP arena supports persistent games and authenticated token
recovery. The Solana program and HTTP mode share rules but differ in settlement
funding and randomness. Neither archived games nor test counts validate demand.

## Next decision: does an external operator get a useful result?

Run the proposed pilot with five external operators already comparing competitive
agents. Record integration time, whether the comparison changes a decision, and
repeat participation. Track any actual payment separately. Hold rules, opponents,
and agent versions constant and rotate seats before attributing results to skill.
The pilot is proposed, not completed.

## Technical prerequisites

- Re-run both epochs' SBF replay tests after rules changes.
- Resolve on-chain entropy coverage: license yield and customs are not covered by
  the law reveal in the same way. Exercise a real Switchboard oracle and failures.
- Test deployment, restart durability, and resource use before standing public hosting.
- Repair indexer durability and transaction provenance before treating its exports
  as a verified history. Version 2 exports recorded events without synthetic states.
- Resolve the economics of 90s settlement: the factory consumes the default 5% rake;
  HTTP license rent is exogenous simulated money and is not paid on-chain.

## Parked

Keep additional demos, new economic mechanics, leagues, MCP integrations,
certification, and wider ecosystem work parked until the pilot identifies a need.
Existing ideas remain in [IDEAS_PARKED.md](../IDEAS_PARKED.md). Public devnet and
mainnet readiness require separate verification; mainnet also requires security
and legal review. This roadmap does not authorize deployment or outreach.
