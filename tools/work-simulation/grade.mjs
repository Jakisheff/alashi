import { handler } from './case.mjs';

// Acceptance cases are separate from worker inputs and do not inspect A/B/C/D.
const checks = [
  ['active', 'Активная подписка', 'feature', false, { subscription: 'active' }, true],
  ['cancelled', 'Отмена без отдельной покупки', 'feature', false, { subscription: 'cancelled' }, false],
  ['expired', 'Истечение без отдельной покупки', 'feature', false, { subscription: 'expired' }, false],
  ['owned', 'Покупка сохраняется после отмены', 'context', false, { subscription: 'cancelled', purchased: true }, true],
  ['delisted', 'Купленная книга удалена из каталога', 'context', false, { subscription: 'cancelled', purchased: true, delisted: true }, true],
  ['grace', 'Подтверждённый льготный период', 'context', false, { subscription: 'expired', graceValid: true }, true],
  ['legacy', 'Старый клиент читает canRead', 'competence', false, { subscription: 'active' }, 'legacy'],
  ['missing-user', 'Запрос без пользователя', 'competence', false, { subscription: 'active', userId: null }, 400],
  ['unknown-state', 'Неизвестное состояние подписки', 'competence', false, { subscription: 'unknown' }, 400],
  ['new-reader', 'Другой активный читатель', 'feature', true, { subscription: 'active', userId: 'TEST-new-reader' }, true],
  ['gift', 'Подаренная книга после отмены', 'context', true, { subscription: 'cancelled', gifted: true }, true],
  ['blank-user', 'Пустой идентификатор пользователя', 'competence', true, { subscription: 'active', userId: '   ' }, 400],
];

export function grade(policy) {
  return checks.map(([id, title, category, held_out, fields, expected]) => {
    const actual = handler(policy, { userId: 'TEST-reader', ...fields });
    const observed = expected === 'legacy' ? typeof actual.canRead === 'boolean' : typeof expected === 'number' ? actual.status : actual.canRead ?? actual.allowed;
    return { id, title, category, held_out, passed: observed === (expected === 'legacy' ? true : expected), expected, observed, response: actual };
  });
}

export function hiringProfile(runs) {
  const find = (competence, context) => runs.find(r => r.competence === competence && r.context === context);
  const cells = [find(false, false), find(false, true), find(true, false), find(true, true)];
  if (runs.length !== 4 || cells.some(r => !r || r.status !== 'completed')) return { status: 'insufficient_data', claim: 'Нужны четыре завершённые конфигурации.', competence_gains: [], context_gains: [], skills: [] };
  const gains = (before, after, category) => before.checks.filter(c => c.category === category && !c.passed && after.checks.find(a => a.id === c.id)?.passed).map(c => c.id);
  const [a, b, c, d] = cells;
  const competence_gains = gains(a, c, 'competence').filter(id => gains(b, d, 'competence').includes(id));
  const context_gains = gains(a, b, 'context').filter(id => gains(c, d, 'context').includes(id));
  const regressions = [[a, c], [b, d], [a, b], [c, d]].flatMap(([before, after]) => before.checks.filter(check => check.passed && after.checks.find(other => other.id === check.id)?.passed === false).map(check => `${before.id}→${after.id}: ${check.id}`));
  return {
    status: regressions.length ? 'mixed_test_evidence' : 'test_hypothesis',
    claim: regressions.length ? 'Есть регрессии. Преимущество конфигурации требует разбора.' : competence_gains.length ? 'В TEST-кейсе полезна компетенция проверки API-контрактов и входных данных.' : 'TEST-сравнение не дало основания добавлять эту компетенцию.',
    competence_gains, context_gains, regressions,
    skills: competence_gains.length && !regressions.length ? ['Проверять изменение на старом клиенте.', 'Проверять недопустимые входные данные.', 'Передавать воспроизводимые контрактные проверки следующему участнику.'] : [],
    context_action: context_gains.length ? 'Передать правила владения книгами и льготного периода; проверить применение на новых случаях.' : 'Преимущество дополненного контекста не установлено.',
    hiring_decision: 'Не установлено, нужен ли найм: сравнить обучение действующей команды с пробной работой кандидата и учесть затраты людей.',
    evidence: [`${c.id} против ${a.id} и ${d.id} против ${b.id}: вклад проверки контракта.`, `${b.id} против ${a.id} и ${d.id} против ${c.id}: вклад бизнес-контекста.`],
    human_transfer: 'not_validated', human_cost: 'not_measured',
  };
}
