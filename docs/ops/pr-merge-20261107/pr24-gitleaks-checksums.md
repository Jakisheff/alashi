# Historical checksum findings in PR24

План: 0,25 часа, до 17:16 (07.11.2026, Asia/Almaty).
Факт: 0,1 часа. Проверка завершена 07.11.2026 в 17:06 (Asia/Almaty).

Observed: [PR24 job 128397076686](https://github.com/Jakisheff/alashi/actions/runs/42370372495/job/128397076686) reported eight `generic-api-key` findings. Each value is the SHA-256 checksum of its named research file. Each checksum was independently recomputed from that file's Git blob at the flagged commit; all eight matched. The [classification record](pr24-gitleaks-classification.json) contains the field names, exact fingerprints and comparison results. It omits the values.

The four flagged commits are already ancestors of main `8c1dffb4267083bcbab6b084769846ada0f70670`. Both this failure and [PR67's successful scan](https://github.com/Jakisheff/alashi/actions/runs/42369793655/job/128395358051) used Gitleaks 8.24.3 with its default configuration. The difference was the first-parent scan range: PR24 scanned 34 commits beginning at `0814927a469a6716a33a037e53e56c07994ae900^`; PR67 scanned four beginning at `ea07eebe44ea2ede945f6f52f1cbb86a48d1fdeb^`.

The added exceptions identify only these eight commit/path/rule/line fingerprints. The existing three exceptions are preserved. There are no path or rule exclusions.

Local reproduction uses installed Gitleaks 8.30.1. The [red run](pr24-gitleaks-red.log) uses the exact CI range ending at `b38053c8ecec5d057337c1d2bf29fb002ceda000` and reports the same eight findings with exit code 2. The [green run](pr24-gitleaks-green.log) repeats that range with the eight exceptions: zero findings, exit code 0. GitHub Actions must verify the final commit with version 8.24.3 before merge.

Only the exception file and this evidence change. The frontend and game source remain identical to PR24 head `b38053c8ecec5d057337c1d2bf29fb002ceda000`.
