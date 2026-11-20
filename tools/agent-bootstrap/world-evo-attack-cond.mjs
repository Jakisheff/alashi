#!/usr/bin/env node
// Conditional-policy adversary harness, 21 November 2026 (genome44 prereg).
// Copy of world-evo-attack.mjs extended with round- and cash-conditioned
// policies: the classes the 20261114 adversary coverage explicitly excluded
// ("conditioning on round, cash thresholds or opponent history"). The static
// genome path is untouched; a conditional genome carries a `cond` object that
// patches the base genes per decision point before the original logic runs:
//   cond: { kind: 'round_switch', at: 4, before: {t:0}, after: {t:4} }
//     from round `at` the `after` patch applies, before it the `before` patch.
//   cond: { kind: 'cash_threshold', threshold: 20_000_000,
//           over: {a:0,s:0}, under: {a:-1,s:1} }
//     per acting agent: own public cash over the threshold takes `over`,
//     otherwise `under`. a:-1 means "never bribe" (appetite gate: x % 0 is
//     NaN, never === 1), which keeps the extension inside the existing
//     chooseForAgent branch — no new action outside the worker menu.
// Modes mirror the original: calibrate (anchor richest|half|donkey on seed
// 20261303 must reproduce the published row bit for bit) and genomes <file>.
// The serve mode is inherited unchanged and does not synthesize a round
// field; round_switch there resolves as round 1. Do not use it for cond runs.
import { createHash } from 'node:crypto'
import { spawn } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

export const WORKER = resolve(process.argv[2] && process.argv[2].endsWith('.mjs') ? '' : process.cwd(), '')
const RULES = ['richest', 'poorest', 'max_id', 'min_id', 'first_slot', 'seeded_random', 'by_influence']
const SELLS = ['all', 'half', 'unit']
const ALTS = ['produce', 'donkey']
const LAWS = ['abstain', 'yes', 'no']
export const ANCHOR_L4 = { t: 0, a: 0, s: 0, alt: 0, law: 0, k: 0 }
export const ANCHOR_FIRST_SLOT = { t: 4, a: 0, s: 0, alt: 0, law: 0, k: 0 }
export const UNIFORM = { t: 5, a: 0, s: 0, alt: 0, law: 0, k: 0, uniform: true }
export const ANCHOR_HALF_DONKEY = { t: 0, a: 0, s: 1, alt: 1, law: 0, k: 0 }
export const CALIB_SEED = 20261303

const hashDraw = (...parts) => {
  const h = BigInt('0x' + createHash('sha256').update(parts.join(':')).digest('hex').slice(0, 16))
  return Number(h % 100000n) / 100000
}
const patchId = (p) => p ? Object.entries(p).map(([k, v]) => `${k}${v}`).join(',') : '-'
const condId = (c) => c.kind === 'round_switch' ? `round_switch@${c.at}[${patchId(c.before)}->${patchId(c.after)}]`
  : c.kind === 'cash_threshold' ? `cash_gt_${c.threshold / 1_000_000}M[${patchId(c.over)}|${patchId(c.under)}]`
    : String(c.kind)
const genomeId = g => `${RULES[g.t]}|a${g.a}|${SELLS[g.s]}|${ALTS[g.alt]}|${LAWS[g.law]}|k${g.k}`
  + (g.cond ? `|cond:${condId(g.cond)}` : '')

// Resolve a conditional genome to the static genome effective at this
// decision point. Static genomes (no cond) return unchanged, so calibration
// against the original harness is exact.
function resolveGenome(genome, agent, snapshot) {
  if (!genome.cond) return genome
  let patch
  if (genome.cond.kind === 'round_switch') {
    const round = snapshot.round ?? 1
    patch = round >= genome.cond.at ? genome.cond.after : genome.cond.before
  } else if (genome.cond.kind === 'cash_threshold') {
    const ownCash = genome.__cash?.get(agent.actor) ?? -1
    patch = ownCash > genome.cond.threshold ? genome.cond.over : genome.cond.under
  } else if (genome.cond.kind === 'warm_target') {
    // no static patch: the accepted-bribe preference is applied inside
    // chooseForAgent via the warmTarget argument
    patch = {}
  } else {
    throw new Error(`unknown_cond_${genome.cond.kind}`)
  }
  return { ...genome, ...patch }
}

function createWorker(binary) {
  const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'pipe'] })
  let pending = null, buffer = ''
  child.stdout.on('data', (data) => {
    buffer += data.toString()
    let end
    while ((end = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, end); buffer = buffer.slice(end + 1)
      if (!line.trim() || !pending) continue
      const p = pending; pending = null
      const value = JSON.parse(line)
      if (value.ok !== true) p.reject(new Error(`worker_refused_${value.error ?? 'unknown'}`))
      else p.resolve(value)
    }
  })
  child.stderr.on('data', (d) => { child.__stderr = String(d) })
  return {
    request(value) {
      return new Promise((res, rej) => {
        pending = { resolve: res, reject: rej }
        child.stdin.write(JSON.stringify(value) + '\n')
      })
    },
    close() { child.kill('SIGTERM') },
  }
}

function chooseForAgent(genome, agent, snapshot, lastTarget, warmTarget) {
  genome = resolveGenome(genome, agent, snapshot)
  const menu = agent.allowed_actions
  const bribes = menu.filter((a) => a.action === 'bribe')
  const pickTarget = () => {
    const cashOf = (to) => {
      const row = (snapshot.agents.find((x) => x.actor === agent.actor)?.interaction_peers ?? [])
      return to
    }
    switch (RULES[genome.t]) {
      case 'richest': {
        // visible cash comes from the user content the driver assembles; here
        // we use the peers' public rows the driver passes alongside
        let best = bribes[0]
        for (const offer of bribes) {
          const c = genome.__cash?.get(offer.params.to) ?? -1
          const b = genome.__cash?.get(best.params.to) ?? -1
          if (c > b || (c === b && offer.params.to < best.params.to)) best = offer
        }
        return best
      }
      case 'poorest': {
        let best = bribes[0]
        for (const offer of bribes) {
          const c = genome.__cash?.get(offer.params.to) ?? -1
          const b = genome.__cash?.get(best.params.to) ?? -1
          if (c < b || (c === b && offer.params.to < best.params.to)) best = offer
        }
        return best
      }
      case 'max_id': return [...bribes].sort((x, y) => y.params.to - x.params.to)[0]
      case 'min_id': return [...bribes].sort((x, y) => x.params.to - y.params.to)[0]
      case 'first_slot': return bribes[0]
      case 'seeded_random': return bribes[Math.floor(hashDraw(snapshot.step, agent.actor, 'pick') * bribes.length) % bribes.length]
      case 'by_influence': {
        let best = bribes[0]
        for (const offer of bribes) {
          const c = genome.__influence?.get(offer.params.to) ?? -1
          const b = genome.__influence?.get(best.params.to) ?? -1
          if (c > b || (c === b && offer.params.to < best.params.to)) best = offer
        }
        return best
      }
    }
  }
  if (snapshot.phase === 'action' && snapshot.stage !== 'post_vote') {
    if (genome.uniform)
      return menu[Math.floor(hashDraw(snapshot.step, agent.actor, 'uniform') * menu.length) % menu.length]
    if (bribes.length) {
      agent.__bribeOps = (agent.__bribeOps ?? 0) + 1
      const allowed = genome.a === 0 || (agent.__bribeOps % (genome.a + 1) === 1)
      if (allowed) {
        let offer
        const sticky = lastTarget !== undefined && genome.k > 0
          && hashDraw(snapshot.step, agent.actor, 'stick') < genome.k / 8
          ? bribes.find((b) => b.params.to === lastTarget) : undefined
        const warm = genome.cond?.kind === 'warm_target' && warmTarget !== undefined
          ? bribes.find((b) => b.params.to === warmTarget) : undefined
        offer = sticky ?? warm ?? pickTarget()
        agent.__lastTarget = offer.params.to
        return offer
      }
    }
    const alt = menu.find((a) => a.action === ALTS[genome.alt]) ?? menu.find((a) => a.action === 'produce') ?? menu[0]
    return alt
  }
  if (snapshot.phase === 'market') {
    const sells = menu.filter((a) => a.action === 'sell')
    if (!sells.length) return menu[0]
    const units = sells.map((s) => s.params.units)
    const target = SELLS[genome.s] === 'all' ? Math.max(...units)
      : SELLS[genome.s] === 'unit' ? Math.min(...units)
        : Math.ceil(Math.max(...units) / 2)
    return sells.find((s) => s.params.units === target) ?? sells[sells.length - 1]
  }
  if (snapshot.phase === 'law') {
    if (snapshot.stage === 'post_vote') return menu.find((a) => a.action === 'pass') ?? menu[0]
    const vote = LAWS[genome.law] === 'abstain' ? 'vote_abstain' : LAWS[genome.law] === 'yes' ? 'vote_yes' : 'vote_no'
    return menu.find((a) => a.action === vote) ?? menu[0]
  }
  return menu[0]
}

export async function playGame({ worker, binary, seed, genome, agents = 100 }) {
  const own = worker ?? createWorker(binary)
  try {
    let snap = await own.request({ op: 'init', experimental: true, game_id: seed, seed, agents, epoch: 0, entry_fee: 10_000_000 })
    const lastTargets = new Array(agents).fill(undefined)
    const warmTargets = new Array(agents).fill(undefined)
    const bribeCounts = {}
    let accepted = 0, rejected = 0, skipped = 0
    let condTally = {}
    while (snap.phase !== 'finished') {
      // visible cash/influence for targeting comes from the public rows
      const cash = new Map(snap.world.public_state.map((r) => [r.actor, r.cash]))
      const influence = new Map(snap.world.public_state.map((r) => [r.actor, r.influence]))
      const g = { ...genome, __cash: cash, __influence: influence }
      const decisions = snap.required_actors.map((actor) => {
        const agent = snap.agents.find((a) => a.actor === actor)
        const choice = chooseForAgent(g, agent, snap, lastTargets[actor], warmTargets[actor])
        lastTargets[actor] = agent.__lastTarget ?? lastTargets[actor]
        return { actor, action: choice.action, params: choice.params }
      })
      const applied = await own.request({ op: 'apply', expected_step: snap.step, expected_state_hash: snap.state_hash, decisions })
      for (const r of applied.receipts) {
        if (r.skipped) skipped++
        else if (r.accepted) accepted++
        else rejected++
      }
      for (const d of decisions) if (d.action === 'bribe' && applied.receipts.find((r) => r.actor === d.actor && r.accepted && !r.skipped)) {
        bribeCounts[d.params.to] = (bribeCounts[d.params.to] ?? 0) + 1
        warmTargets[d.actor] = d.params.to
      }
      if (genome.cond) condTally[`${snap.round}:${snap.phase}`] = (condTally[`${snap.round}:${snap.phase}`] ?? 0) + 1
      snap = applied.snapshot
    }
    const total = Object.values(bribeCounts).reduce((a, b) => a + b, 0)
    const share = total ? Math.max(...Object.values(bribeCounts)) / total : 0
    const hhi = total ? Object.values(bribeCounts).reduce((a, c) => a + (c / total) ** 2, 0) : 0
    return { seed, genome: genomeId(genome), genomeRaw: genome, bribes: total,
      recipients: Object.keys(bribeCounts).length, share, hhi,
      accepted, rejected, skipped, ...(genome.cond ? { condPhases: condTally } : {}) }
  } finally { if (!worker) own.close() }
}

async function lanes(jobs, lanesCount, run) {
  let next = 0
  const results = []
  const settled = await Promise.allSettled(Array.from({ length: lanesCount }, async () => {
    for (;;) {
      const i = next++
      if (i >= jobs.length) break
      const worker = createWorker(process.env.ALASHI_WORLD_WORKER ?? 'arena/target/debug/world_worker')
      try { results.push(await run(worker, jobs[i])) }
      finally { worker.close() }
    }
  }))
  for (const r of settled) if (r.status === 'rejected') console.error('LANE ERROR:', r.reason)
  return results
}

async function main() {
  const [mode, ...rest] = process.argv.slice(2)
  const outDir = 'data/eval/evolved/world100-attack-cond-20261121'
  mkdirSync(outDir, { recursive: true })
  const TRAIN = [20261201, 20261203, 20261205]
  if (mode === 'calibrate') {
    const result = await playGame({ binary: process.env.ALASHI_WORLD_WORKER ?? 'arena/target/debug/world_worker', seed: CALIB_SEED, genome: ANCHOR_HALF_DONKEY })
    console.log(JSON.stringify(result))
  } else if (mode === 'genomes') {
    const [file] = rest
    const entries = readFileSync(file, 'utf8').trim().split('\n').map((l) => JSON.parse(l))
    const jobs = entries.flatMap((e) => e.seeds.map((seed) => ({ ...e, seed })))
    const t0 = Date.now()
    const results = await lanes(jobs, Number(process.env.LANES ?? 6), async (worker, job) => {
      const r = await playGame({ worker, seed: job.seed, genome: job.genome })
      return r
    })
    writeFileSync(join(outDir, `${file.split('/').pop()}.results.json`), JSON.stringify(results, null, 1))
    console.log(JSON.stringify({ games: results.length, seconds: Math.round((Date.now() - t0) / 1000),
      worst: results.reduce((w, r) => (r.share > (w?.share ?? 0) ? r : w), null) }))
  } else {
    console.error('modes: calibrate | genomes <file>')
    process.exit(2)
  }
}

const isDirect = process.argv[1] && process.argv[1].endsWith('world-evo-attack-cond.mjs')
if (isDirect) main().catch((e) => { console.error(e); process.exit(1) })
