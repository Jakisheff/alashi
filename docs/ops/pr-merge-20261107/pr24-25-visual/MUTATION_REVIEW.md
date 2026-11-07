# Coin mutation review, 7 November 2026

Observed: the focused run selected 18 mutation sites in contact geometry, symbol extrusion and fade. The first run killed 9 and left 9 survivors. This is not a whole-file mutation score: the scanner found 121 sites, and this run used no coverage instrumentation.

One survivor violated an existing documented invariant. Changing `thickness - bevel * 2` to `thickness + bevel * 2` raised the lilac field above the silver rim. The existing total contact-envelope check still passed because the raised symbol determined the outer depth. For the BUY coin, the field changed from 0.00203 scene units below the rim to 0.00203 above it. The new test compares the rendered field and rim bounds on both faces. It fails on this mutant and passes on the original source. No model code changed.

The rerun killed this survivor. Its stored result now contains 10 killed and 8 surviving selected sites. Logs retain the failed gate result; no clean mutation score is claimed.

| Remaining survivor | Classification and evidence |
| --- | --- |
| Field bevel multiplier `* 2` to `/ 2` | Cosmetic depth change. The field remains recessed on both faces and inside the documented contact envelope. No exact recess depth is specified. |
| Symbol `bevelEnabled: true` to `false` | Cosmetic profile change. The extruded symbol retains its outward faces, open aperture and envelope. No exact bevel profile is asserted by the contract. |
| Symbol `bevelSegments: 1` to `0` | Cosmetic profile change with the same surviving geometric invariants. |
| Symbol `steps: 1` to `0` | Changes extrusion tessellation/profile. All position, normal and UV values remain finite; both relief faces and the aperture tests pass. |
| Relief offset `+ radius *.004` to subtraction | Cosmetic relief-depth change. Both symbols remain raised, outward-facing and inside the contact envelope. |
| Side comparison `< 0` to `<= 0` | Equivalent on the only inputs, `[-1, 1]`. |
| Side comparison `< 0` to `< 1` | Equivalent on the only inputs, `[-1, 1]`. |
| Fade comparison `>.98` to `>=.98` | Changes depth writing only at opacity exactly 0.98. Opacity values, instance isolation and disposal are unchanged. The precise threshold has no documented external contract. |

The five cosmetic variants are not alternate accepted designs. This review accepts only the original rendered source against the owner's reference. The tests enforce its stated geometric constraints without pinning every artistic constant.

Files: `coin-mutations.log`, `coin-recess-mutations.log`, `coin-field-recess-red.log`, `coin-field-recess-green.log`, `mutation-classification.jsonl`. Crapper was run without coverage: coin CC 12, living rig CC 75, CRAP and coverage unavailable. Dryer at 0.8 found no duplicate candidates in the selected files. No unrelated rig refactor was made.
