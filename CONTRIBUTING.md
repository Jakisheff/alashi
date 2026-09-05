# Contributing to Alashi

Thank you for your interest in contributing to the arena where the rules belong to the players!

## How to Contribute

### Reporting Issues
- Use GitHub Issues to report bugs or suggest features
- Include steps to reproduce, expected and actual behavior
- For game-rule questions, cite the spec (`docs/SPEC_EPOCH_90S.md`, `docs/SPEC_VOTE_CONTRIBUTION.md`) rather than a played party: parties are evidence, specs are the canon

### Submitting Pull Requests
1. Fork the repository
2. Create a feature branch: `git checkout -b feat/your-feature`
3. Make your changes with clear, conventional commit messages
4. Ensure tests pass: `cargo test` (workspace) and `cargo test --manifest-path arena/Cargo.toml`
5. Open a Pull Request against `main`

### Commit Convention
We use [Conventional Commits](https://www.conventionalcommits.org/):
```
feat: add vote-trading hint to the arena
fix: settle refund when the bank is empty
docs: update the action dictionary in api.md
test: add barter acceptance e2e
```

### Rules Changes Are Special
The rules crate (`rules/`) is the single source of truth shared by the on-chain program, the simulator, and the arena. Any change there must keep the replay-equivalence tests green in both epochs, and the five mandatory test areas must stay covered: happy path, voting, forced exit, payout, rake.

### Development Setup
See [Quick Start](README.md#quick-start) in the README. The fastest loop is the HTTP arena: `tools/stack_up.sh`, then play a party with curl in one minute (`docs/QUICKSTART_JURY.md`).
