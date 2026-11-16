# Launch HN Agent37 thread (item 50059096): categorized buyer speech

136 comments, 2026-10-12..2026-10-15 (plus two later). Pulled from the Algolia
Items API on 2026-11-17; flat text in the session workspace, quotes verbatim
below with HN comment ids for re-verification (https://news.ycombinator.com/item?id=<id>).

Categories are analytical; the quotes are raw. Vendor (vishnukool) replies are
marked and excluded from pain counting.

## A. Credentials / blast radius / what the agent may do (7+ voices)

- talkarimli (50063391): "i think the sandbox part is getting commoditized... the harder part is what the agent can touch inside it. you connect google, the agent gets access to everything. sandbox protects the host. it does not protect your tokens. scoped, short lived credentials per task. that i would pay more for."
- ekorondy (50071898): "Most agent failures that actually hurt are not sandbox escapes. They are the agent acting as you with a coarse OAuth grant, or reading a secret that was sitting in the environment. A cheaper box does not fix that. You still need an egress allowlist the guest cannot rewrite, credentials that never enter the guest, and a hard call budget per task. I would pay more for that broker layer than for another 2x cheaper sandbox."
- ekorondy (50124142): "default-deny on fields the rule doesn't know about... let rules check values, not just whether a field is present... let the tool author tag each action as read, write, or irreversible, and treat anything untagged as irreversible."
- arbor-group (50071998): "the interesting boundary is really per-action rather than per-session... this task may read mail but not send it, or may create a draft but not charge a card... is a clear record of what the broker allowed after the fact enough, or do you want approval before anything irreversible?"
- ryanascend (50073873): "The agent never gets a key that can move money out. I fund a separate wallet, the agent gets delegated spending that I can revoke... The thing I worried about was never the agent breaking out of a box. It was the agent doing something allowed but dumb with a grant that was too broad."
- cagster (50064826): outside-broker pattern, method+path allowlist, total call budget.
- radim_h (50100782): same pattern shipped (stashbase.dev); open problem: "an allowed request being a bad idea".
- rajangdavis (50060144): Google connectors "kind of privacy nightmare... hand over access to every Google service I use".

## B. Wrong value-capture axis: sell the work, not the electrons (4 voices)

- mparis (50060422): "you may consider trying to get closer to the actual work that people actually need these agents to do... we have built our own agent sandbox infrastructure on top of firecracker... We will never sell it. It's part of our competitive advantage for doing healthcare tasks."
- mparis (50061198): "It's hard to make a sustainable business when selling something that costs $5 a month to developers... I'd be trying to figure out how to charge more and sell the value of the work you are enabling your customers to accomplish, not the cost of the underlying electrons."
- MikhailTal (50060717): "Sandboxes look a bit commoditized. You cannot 10x on basically what's just a VM with a few agent-friendly features in 2026."
- bitlad (50060361): "positioning yourself as a cheap provider is inherently a bad idea." (plus mbesto 50062026, skeeter2020 50061679: race to the bottom, incumbents can eat the loss.)

## C. State / memory must outlive the sandbox (3+ voices)

- openABandMemory (50133425): "The cheap-sandbox part is the easy half... a sandbox is disposable but the agent's context shouldn't be. If each run starts in a fresh VM, whatever the last session learned... is gone unless you persist it somewhere outside the box."
- mohamed01121986 (50066903): "Most of my agent work isn't compute heavy, it's the setup and teardown cycle that eats the time... how do you handle filesystem state between runs?"
- select1dev (50069352): "The things that actually cost me time were not CPU or RAM... network policy... environment hygiene. The sandbox needs a clean env with no inherited secrets, and a persistent sandbox that does not pick up changed env vars or allowlist entries without a restart bit me more than once. Third, a wall-clock timebox per run, because a stuck npm install behind a proxy fails silently rather than loudly."

## D. Reproducibility / auditability asked unprompted

- Sky_Joy3 (50151499): "Saying 'under 10% of the cost' is catchy, but what really matters on HN is which constraints you're loosening... Reproducibility: are runs deterministic and replayable for debugging?... Observability: what logs or traces do customers get for auditing calls, failures, and usage?"
- lasiotus (50065483): "gVisor is... less secure than VMs; but the front page price-compares against VMs, so not apples-to-apples."

## E. Sleep / lifecycle traps

- uzairhaq (50111787): "on the auto sleep — how does wake work for agents that listen on an outbound connection... Once the sandbox sleeps nobody's listening... are those agents expected to stay awake (and burn awake hours)?" Vendor: relay "would be the right way", not built.
- modo_ (50064771): "people set up crons to run every 15 minutes and accidentally spend a ton... Do you have a way to run a cheap check first?"
- pgcosta (50062588): how do cronjobs work in a sleeping sandbox at all.
- JHM168 (50065802): "cold start time was ever so slightly annoying."

## F. Self-hosting as the live alternative (non-customer tier-1 evidence)

- stillpointlab (50061261 + 50061643): building an ad-hoc Linux-box agent playground; "there are many tiny details to the whole setup... I'm actually surprised I don't see more discussion about this from devs."
- EddieRingle (50064718): "a few VMs in a Proxmox cluster... all self-hosted (so I don't have to worry about 'enough credits')."
- pranshuchittora (50061240): "Just use EC2 t4g.nano." debarshri (50066262): "Use digitalocean's microvms, they're cheaper."
- petesergeant, binsquare, salt-licker: DIY/open-source answers (byre, smolvm, yaac.dev).

## G. Incidental product signals

- Pricing anchors buyers use among themselves: boat.dev, fly.io Sprites, maritime.sh, Cloudflare agents ("30X lower cost"), AWS AgentCore/Strands Box (vscarpenter 50072925: open-source sandbox + policy engine + credential swap at gateway — the broker layer exists at AWS now).
- BAA/enterprise: ghm2180 (50063837) asks healthcare compliance; vendor: "depends on the contract value."
- Payoff method: bambax (50067623): card-only payment blocks some EU buyers.
- Vendor confirmations useful for ALASHI economics: average customer spend $0.37/mo; free-tier cost $0.05-0.10/user/mo with ~10 awake-hours/week cap (zeninfinity/vishnukool 50067766); oversubscription is the margin engine (vishnukool 50066044); cron API built because in-box cron cannot wake the box.
