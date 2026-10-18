import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { decide, directRule, observationFor } from './case.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const gameBinary = () => process.env.ALASHI_EVALGAME_BIN || resolve(process.env.CARGO_TARGET_DIR || resolve(root, 'arena/target'), 'debug/evalgame');

export async function play(policy, scenario) {
  const child = spawn(gameBinary(), [], { cwd: root, stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '', bytes = 0, game, failure;
  const decisions = [];
  const timeout = setTimeout(() => { failure = new Error('game_timeout'); child.kill(); }, 10000);
  child.stderr.on('data', part => { if (stderr.length < 4096) stderr += part; });
  child.stdin.on('error', error => { failure ??= error; });
  const finished = new Promise((yes, no) => { child.once('error', no); child.once('close', code => code === 0 ? yes() : no(failure || new Error(`evalgame_exit_${code}: ${stderr.slice(0, 1000)}`))); });
  finished.catch(() => {});
  const lines = createInterface({ input: child.stdout });
  child.stdin.write(JSON.stringify({ seed: scenario.seed, seat: scenario.seat, opponents: scenario.opponents, epoch: scenario.epoch, vote_weight_mode: scenario.vote_weight_mode }) + '\n');
  try {
    for await (const line of lines) {
      bytes += line.length; if (bytes > 2_000_000) throw new Error('game_output_limit');
      const message = JSON.parse(line);
      if (message.type === 'request') {
        const action = (policy.direct_sale_rule ? directRule : decide)(policy, message.phase, message.observation);
        decisions.push({ phase: message.phase, observation: observationFor(policy, message.phase, message.observation), action });
        child.stdin.write(JSON.stringify(action) + '\n');
      } else if (message.type === 'decision') {
        if (message.parse_error) throw new Error('invalid_game_decision');
      } else if (message.type === 'result') game = message.game;
      else throw new Error('unknown_game_message');
    }
    await finished; if (failure) throw failure;
    assert.ok(game, 'missing native game result');
    assert.equal(game.seed, scenario.seed); assert.equal(game.epoch, scenario.epoch); assert.equal(game.n_factions, 3);
    assert.equal(game.vote_weight_mode, scenario.vote_weight_mode);
    assert.deepEqual(game.phases.map(p => `${p.round}:${p.phase}`), [1, 2, 3, 4, 5, 6].flatMap(round => ['market', 'action', 'law'].map(phase => `${round}:${phase}`)));
    assert.equal(game.payouts.reduce((a, b) => a + b, 0) + game.rake, game.bank, 'native settlement conservation');
    const own = game.phases.flatMap(p => p.actions.filter(a => a.actor === scenario.seat).map(a => ({ round: p.round, phase: p.phase, ...a })));
    const invalid = own.filter(a => !a.ok);
    return { ...scenario, initial: { cash: decisions[0].observation.cash, goods: decisions[0].observation.goods }, cash: game.final_cash[scenario.seat], goods: game.final_goods[scenario.seat],
      payout: game.payouts[scenario.seat], rank: game.ranks.indexOf(scenario.seat) + 1,
      bank: game.bank, rake: game.rake, payouts: game.payouts, invalid_actions: invalid.length,
      produced: own.filter(a => a.action === 'produce' && a.ok).length,
      sales: own.filter(a => a.action === 'sell' && a.ok).length,
      participants: game.final_cash.map((cash, actor) => ({ actor, cash, goods: game.final_goods[actor], payout: game.payouts[actor],
        actions: game.phases.flatMap(p => p.actions.filter(a => a.actor === actor).map(a => ({ round: p.round, phase: p.phase, ...a }))) })),
      decisions, actions: own,
      laws: game.phases.filter(p => p.phase === 'law').map(p => ({ round: p.round, passed: p.law_passed, vetoed: p.vetoed, yes: p.yes_influence, no: p.no_influence })) };
  } finally {
    clearTimeout(timeout); lines.close(); child.stdin.destroy();
    if (child.exitCode === null) child.kill();
    await finished.catch(() => {});
  }
}
