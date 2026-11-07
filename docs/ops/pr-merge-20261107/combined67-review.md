# Viewer and replay integration review

Independent read-only review found no blocker in the local combination of PR69 and PR67. The source conflict was one StreamPage JSX line. Resolution preserves onUnavailable={abandonRenderer}, onVisualTime={reportVisualTime} and receipt time from visualTime. The staging VM provides both callbacks.

Renderer failure clears active work, pending work and reactions before accepting further visual events. Public stream history is recorded before the visual callback. Late completion sees an empty queue; stale time callbacks are rejected by the visual-time generation and layout cleanup, including new-event, seek and unmount boundaries.

The root's combined lifecycle run passes all nine recovery checks and both healthy controls. The staging check passes, including the displayed-time receipt assertion. Independent review inspected those logs rather than rerunning them. Diff checks pass. The optional ALASHI_VISUAL_OUTPUT environment variable only selects the evidence directory, so later integration captures preserve the original repair session.

Final combined browser acceptance and exact-head CI remain required before PR69 merges. Game rules and backend sources are unchanged.
