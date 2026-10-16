# Marketing duel acceptance

The scope changed on 16 October 2026 after Amir clarified the product: two
marketers use their own tools and agents on the same advertising task, with a
GeoGuessr-style timed duel. The earlier sale-workflow implementation was removed
before publication. The first shared task is an English ALASHI ad for agent owners. Amir subsequently
clarified that ALASHI is an arena for testing skills and agents across tasks.
The UI uses owner teams and labels advertising as the first challenge. Only the
advertising result contract and acceptance checks are implemented in this pilot.

## Observed

The signed HTTP check completed two duels with four synthetic registered wallets.
It made 12 requests to a local deterministic provider, with at least three model
requests active at once. No external LLM requests were made. The source plan used
three parallel specialists and a writer. Its derivative used local brief
extraction, a draft, a local copy check and final delivery.

Usage fixtures were 480 tokens for the original plan and 240 for its derivative.
A fourth wallet completed the derivative in the next duel with 240 fixture tokens.
The provider returned identical ads to isolate workflow execution from creative
variation. The host's acceptance reason explicitly identifies a fixture review.
These figures do not establish real token savings or advertising effectiveness.
See [HTTP_RESULT.json](HTTP_RESULT.json).

The API hid both final ads until reveal and omitted intermediate reports from
public runs. A participant could not review its own ad. Format checks alone did
not produce a token leader. An interrupted report resumed from the private
journal without repeating the model request. Unknown usage remained unknown;
known tokens from failed answers remained charged. Snapshot failure rolled back
writes. Original instructions and tool bindings could remain private; derived
plans required publication. A private-trace regression exposed task names before
the fix. Public private-original traces now use opaque task labels and hide tool
identifiers, provider/model configuration and response identifiers. Types,
dependency shape, timing and costs stay observable; owners retain the full trace.

After stopping the isolated review server, OFFLINE appeared and playback held
1.500 seconds across two observations despite an active Play control. Restarting
the service restored the stored duel. See [BROWSER_RESULT.json](BROWSER_RESULT.json).

Desktop review showed the whole approved terrain, both Degenie teams and upright
A/B labels. The replay includes preparation, parallel travel, station work and
report return. Pause and seek allow inspection of the same recorded event trace.
The report marker stays attached to its carrier; it is an information marker,
not a physical goods exchange.

At a 390 by 844 browser viewport, the full map was visible and the page width was
390 pixels. Motion advanced from 1.5 seconds to 2.551 seconds after playback
resumed. The mobile view displays costs and complete ads below the map. It does
not make every station label readable at that scale. See
[desktop-motion.png](desktop-motion.png), [phone-map.png](phone-map.png) and
[result-comparison.png](result-comparison.png).

## Validation record

- Rust rules: 40 tests passed locally.
- Full arena: 132 tests passed locally, including the signed HTTP duel,
  seat persistence, owner cookies and the actual agent-process e2e test.
- Local runner: six Node tests passed, including ambiguous inflight recovery
  and preserving known usage when an owner tool emits an oversized report.
  A UTF-8 regression failed before the byte-limit fix and passed afterward.
- Frontend: TypeScript and production build passed. Task staging and exact hashes
  of the approved terrain, atlas and route layout passed.
- Animation suite: original pose, trade contact and rig checks passed, including
  827 legacy frames, 1,099 reaction frames and 5,196 action scenario frames.

Two existing test failures were reproduced during the full arena run. A mock
socket read returned WouldBlock; the test helper now explicitly uses blocking
reads and a two-second timeout. A five-second lobby expired before the real test
agent joined; its fixture now starts from observed joins and explicitly advances
the lobby. The fixture accepts a transition already performed by its background
crank instead of issuing a second advance. Production game timing and production chat behavior were not changed.

The [core implementation CI](https://github.com/Jakisheff/alashi/actions/runs/38936010694)
passed all five jobs on ad9a1d0, including fresh SBF execution, program host
compilation, indexer tests and bots compilation. Those latter checks were not
repeated locally. The final UTF-8 receipt guard has six passing local Node tests;
current-head CI is available in [PR 57](https://github.com/Jakisheff/alashi/pull/57).

## Limits

The browser reviewed public runs through an isolated local service. Browser wallet
login on the production HTTPS origin was not exercised here. Signed CLI owner
verification and authorization were tested against the HTTP server. Wallet
identities do not establish that different people control those wallets.

The system reduced-motion setting was not toggled. The map respects that setting
in code and holds meaningful poses; original atlas pose checks supplement this.
A new viewer's understanding and frame rate on a physical phone were not measured.

Provider usage remains owner-submitted. The owner-approved tool registry can call
an agent, but ALASHI cannot attest to a tool's declaration that it made no model
calls. Model prices, paid tool fees, clicks and sign-ups were not measured. No
monetary or conversion winner is declared. Host review is not a campaign test.

The platform route and backend are implemented in the repository. This work does
not deploy them to alashi.network or publish an advertisement. Generated founder
videos remain local; this change publishes code and PNG evidence only.
