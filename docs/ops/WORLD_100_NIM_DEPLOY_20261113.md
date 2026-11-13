# NVIDIA NIM deployment on Brev L40S for the model arm, 13 November 2026

Ops record for the model arm of the [bribe grid](WORLD_100_GRID_20261113.md).
Instance: Brev `ml-dev-ops-main-validation`, l40s-48gb.1x, stopped after
the measurement; the local grid arms never used the GPU.

## Path to a working NIM

- The stock image `nvcr.io/nim/meta/llama-3.1-8b-instruct:latest`
  (26.7 GB, weights bundled) requires a CUDA 12.8+ driver; the instance
  shipped 565.57.01 (CUDA 12.7) and the first container died on driver
  init.
- Older tags (1.1, 1.2) fit the driver but download profiles from the
  NGC hub at first start and returned 402 Payment Required: that NGC
  account has no model entitlement (a catalog subscription), so
  bundled-weight images are the workable route without a license.
- Driver upgrade path: the packaged 565 stack was held (`apt-mark`), so
  the resolver refused conflicts until all nvidia packages were unheld
  and purged, then `nvidia-driver-580-server` installed cleanly
  (580.178.04, CUDA 13.0). The purge also removed the container
  toolkit; `nvidia-container-toolkit` had to be reinstalled, and the
  new toolkit rejects the `--gpus` hook on this daemon, so the
  container runs with `--runtime=nvidia` and no `--gpus` flag.
- With the bundled-weight `latest` image and the 580 driver, NIM serves
  OpenAI-compatible `/v1/chat/completions` on container port 8000,
  mapped to host 9000, answering `meta/llama-3.1-8b-instruct` with
  valid usage fields and accepted `chat_template_kwargs`.

## Access pattern

Brev CLI auth works through a personal API key (`brev login --api-key`,
org resolves from the key); the oauth device flow saved tokens that the
CLI then rejected as logged out. `brev port-forward ml-dev-ops-main-validation
-p 9000:9000` exposes the NIM endpoint on loopback, which the frozen
world profile requires (`http://127.0.0.1:9000/v1/chat/completions`).

## Cost

Instance uptime for the model arm was about 1.5 hours at L40S pricing,
including the image pull, driver work and the 100-call compliance
measurement; the instance was stopped immediately after. No NGC
catalog spend (no entitlement used); the NGC key served only registry
auth for the image pull.
