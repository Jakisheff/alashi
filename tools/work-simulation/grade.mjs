import { scenarios } from './case.mjs';
import { play } from './game.mjs';
import { createHash } from 'node:crypto';

export const mean = values => values.reduce((a, b) => a + b, 0) / values.length;
export async function grade(policy) {
  const games = [];
  for (const scenario of scenarios) {
    const game = await play(policy, scenario);
    games.push({ ...game, decision_trace_sha256: createHash('sha256').update(JSON.stringify(game.decisions)).digest('hex'), decisions: games.length === 0 ? game.decisions : [] });
  }
  return games;
}

export function hiringProfile(runs) {
  const find = (competence, context) => runs.find(r => r.competence === competence && r.context === context);
  const cells = [find(false, false), find(false, true), find(true, false), find(true, true)];
  const empty = { status: 'insufficient_data', claim: 'Нужны четыре завершённые конфигурации с одинаковыми условиями партий.', comparisons: [], skills: [], context_action: 'Контекст пока не оценён.', hiring_decision: 'Не установлено, нужен ли найм.', human_transfer: 'not_validated', human_cost: 'not_measured' };
  if (runs.length !== 4 || cells.some(r => !r || r.status !== 'completed' || r.games?.length !== scenarios.length)) return empty;
  const [a, b, c, d] = cells;
  const comparisons = [];
  for (const [before, after, factor] of [[a, c, 'competence'], [b, d, 'competence'], [a, b, 'context'], [c, d, 'context']]) {
    const pairs = before.games.map(old => {
      const next = after.games.find(g => g.key === old.key);
      if (!next || JSON.stringify([old.seed, old.seat, old.opponents, old.epoch, old.vote_weight_mode, old.initial]) !== JSON.stringify([next.seed, next.seat, next.opponents, next.epoch, next.vote_weight_mode, next.initial])) return null;
      return { key: old.key, cash_delta: next.cash - old.cash, payout_delta: next.payout - old.payout, invalid_delta: next.invalid_actions - old.invalid_actions };
    });
    if (pairs.some(p => !p) || new Set(pairs.map(p => p.key)).size !== scenarios.length) return empty;
    comparisons.push({ before: before.id, after: after.id, factor, mean_cash_delta: mean(pairs.map(p => p.cash_delta)),
      mean_payout_delta: mean(pairs.map(p => p.payout_delta)), positive: pairs.filter(p => p.cash_delta > 0).length,
      negative: pairs.filter(p => p.cash_delta < 0).length, tied: pairs.filter(p => p.cash_delta === 0).length, pairs });
  }
  const legal = cells.every(r => r.games.every(g => g.invalid_actions === 0));
  const inventory = comparisons.filter(c => c.factor === 'competence');
  const useful = legal && inventory.every(c => c.mean_cash_delta >= 0) && inventory.some(c => c.mean_cash_delta > 0);
  return { status: !legal ? 'invalid_actions' : useful ? 'game_test_hypothesis' : 'no_competence_advantage',
    claim: !legal ? 'Есть недопустимые игровые действия. Профиль требует разбора.' : useful ? 'В этой TEST-серии полезно планирование объёма продаж по доступному запасу.' : 'Эта TEST-серия не установила преимущества добавленного планирования.',
    comparisons,
    skills: useful ? ['Связывать произведённый запас с объёмом продажи в Market.', 'Проверять остатки к последнему рынку и законность действий.'] : [],
    context_action: 'Передавать публичную карту закона и право вето. Проверять эффект отдельно: выгода не гарантирована в каждой партии.',
    hiring_decision: 'Проверить этот вклад на других соперниках, затем сравнить обучение команды с пробной работой кандидата. Необходимость найма и затраты людей не установлены.',
    human_transfer: 'not_validated', human_cost: 'not_measured' };
}
