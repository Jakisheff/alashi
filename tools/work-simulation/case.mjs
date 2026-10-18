export const caseTitle = 'TEST: доступ к книге после отмены подписки';
export const configurations = [
  { id: 'A', competence: false, context: false },
  { id: 'B', competence: false, context: true },
  { id: 'C', competence: true, context: false },
  { id: 'D', competence: true, context: true },
];

export function material(enriched) {
  return {
    label: 'SYNTHETIC TEST DATA. Fictional library; no company or employee data.',
    task: 'Добавить решение о доступе к книге. Активная подписка даёт доступ; отменённая или истёкшая сама по себе доступа не даёт. Сохранить старого клиента.',
    business_rules: enriched ? ['retain_ownership', 'honor_grace_window'] : [],
    expert_note: enriched ? 'Отдельно купленная или подаренная книга остаётся у читателя после отмены подписки и удаления книги из каталога. Подтверждённый льготный период сохраняет доступ до своего окончания.' : null,
  };
}

// This handler executes the submitted policy. It never receives a configuration ID.
export function handler(policy, input) {
  if (policy.validate_user && (typeof input.userId !== 'string' || !input.userId.trim())) return { status: 400, error: 'invalid_user' };
  if (policy.validate_state && !['active', 'cancelled', 'expired'].includes(input.subscription)) return { status: 400, error: 'invalid_subscription' };
  const allow = input.subscription === 'active'
    || policy.retain_ownership && (input.purchased === true || input.gifted === true)
    || policy.honor_grace_window && input.graceValid === true;
  return { status: 200, [policy.response_field]: Boolean(allow) };
}

export const legacyContract = { response_field: 'canRead', user: 'nonempty_string', states: ['active', 'cancelled', 'expired'] };

export function exampleTransfer() {
  const demonstrations = [
    { id: 'demo-1', signal: 'confirmed_promise', decision: 'allow' },
    { id: 'demo-2', signal: 'expired_trial', decision: 'deny' },
    { id: 'demo-3', signal: 'confirmed_promise', decision: 'allow' },
    { id: 'demo-4', signal: 'expired_trial', decision: 'deny' },
  ];
  const heldOut = [
    { id: 'new-reader-17', signal: 'confirmed_promise', expected: 'allow' },
    { id: 'new-reader-28', signal: 'expired_trial', expected: 'deny' },
    { id: 'new-reader-39', signal: 'conflicting_promises', expected: null },
  ];
  const learned = new Map();
  for (const d of demonstrations) {
    const previous = learned.get(d.signal);
    learned.set(d.signal, previous && previous !== d.decision ? 'unknown' : d.decision);
  }
  return {
    test_data: true, mechanism: 'Deterministic mapping learned from labelled action examples; not a neural model or human experiment.',
    demonstrations,
    cases: heldOut.map(c => ({ ...c, words_only: 'unknown', with_examples: learned.get(c.signal) ?? 'unknown' })),
    human_reference: 'not_collected',
    hypothesis_status: 'not_validated',
    owner_hypothesis: 'Неявное знание человека и команды может оказаться биологическим рвом: часть знания может не передаваться агентам ни словесно, ни через примеры.',
    limit: 'Неполученное объяснение и остаток неизвестного не доказывают биологическую исключительность. Здесь нет данных людей, эмоционального интеллекта или коллективного бессознательного.',
  };
}
