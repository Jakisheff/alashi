# Competitor issue trackers: top-commented issues (GitHub search API, 2026-11-17)

E2B: 424 issues total; Daytona: 1257. Top by comments. Raw titles, grouped by
the pain they evidence.

## E2B (e2b-dev/E2B)

| Issue | State | Comments | Pain evidenced |
|---|---|---|---|
| 401 Not Authorized when `e2b template build` | closed | 18 | setup/teardown tax, auth friction (C) |
| Building a template that requires running commands like copy, fails | closed | 17 | setup tax: template builds are fragile (C) |
| Error running code on Vercel edge environment | closed | 16 | environment mismatches (C) |
| Paused sandbox is not persisting the file changes / addition after second time resumed | closed | 15 | state across runs is fragile (C) |
| 502 badgateway error | closed | 14 | silent failures (C) |
| Template build stuck at file copy | closed | 14 | silent hangs: "stuck" (C) |

[my analysis] E2B's most-discussed issues are not "too expensive" or "too
slow": they are setup fragility, silent hangs, and state loss. This matches
HN pains C and E from the thread file.

## Daytona (daytonaio/daytona)

| Issue | State | Comments | Pain evidenced |
|---|---|---|---|
| OSX Daytona workspace | closed | 97 | platform basics |
| Windows Daytona workspace | closed | 58 | platform basics |
| Rename Workspace to Dev Environment | closed | 30 | positioning churn |
| Error Details: Failed to fetch | closed | 29 | opaque failures (C) |
| Persist dashboard sorting states | closed | 29 | UI state |
| private registry x509 problem | open | 28 | setup tax, credentials adjacent (A/C) |

[my analysis] Daytona's pain cluster is workstation-era (local dev
environments), which is their history; less relevant to agent-era buyers than
E2B's cluster.

Boundary: issue counts are a coarse proxy (a minority files issues). They are
used here only to corroborate the HN thread categories, not as standalone
demand evidence.
