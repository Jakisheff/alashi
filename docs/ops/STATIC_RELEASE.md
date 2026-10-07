# Static release pointer helper

Use only with an already prepared, immutable directory under `/var/www/alashi/releases/<name>/`. This helper does not build, upload, delete releases, change nginx/backend, or perform public HTTP checks. Old release directories must remain available: the current r10 index still loads JS/CSS from the older v6r3 release.

The directory needs nonempty `index.html`, `version.json`, and `assets/`. `version.json` must contain full `frontend_commit` and `backend_commit` SHA-1 values and a nonempty `deck`; set these to the actual intended frontend, backend, and deck before publication. An optional `release` must match the directory name. The helper validates local script and stylesheet URLs under `/releases/<name>/` against existing files. It rejects symlinks, special files, and secret-like filenames in the candidate tree. Artifact provenance, metadata truth, content secret scanning, and public browser/HTTP behavior still need independent review.

For a reviewed release, from the repo:

```sh
python3 tools/static_release.py publish <name>
python3 tools/static_release.py rollback
```

Both commands accept `--root <site-root> --state-dir <private-state-dir>` for an isolated fixture. Default state is `/home/ubuntu/.local/state/alashi-publisher` (mode 700), outside the web root. Its lock and append-only JSONL journal are mode 600. A single nonblocking `flock` covers validation and switching. Each pointer uses a temporary symlink plus `os.replace`; the helper writes an `attempt` journal entry, switches `previous` to the old current, switches `current` to the new release, then writes `complete`. The two pointer replacements are **not one transaction**. If interrupted between them, inspect both links and the journal before another operation; equal pointers cause a closed failure. A failed switch is recorded as `failed` when possible. The final `complete` journal write or fsync can also fail after both pointers have switched. In either case, inspect actual `current`/`previous` and the `attempt` entry before retrying; do not blindly roll back.

Before a real switch, check the prepared files, immutable asset URLs, SHA metadata, and required project tests. Afterward, verify `/`, `/version.json`, JS/CSS/GLB, deck, and relevant API paths against the expected SHA; roll back if this fails. The helper itself does not perform these external checks. For a local rehearsal, run `python3 tools/static_release_test.py`; it creates two successive releases and rolls back entirely in a temporary root and private state directory.
