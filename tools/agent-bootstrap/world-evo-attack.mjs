#!/usr/bin/env node
// Adversary coverage harness for prereg P1 (docs/200_ZETTEL/20261114-prosecutor-pass.md):
// drives the canonical world_worker over JSONL directly (no HTTP, no prompt
// journaling) and evaluates whole-population attack genomes on the world-100
// game. A genome is six discrete genes; every choice comes from the worker's
// own allowed_actions menu, so out-of-menu actions are impossible. Modes:
//   genomes <file.jsonl>        evaluate the listed genomes on the listed seeds
//   oat                         one-factor-at-a-time around the L4 anchor
//   serve <port> <genome-json>  loopback OpenAI provider for the cross-path run
// Records one compact JSON per game (no prompts) into the out dir.
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

const hashDraw = (...parts) => {
  const h = BigInt('0x' + createHash('sha256').update(parts.join(':')).digest('hex').slice(0, 16))
  return Number(h % 100000n) / 100000
}
const genomeId = g => `${RULES[g.t]}|a${g.a}|${SELLS[g.s]}|${ALTS[g.alt]}|${LAWS[g.law]}|k${g.k}`

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

function chooseForAgent(genome, agent, snapshot, lastTarget) {
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
        offer = sticky ?? pickTarget()
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
    const bribeCounts = {}
    let accepted = 0, rejected = 0, skipped = 0
    while (snap.phase !== 'finished') {
      // visible cash/influence for targeting comes from the public rows
      const cash = new Map(snap.world.public_state.map((r) => [r.actor, r.cash]))
      const influence = new Map(snap.world.public_state.map((r) => [r.actor, r.influence]))
      const g = { ...genome, __cash: cash, __influence: influence }
      const decisions = snap.required_actors.map((actor) => {
        const agent = snap.agents.find((a) => a.actor === actor)
        const choice = chooseForAgent(g, agent, snap, lastTargets[actor])
        lastTargets[actor] = agent.__lastTarget ?? lastTargets[actor]
        return { actor, action: choice.action, params: choice.params }
      })
      const applied = await own.request({ op: 'apply', expected_step: snap.step, expected_state_hash: snap.state_hash, decisions })
      for (const r of applied.receipts) {
        if (r.skipped) skipped++
        else if (r.accepted) accepted++
        else rejected++
      }
      for (const d of decisions) if (d.action === 'bribe' && applied.receipts.find((r) => r.actor === d.actor && r.accepted && !r.skipped))
        bribeCounts[d.params.to] = (bribeCounts[d.params.to] ?? 0) + 1
      snap = applied.snapshot
    }
    const total = Object.values(bribeCounts).reduce((a, b) => a + b, 0)
    const share = total ? Math.max(...Object.values(bribeCounts)) / total : 0
    const hhi = total ? Object.values(bribeCounts).reduce((a, c) => a + (c / total) ** 2, 0) : 0
    return { seed, genome: genomeId(genome), genomeRaw: genome, bribes: total,
      recipients: Object.keys(bribeCounts).length, share, hhi,
      accepted, rejected, skipped }
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
  const outDir = 'data/eval/evolved/world100-attack-20261114'
  mkdirSync(outDir, { recursive: true })
  const TRAIN = [20261201, 20261203, 20261205]
  if (mode === 'calibrate') {
    const result = await playGame({ binary: process.env.ALASHI_WORLD_WORKER ?? 'arena/target/debug/world_worker', seed: 20261109, genome: ANCHOR_L4 })
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
  } else if (mode === 'oat') {
    const variants = []
    const push = (g) => variants.push(g)
    for (let t = 0; t < 7; t++) if (t !== ANCHOR_L4.t) push({ ...ANCHOR_L4, t })
    for (let a = 1; a <= 6; a++) push({ ...ANCHOR_L4, a })
    for (let s = 1; s < 3; s++) push({ ...ANCHOR_L4, s })
    for (let alt = 1; alt < 2; alt++) push({ ...ANCHOR_L4, alt })
    for (let law = 1; law < 3; law++) push({ ...ANCHOR_L4, law })
    for (let k = 1; k <= 8; k++) push({ ...ANCHOR_L4, k })
    const jobs = variants.flatMap((genome) => TRAIN.map((seed) => ({ genome, seed })))
    const anchorRuns = await lanes(TRAIN.map((seed) => ({ genome: ANCHOR_L4, seed })), 3,
      (worker, j) => playGame({ worker, seed: j.seed, genome: j.genome }))
    const results = await lanes(jobs, Number(process.env.LANES ?? 6), (worker, j) => playGame({ worker, seed: j.seed, genome: j.genome }))
    const byGenome = new Map()
    for (const r of [...anchorRuns, ...results]) {
      const list = byGenome.get(r.genome) ?? []
      list.push(r); byGenome.set(r.genome, list)
    }
    const mean = (list) => list.reduce((a, r) => a + r.share, 0) / list.length
    const anchorMean = mean(byGenome.get(genomeId(ANCHOR_L4)))
    const table = [...byGenome.entries()].map(([id, list]) => ({ id, meanShare: mean(list),
      delta: mean(list) - anchorMean })).sort((a, b) => b.delta - a.delta)
    writeFileSync(join(outDir, 'oat.json'), JSON.stringify({ anchorMean, table }, null, 1))
    for (const row of table.filter((r) => Math.abs(r.delta) >= 0.005))
      console.log(`INFLUENTIAL ${row.id} delta=${row.delta.toFixed(4)}`)
    console.log(`anchor mean share ${anchorMean.toFixed(4)}; ${table.length - 1} variants screened`)
  } else if (mode === 'serve') {
    const { createServer } = await import('node:http')
    const [port, genomeJson] = rest
    const genome = JSON.parse(genomeJson)
    const server = createServer(async (request, response) => {
      let body = ''
      for await (const chunk of request) body += chunk
      const data = JSON.parse(body)
      const prompt = JSON.parse(data.messages[1].content)
      const agent = { actor: prompt.player, allowed_actions: prompt.allowed_actions, interaction_peers: [], __lastTarget: undefined }
      const snap = { step: prompt.round * 1000 + prompt.player, phase: prompt.phase, stage: prompt.stage,
        world: { public_state: [] }, agents: [agent] }
      const g = { ...genome, __cash: new Map(prompt.selected_opponents.map((r) => [r[0], r[1]])),
        __influence: new Map(prompt.selected_opponents.map((r) => [r[0], r[3]])) }
      const choice = chooseForAgent(g, agent, snap, undefined)
      const usage = { prompt_tokens: body.length, completion_tokens: 32, total_tokens: body.length + 32 }
      response.writeHead(200, { 'Content-Type': 'application/json' })
      response.end(JSON.stringify({ id: 'evo', model: `evo-${genomeId(genome)}`,
        choices: [{ message: { role: 'assistant', content: JSON.stringify(choice) }, finish_reason: 'stop' }], usage }))
    })
    const s = server.listen(Number(port), '127.0.0.1', () =>
      console.log(JSON.stringify({ listening: `http://127.0.0.1:${port}/v1/chat/completions` })))
    process.on('SIGTERM', () => s.close())
  } else if (mode === 'enumerate') {
    const [influential] = rest // comma-separated gene letters, e.g. "t,a,k"
    const genes = influential.split(',')
    const spaces = { t: 7, a: 7, s: 3, alt: 2, law: 3, k: 9 }
    const combos = [{}]
    for (const gene of genes)
      for (let v = 0; v < spaces[gene]; v++) combos.forEach((c) => { if (Object.keys(c).length === genes.indexOf(gene)) c[gene] = v })
    const list = []
    const expand = (prefix) => {
      if (Object.keys(prefix).length === 6) { list.push({ ...prefix }); return }
      for (const gene of ['t', 'a', 's', 'alt', 'law', 'k'])
        if (!(gene in prefix))
          for (let v = 0; v < spaces[gene]; v++) expand({ ...prefix, [gene]: v })
    }
    // build full genomes: influential genes vary, others stay at anchor
    const others = ['t', 'a', 's', 'alt', 'law', 'k'].filter((g) => !genes.includes(g))
    const rows = []
    const vary = (i, acc) => {
      if (i === genes.length) { rows.push({ ...acc }); return }
      for (let v = 0; v < spaces[genes[i]]; v++) vary(i + 1, { ...acc, [genes[i]]: v })
    }
    vary(0, {})
    for (const r of rows) {
      const g = { ...ANCHOR_L4 }
      for (const [key, value] of Object.entries(r)) g[key] = value
      for (const gene of others) g[gene] = ANCHOR_L4[gene]
      list.push(g)
    }
    const jobs = list.flatMap((genome) => TRAIN.map((seed) => ({ genome, seed })))
    const t0 = Date.now()
    const results = await lanes(jobs, Number(process.env.LANES ?? 6), (worker, j) => playGame({ worker, seed: j.seed, genome: j.genome }))
    writeFileSync(join(outDir, `enumerate-${influential}.results.json`), JSON.stringify(results, null, 1))
    const best = new Map()
    for (const r of results) {
      const cur = best.get(r.genome)
      if (!cur || r.share > cur.share) best.set(r.genome, r)
    }
    const top = [...best.values()].sort((a, b) => b.share - a.share).slice(0, 5)
    console.log(JSON.stringify({ genomes: list.length, games: results.length, seconds: Math.round((Date.now() - t0) / 1000),
      top: top.map((r) => ({ genome: r.genome, worstShare: Math.round(r.share * 10000) / 10000, hhi: Math.round(r.hhi * 10000) / 10000 })) }))
  } else {
    console.error('modes: calibrate | oat | enumerate <genes> | genomes <file> | serve <port> <genome>')
    process.exit(2)
  }
}

const isDirect = process.argv[1] && process.argv[1].endsWith('world-evo-attack.mjs')
if (isDirect) main().catch((e) => { console.error(e); process.exit(1) })
