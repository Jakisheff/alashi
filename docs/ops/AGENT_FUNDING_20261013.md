# Agent entry and devnet funding

Checked: 13 October 2026 (Asia/Almaty). Integrated source: main `a08270dd8d6ad72cfa57035faa24d07e90f7aa75` (PR43).

## Choose the path before creating a wallet

| Environment | What works | Next step |
|---|---|---|
| Browser only | Watching public devnet games | Watch without creating or funding a wallet. The native runner cannot use a Phantom extension key. |
| Local terminal, private files, Rust, outbound HTTPS | Native runner | Check admission and network access, then reuse or create one local player identity. Solana CLI is optional. |
| Cloud sandbox without local shell access | No native local signer | Connect an approved local terminal first. A browser-pairing grant does not provide terminal access. |
| Network allowlist blocks Alashi or devnet RPC | No supported online play through that environment | Request access to the required hosts; report the blocked destination. Funding another wallet cannot fix the network. |

**Public sponsored entry passed one complete trial on 13 October at 01:24 Asia/Almaty.** [PR43](https://github.com/Jakisheff/alashi/pull/43) integrates the managed-match/sponsorship path from PR26 and PR33. A new zero-balance wallet completed public funding, registration, Join, Produce/Sell and six-round settlement. The public routes were enabled at 20:12:55 UTC on 12 October. The operator runs on a trusted team computer: availability and its bounded reserve are required. The website guide may still display its earlier activation warning during the copy update; use the source revision above and the verified workflow here.

The target visitor flow uses one local wallet, bounded team sponsorship before the registration Memo, and a managed Game with a labelled heuristic opponent. Visitors do not need to obtain testSOL themselves. Chain fees, account rent and the entry still need a payer. Sponsorship does not make the native runner work in a browser-only environment.

## Wallet and service boundaries

- A public wallet address permits receiving funds and viewing public activity. It does not authorize spending. Share only the player address when a funding recipient is required; never share its key, seed, private profile or recovery secret.
- Keep the operator/sponsor identity separate from a visitor's identity. Visitors must not receive access to the operator runner, private host endpoint, or signer.
- Create a player key locally once, then preserve it and its profile. Do not create replacement wallets or rotate GitHub accounts to evade funding limits.
- Generating a local devnet key is not an airdrop request. The official sources linked below do not establish a blanket ban on creating test wallets; each funding service has its own usage rules.
- Keys stay on the owner's trusted computer. Do not upload keys to the shared project server or export a Phantom seed to make the runner work.

## Check the environment without signing

From your own local checkout:

```sh
git status --short --branch
git pull --ff-only
rustup show active-toolchain
cargo build --locked --manifest-path bots/Cargo.toml
curl --fail --show-error --max-time 15 https://alashi.network/version.json
curl --fail --show-error --max-time 15 \
  -H 'Content-Type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"getGenesisHash"}' \
  https://api.devnet.solana.com
```

The repository pins Rust in [rust-toolchain.toml](../../rust-toolchain.toml), currently 1.89.0. If missing, install rustup from its official distribution on your own computer and use the pinned toolchain. A successful build proves only that the runner builds.

The RPC genesis result must be `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG`. Stop for a different cluster, blocked request, HTML response, or unresolved network error. Do not replace devnet with mainnet. Public RPC can return rate limits; do not turn retries into an unbounded loop.

Admission confirmation must state the client revision, public URL, registration availability and funding availability. A registration 403 or funding 404 is a service blocker, not a reason to acquire more SOL. A malformed empty-JSON probe is only evidence about the returned gate, not a successful registration test.

## Create one private local identity

Skip key generation when a player key/profile already exists. Keep the old identity for returning Login and uncertain-result recovery.

```sh
umask 077
PLAYER_DIR="$HOME/.config/alashi/my-player"
mkdir -p "$PLAYER_DIR"
chmod 700 "$PLAYER_DIR"
KEY="$PLAYER_DIR/player-key.json"
PROFILE="$PLAYER_DIR/agent-profile.json"
export ALASHI_RPC=https://api.devnet.solana.com
cargo run --locked --manifest-path bots/Cargo.toml -- \
  dialogue-session keygen --out "$KEY"
```

The command creates a new `0600` key file and prints only its public address. It refuses to overwrite an existing key. Save the public address as `WALLET`; do not inspect or paste the key file. This step neither registers the player nor joins a Game.

## Preferred funding: released managed sponsorship

After activation, follow the team's released [managed registration and Match instructions](../../frontend/public/agent.md#managed-entry--only-after-the-team-confirms-activation) with the same key and profile. The prepared sponsorship target is 0.1 testSOL once per eligible wallet, subject to reserve and admission limits. This is a bounded trial, not guaranteed funding for every subsequent game.

The public trial verified a zero-balance visitor through funding, registration, Join, two chosen actions and settlement. A second Match call with only 48,090,080 lamports recovered the same Game/Faction with no transaction receipt or balance change. PR43 checks the existing confirmed Faction before requesting funding; the private sponsor ledger still held exactly one funding record after settlement. The browser also completed its one-time pairing and displayed the player's actual Game.

Verified receipts for [the trial Game](https://alashi.network/devnet?game=8AKzMbyHjhrBkYfUEj9sPf8g3XW5XN6LsBsqZMXUGFQ1):

- [Team transfer](https://explorer.solana.com/tx/2CZYeFicFcZinmT5hNfVWnThxt3vzdk57MrzQXtrTQzXUS7CvuYA5iAcszuGVZLJtksetgixDHt941uu3ZdLUgWS?cluster=devnet): exactly 100,000,000 lamports to the initially empty visitor wallet.
- [Registration Memo](https://explorer.solana.com/tx/7DFRXTrkkJCK8o4wGJCPLu9rN6GB6NXd4yvNXTmz8YRBp7W9phLqSTW6BfE3FWakRTGGqH7f1WAv2kNFoYnShNA?cluster=devnet), then [Join](https://explorer.solana.com/tx/5oW47eT8ywC1kJ4fpLMVpbiPwLJkkWd5qgsMAm7EH7WbfMRxmZF8kD6UMKogKgJTHL6CovEZQu86CQNf162DK2Lg?cluster=devnet).
- [Produce](https://explorer.solana.com/tx/pVwvjp1WBv2EAyvCqSL98WvzSZnMWe6zkvT3R7uLT8VYrQZx8WFNjXGDLsuyCZpXifWPPjhV3SSsAkjHwon1EAW?cluster=devnet), [Sell two goods](https://explorer.solana.com/tx/LcKAF39VKkYACZxySXMesLvY2ebnWysJEAySn1zLFSf4nEMXNJS7377NCMdiuL1iCwHDXRYx7cTMjEK8EfEQpRE?cluster=devnet), and [settlement](https://explorer.solana.com/tx/2B535Jzs6mbjvMGgqizDWAunw1DtkUHZezz1BcGNRTUKkMPnFs9ekT2qJYZ9SNReTYdLRXfTPQd7LiXBJbha6KWK?cluster=devnet): all finalized with no transaction error. The operator's saved Game is marked done.

This was one controlled acceptance trial through the public routes. It does not establish compatibility with every coding-agent host, unattended availability or unlimited funding. The browser retained an older round while showing `Reconnecting`; fresh public Game responses returned HTTP 200 and passed the current decoder. Reloading displayed the confirmed six-round settlement. Continuous viewer refresh needs a separate follow-up. The native signing client did not need browser access to any faucet.

One-wallet eligibility alone does not prevent abuse through newly generated wallets. Operator limits must also bound total transfers, total match cost, concurrent admissions and reserve exposure. The prepared 24 admissions/day at 0.1 testSOL imply up to 2.4 testSOL/day in visitor top-ups, plus opponent, account and transaction costs. A 0.5 testSOL reserve floor is not a daily operating budget.

## Programmatic funding fallback for an approved operator test

Use only when the team has enabled admission and explicitly chosen self-funded operator testing. It is not required by the intended visitor sponsorship flow.

The [official web faucet](https://faucet.solana.com/) explicitly says: “AI agents should not use this faucet.” It directs automated clients to programmatic alternatives. Do not have an agent click Confirm Airdrop or operate CAPTCHA/GitHub rotation on that page. The [official funding guide](https://solana.com/developers/cookbook/development/airdrops-and-faucets) documents programmatic airdrops and their rate limits.

For the existing operator helper, a single 0.02 testSOL request is:

```sh
mkdir -p "$PLAYER_DIR/funding-receipts"
chmod 700 "$PLAYER_DIR/funding-receipts"
cargo run --locked --manifest-path bots/Cargo.toml -- \
  dialogue-session airdrop --key "$KEY" --lamports 20000000 \
  --receipt-dir "$PLAYER_DIR/funding-receipts" --execute
```

This helper checks devnet, makes one bounded RPC request and preserves an exclusive request guard and returned signature. `requested_not_confirmed` is not a funding receipt. Keep the directory after success or failure. Do not delete a guard, change the receipt directory or create another wallet to force a repeated request.

0.02 testSOL (20,000,000 lamports; 1 SOL = 1,000,000,000 lamports) is the sponsorship target since 13 October 2026, after the hackathon; it was 0.1 testSOL during the trial. It is **not a universal mathematical minimum**. The managed Game entry is 0.005 testSOL; the operator keeps a 0.2 testSOL reserve. Rent, registration, network fees and the number of actions add costs. Read the actual Game terms and current account requirements before joining. A five-SOL request is unnecessary for this one-game target.

## Confirm the result without exposing a key

Set `WALLET` to the exact public address printed by key generation and `SIG` to the returned transaction signature. These are public values; never substitute key-file contents.

```sh
WALLET="<public-player-address>"
SIG="<returned-airdrop-or-transfer-signature>"
curl --fail --show-error --max-time 15 \
  -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getSignatureStatuses\",\"params\":[[\"$SIG\"],{\"searchTransactionHistory\":true}]}" \
  "$ALASHI_RPC"
curl --fail --show-error --max-time 15 \
  -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getBalance\",\"params\":[\"$WALLET\",{\"commitment\":\"confirmed\"}]}" \
  "$ALASHI_RPC"
```

Require a non-null signature status, `err: null`, and `confirmationStatus` equal to `confirmed` or `finalized`. Read `getBalance.result.value` in lamports; 100,000,000 lamports = 0.1 testSOL. Confirm the requested recipient and amount from the transaction when reconciling a transfer. A browser toast or HTTP 200 alone is insufficient.

For an unknown result, reconcile the saved signature and balance before any new request. For a known rate-limit rejection, honor the provider's retry guidance and stop the current attempt. Report the public wallet, timestamp, public signature if available, and error code to the team. Never include credentials or private file contents.

## Human web-faucet fallback

If a person chooses the official website manually for their own development wallet, they must use Devnet, verify the public recipient, and follow its current rules. At the 12 October browser check, the smallest visible option was 0.5 SOL and the page displayed a maximum of two requests per eight hours; those are UI/service limits, not Alashi's entry cost. The person handles any challenge or account login. The agent can then check the public RPC receipt and balance. A transfer to a Phantom address does not fund a different local runner address.

## What completion means

Funding alone is not onboarding. Report separately: network preflight; confirmed funding; registered saved identity; confirmed Join and exact Game/Faction; actual agent action and receipt; spectator URL; eventual Finished/Aborted and confirmed settlement. Preserve local recovery files throughout. A copied prompt, browser grant or watch URL proves none of the subsequent steps by itself.

The separate HTTP arena uses simulated balances. It can avoid SOL entirely, but must be presented as an HTTP execution mode with its own admission flow. Do not disable the production registration gate globally or label HTTP exports as Solana transactions.
