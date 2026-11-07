# PR 67 review artifacts

[Acceptance and limits](ACCEPTANCE.md) | [Final readiness review](REVIEW.md)

The source under review is `5a80a6ff37de1eba7b238ebf91ff61fa49c327e5`. Animation and replay source blobs match refreshed head `1605f03f`. The reviewer did not edit source or contact production. Final browser runs used local demo fixtures with external requests blocked.

- [Phone motion video, 34.36 seconds including startup](phone-reduced-motion-34s.webm)
- [Desktop motion excerpt, 35 seconds](desktop-pause-seek-35s.webm)
- [Final 24-case resume results](resume/pose-analysis.json) and [log](resume/run.log)
- [Desktop pause/seek results](desktop/observations.json)
- [Phone pause/seek results](phone/observations.json)
- [Final phone motion trace](motion-visible/observations.json)
- [Phone purchase and receipt](phone/390-buy-context.png), [seek zero](phone/390-buy-seek-zero.png)
- [Phone Mule resumed pose](resume/390-mule-resumed.png), [canonical pose](resume/390-mule-canonical.png)
- [Phone Vote](resume/390-vote-resumed.png), [Thinking](resume/390-thinking-resumed.png), [Victory](resume/390-victory-resumed.png)

`ACCEPTANCE.md` explains the excluded raster-hash attempt, overloaded recorder, offscreen-resume harness attempts and deliberately interrupted redundant sweep. Their failures are not included in the accepted counts. Raw long recordings and built site files are excluded from this package.

The fixture scripts retain the local paths used in the review for traceability. `build.mjs` documents the isolated build; `phone/run.mjs` documents the completed pause/seek checks; `resume/run.mjs` contains the corrected in-view advancement checks; `motion-visible/run.mjs` creates the bounded motion sequence. Reuse requires existing local dependencies and adjustment of the checkout/output paths on another machine. They do not create or submit live games.
