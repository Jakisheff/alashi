# Follow-up draft for Kaggle/kaggle-environments issue #1451

Status: DRAFT, waits for Amir's word to post. English only (target repo is
English-only). No markers of the owner's private canon (checked).

---

Seven days in, no maintainer response yet — normal for a volunteer-maintained
repo, so we kept building instead of waiting. Three updates that should make
evaluation cheaper:

**1. One-command deterministic demo (N=8).** A complete 8-agent game through
the real Rust worker using the uniform-legal null arm. No model keys, no GPU,
no network beyond loopback, no Python dependencies:

```sh
git clone https://github.com/Jakisheff/alashi && cd alashi
cargo build --manifest-path arena/Cargo.toml --locked --bin world_worker
node tools/agent-bootstrap/world-demo.mjs \
  --worker "$PWD/arena/target/debug/world_worker" --dir "$(mktemp -d)/world"
```

Output for seed 20261121 (three independent runs, bit-identical):

```json
{"status":"completed","agents":8,"calls":147,"accepted_actions":147,
 "rejected_actions":0,"skipped_decisions":0,"winner":"agent-006",
 "payout":76000000,"rake":4000000,
 "state_hash":"5867c34c97266962a3bfed2f660c5736fdd9294fafd3099944d9ce8ac85105dc"}
```

The run directory keeps the frozen config, every decision, and the engine
replay receipt; re-running the same seed reproduces the same `state_hash`.

**2. The dataset is alive.** world-100-grid (50 recorded 100-agent games,
`verify.py` recomputes all 24 claims from published rows, CI-checked on every
push) moved from 44 to 144 downloads in six days. One command reproduces the
headline claims: `pip install pyarrow && python verify.py`.

**3. Answers to the three questions we asked, by us, in advance.**

- Scale: the engine is parameterized 2–100 agents; 8–10 is enough for a demo,
  100 was the stress test in the preprint.
- Rust dependency: the plugin adapter would be a thin Python wrapper calling
  the worker over a stdin/stdout JSONL protocol (one process, zero Python
  dependencies); CLA would cover only that adapter, the engine stays MIT in
  its own repository.
- On-chain: optional adapter for mutually distrusting agents; the whole
  experimental corpus runs fully off-chain.

Next step on our side, barring other guidance: follow the repo's
create-environment methodology and open a draft PR with the adapter within
two weeks. A smaller-player variant or a demo-tournament format can be
prioritized instead if that makes evaluation easier.
