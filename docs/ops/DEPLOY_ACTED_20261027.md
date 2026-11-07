# Frontend action-status deployment, 27 October 2026

Status: deployed and verified at 2026-10-27T16:27:10+0500 (Astana). The static release `acted-30f44fd-20261027` was built from `30f44fd2b4723eced2c74dd1517f7960b049a73e` and serves the confirmed `acted` value to the Donkey Risk projection. The runtime remains at `39bda6678ec787eaa47e8c7c2768646e6b2f9fa3`; chain API remains at `523e14ac058a58ff25bc4786407fbb8bd60105fe`. The previous static release is `research-39bda66-20261026`.

## Validation

A clean `npm ci` and production build with Node 22.23.4 reproduced all 120 staged frontend file hashes; CI uses 22.18.0. The immutable release prefix is `/releases/acted-30f44fd-20261027/`. All 318 candidate files matched their staged SHA-256 values, including the 60 preserved presentation files and the unchanged research runner archive. Candidate files and directories use 0644 and 0755; the publisher's release validation passed. The previous release's non-build files were retained without changes.

Before and after activation, 118 public HTTPS checks passed: the manifest, routes, 48 versioned JS/CSS hashes, runner, GLB, map, presentation downloads, public libraries and anonymous private-route rejections. The check set derives from [the 26 October evidence](FFS_DEPLOY_20261026.json); candidate asset hashes, keyed by SHA-256 of their relative UTF-8 paths, and results are in [deployment evidence](DEPLOY_ACTED_20261027.json). Headless Chrome loaded the map and `/devnet` without console errors. The confirmed public Game response carried `acted:false` for two factions; the frontend adapter retained both values and kept replay status unknown. The local fixture also tested both boolean values.

There were no active HTTP games immediately before switching. All four services remained active with unchanged PIDs and executable hashes; no backend or nginx change, devnet transaction, or paid provider call was made. [PR 61 CI](https://github.com/Jakisheff/alashi/actions/runs/40493799689) passed frontend and other runtime jobs; RustSec failed on pre-existing lockfile findings. A separate npm audit reported a high-severity `zod` advisory in the development router plugin dependency; the lockfile was unchanged.

## Rollback

The publisher retained the prior release as `previous`. On the host, inspect both links and use `python3 /tmp/alashi-acted-publisher.py rollback` while `previous` still points to `research-39bda66-20261026`; alternatively the repository helper can explicitly select it with `python3 tools/static_release.py publish research-39bda66-20261026`. Verify both pointers and rerun the public, browser, service and API checks. The static rollback does not change backend state.
