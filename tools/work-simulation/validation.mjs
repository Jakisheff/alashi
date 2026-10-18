import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { mean } from './grade.mjs';

export const conditionsFor = protocol => protocol.families.flatMap(family => family.seeds.flatMap(seed => protocol.seats.map(seat => ({
  key: `${family.id}:${seed}:${seat}`, family: family.id, seed, seat, opponents: family.opponents,
  epoch: protocol.epoch, vote_weight_mode: family.vote_weight_mode,
}))));

const identity = g => [g.seed, g.seat, g.opponents, g.epoch, g.vote_weight_mode];
const settlement = g => [g.cash, g.goods, g.payout, g.rank, g.payouts, g.bank, g.rake, g.laws];
export function pairedGames(before, after, conditions) {
  for (const games of [before, after]) {
    assert.equal(games.length, conditions.length, 'missing_validation_games');
    assert.equal(new Set(games.map(g => g.key)).size, conditions.length, 'duplicate_validation_game');
  }
  return conditions.map(condition => {
    const old = before.find(g => g.key === condition.key), next = after.find(g => g.key === condition.key);
    assert.ok(old && next, 'missing_validation_pair');
    assert.deepEqual(identity(old), identity(condition), 'condition_changed');
    assert.deepEqual(identity(next), identity(condition), 'condition_changed');
    assert.deepEqual(old.initial, next.initial, 'unequal_initial_resources');
    assert.ok(old.decisions.length && next.decisions.length, 'missing_decision_trace');
    return { key: condition.key, family: condition.family, cash_delta: next.cash - old.cash, payout_delta: next.payout - old.payout,
      same_decisions: isDeepStrictEqual(old.decisions, next.decisions), same_actions: isDeepStrictEqual(old.actions, next.actions),
      same_settlement: isDeepStrictEqual(settlement(old), settlement(next)), invalid_actions: old.invalid_actions + next.invalid_actions };
  });
}

export function validationResult(runs, challenge, protocol) {
  const conditions = conditionsFor(protocol);
  assert.equal(runs.length, 6);
  assert.equal(new Set(runs.map(r => r.id)).size, 6);
  const find = id => { const run = runs.find(r => r.id === id); assert.ok(run, 'missing_controller'); return run; };
  const pairs = [], summaries = [];
  for (const context of [false, true]) {
    const baseline = find(context ? 'B' : 'A'), role = find(context ? 'D' : 'C'), rule = find(context ? 'F' : 'E');
    assert.ok([baseline, role, rule].every(r => r.context === context), 'unequal_context');
    const equality = pairedGames(role.games, rule.games, conditions);
    pairs.push(...equality.map(p => ({ ...p, context, role: role.id, rule: rule.id })));
    const effect = pairedGames(baseline.games, rule.games, conditions);
    for (const family of protocol.families) {
      const select = run => run.games.filter(g => g.family === family.id);
      const group = effect.filter(p => p.family === family.id);
      summaries.push({ family: family.id, label: family.label, context, cases: group.length,
        baseline_payout: mean(select(baseline).map(g => g.payout)), role_payout: mean(select(role).map(g => g.payout)), rule_payout: mean(select(rule).map(g => g.payout)),
        mean_payout_delta: mean(group.map(p => p.payout_delta)), mean_cash_delta: mean(group.map(p => p.cash_delta)),
        payout_wins: group.filter(p => p.payout_delta > 0).length, payout_losses: group.filter(p => p.payout_delta < 0).length,
        payout_ties: group.filter(p => p.payout_delta === 0).length });
    }
  }
  const challengePairs = pairedGames(find('E').games, challenge.games, conditions);
  const legal = runs.every(r => r.games.every(g => g.invalid_actions === 0)) && challenge.games.every(g => g.invalid_actions === 0);
  const exact = pairs.filter(p => p.same_decisions && p.same_actions && p.same_settlement && !p.invalid_actions).length;
  const equivalent = legal && exact === conditions.length * 2;
  const payoutWitness = challengePairs.find(p => p.payout_delta > 0);
  const treasuryWitness = challengePairs.find(p => p.cash_delta > 0);
  const baseline = find('A'), direct = find('E');
  const effectKey = pairedGames(baseline.games, direct.games, conditions).find(p => p.payout_delta > 0)?.key;
  let mechanism = null;
  if (effectKey) {
    const old = baseline.games.find(g => g.key === effectKey), next = direct.games.find(g => g.key === effectKey);
    const index = old.decisions.findIndex((d, i) => !isDeepStrictEqual(d.action, next.decisions[i]?.action));
    if (index >= 0) {
      const before = old.decisions[index], after = next.decisions[index];
      const receipt = (game, decision) => game.actions.find(a => a.round === decision.observation.round && a.phase === decision.phase && a.action === decision.action.action);
      mechanism = { key: effectKey, same_observation: isDeepStrictEqual(before.observation, after.observation),
        before, after, baseline_receipt: receipt(old, before), rule_receipt: receipt(next, after),
        baseline_payout: old.payout, rule_payout: next.payout,
        limit: 'Показано первое расхождение политик. Итоговая разница выплаты относится ко всей последовательности решений и реакций соперников, а не к изолированному эффекту одного хода.' };
    }
  }
  return { status: !legal ? 'invalid_actions' : equivalent ? 'rule_preserves_role' : 'equivalence_rejected',
    primary_metric: protocol.primary_metric, conditions: conditions.length, role_rule_pairs: pairs.length, exact_matches: exact,
    claim: equivalent ? 'Прямое правило сохранило все решения и выплаты планировщика в новых TEST-условиях.' : 'Точное совпадение не подтверждено. Требуется разобрать различия до вывода о роли.',
    hiring_decision: equivalent ? 'Для исполнения этой зафиксированной политики отдельные задачи планировщика не нужны. Оснований нанимать отдельного сотрудника по этому TEST нет.' : 'Данные не позволяют сделать вывод о найме.',
    interpretation: 'В обоих вариантах реализована одна стратегия продажи. Новые условия проверяют её перенос и затраты исполнения; адаптивное суждение эксперта здесь отсутствует.',
    cost: { role_tool_calls: find('C').tool_calls, rule_tool_calls: find('E').tool_calls,
      saved_calls_per_policy_setup: find('C').tool_calls - find('E').tool_calls, model_calls: 0, human_cost: 'not_measured',
      scope: 'Дополнительные вызовы относятся к однократной подготовке политики, а не к каждой партии. Время локальных процессов не является ценой сотрудника.' },
    summaries, pairs, mechanism,
    opponent_receipts: protocol.families.map(family => {
      const games = direct.games.filter(g => g.family === family.id);
      const errors = games.flatMap(g => g.participants.filter(p => p.actor !== g.seat).flatMap(p => p.actions.filter(a => !a.ok)));
      return { family: family.id, label: family.label, controller: 'E', games: games.length, rejected_actions: errors.length,
        reasons: [...new Set(errors.map(a => `${a.action}: ${a.err}`))] };
    }),
    challenge: { label: 'Пропустить Market второго раунда, затем снова продавать весь запас', policy: challenge.policy,
      payout_wins: challengePairs.filter(p => p.payout_delta > 0).length, payout_losses: challengePairs.filter(p => p.payout_delta < 0).length,
      payout_ties: challengePairs.filter(p => p.payout_delta === 0).length,
      mean_payout_delta: mean(challengePairs.map(p => p.payout_delta)), mean_cash_delta: mean(challengePairs.map(p => p.cash_delta)),
      witness_key: (payoutWitness ?? treasuryWitness)?.key ?? null,
      conclusion: payoutWitness ? 'Найден случай с большей выплатой после отсрочки. Немедленная продажа не оптимальна по выплате во всех проверенных условиях. Альтернатива тоже является правилом.' : treasuryWitness ? 'Отсрочка иногда повышает казну, но преимущества по выплате не найдено. Оптимальность немедленной продажи не установлена.' : 'В этой серии отсрочка не дала преимущества по выплате или казне. Проверка одной альтернативы не доказывает оптимальность правила.',
      pairs: challengePairs, games: challenge.games },
    next_hiring_evidence: 'Для решения о найме нужен рабочий эпизод, в котором действующее правило не достигает заранее выбранного результата, а дополнительный вклад устойчиво устраняет разрыв с учётом затрат. Сам по себе рост казны такой потребности не устанавливает.',
    human_transfer: 'not_validated', biological_moat: 'hypothesis', runs };
}
