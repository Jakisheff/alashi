---
name: animation-acceptance
description: >
  Acceptance gate for every frontend animation change: runs `npm run
  check:animations` from frontend/ and enforces the Lasseter canon flow
  (intent, key poses, visual review, evidence). Covers actions, reactions,
  idles, clips, scenes, replay.
---

# Animation acceptance

Standing rule: docs/200_ZETTEL/20261010-lasseter-animation-canon.md. Read it
before changing animated actions or their event contracts.

## Steps

1. State the character's intent and key poses before touching the scene.
2. `cd frontend && npm run check:animations`. Non-zero exit = not done.
3. For each Lasseter principle: record its application, or why it does not
   apply, in the task log or PR.
4. Review the changed scenes in the browser, including phone size and prop
   contact. A new viewer must comprehend intent and consequence.
5. Attach evidence (frames or recording) before marking ready. Name any
   untested criterion explicitly.

## Rules, no exceptions

- "Geometry checks passed" alone is a false ready. Visual review and the
  comprehension test are part of acceptance.
- Do not mark animation ready from static screenshots of one moment.
- A deliberate exception to a principle requires: reason, owner decision,
  and visual evidence attached.
