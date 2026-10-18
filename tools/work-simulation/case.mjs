export const caseTitle = 'TEST: команда фракции в экономике alashi';
export const configurations = [
  { id: 'A', competence: false, context: false },
  { id: 'B', competence: false, context: true },
  { id: 'C', competence: true, context: false },
  { id: 'D', competence: true, context: true },
];
export const scenarios = [11, 29, 47, 83].flatMap(seed => [0, 1, 2].map(seat => ({
  key: `${seed}:${seat}`, seed, seat, opponents: ['random', 'random'], epoch: 0, vote_weight_mode: 0,
})));

export function material(enriched) {
  return {
    label: 'SYNTHETIC TEST. Existing alashi rules, scripted faction controllers; no people or company data.',
    goal: 'Превращать произведённый товар в игровую казну за шесть раундов. Соблюдать фазы и проверять итоговый расчёт.',
    public_context: enriched,
    law_guidance: enriched ? 'Использовать публичную карту закона: против налогов и эмбарго. Президент может наложить вето. Это заданная TEST-политика, не универсально лучшая стратегия.' : null,
  };
}

export function observationFor(policy, phase, observation) {
  const o = observation;
  const own = { phase, round: o.round, my_idx: o.my_idx, n_factions: o.n_factions,
    cash: o.cash[o.my_idx], goods: o.goods[o.my_idx], decision_stage: o.decision_stage };
  return policy.public_context ? { ...own, law_card: o.law_card, is_president: o.president === o.my_wallet, veto_pending: o.veto_pending } : own;
}

// These are declared TEST policies. Only the Rust game engine computes consequences.
export function decide(policy, phase, observation) {
  const o = observationFor(policy, phase, observation);
  if (phase === 'market') return o.goods > 0 ? { action: 'sell', units: policy.inventory_review ? o.goods : (policy.sale_units ?? 1) } : { action: 'pass' };
  if (phase === 'action') return { action: o.round < 6 ? 'produce' : 'pass' };
  if (phase !== 'law') throw new Error('unknown_game_phase');
  const adverse = [1, 2, 6].includes(o.law_card);
  if (o.decision_stage === 'post_vote') return { action: adverse && o.is_president && !o.veto_pending ? 'veto' : 'pass' };
  return { action: adverse ? 'vote_no' : 'vote_yes' };
}

export function exampleTransfer() {
  const demonstrations = [
    { id: 'demo-1', signal: 'ally_reserved_market', decision: 'wait' },
    { id: 'demo-2', signal: 'no_commitment', decision: 'sell' },
    { id: 'demo-3', signal: 'ally_reserved_market', decision: 'wait' },
    { id: 'demo-4', signal: 'no_commitment', decision: 'sell' },
  ];
  const heldOut = [
    { id: 'new-round-17', signal: 'ally_reserved_market', expected: 'wait' },
    { id: 'new-round-28', signal: 'no_commitment', expected: 'sell' },
    { id: 'new-round-39', signal: 'conflicting_commitments', expected: null },
  ];
  const learned = new Map();
  for (const d of demonstrations) {
    const previous = learned.get(d.signal);
    learned.set(d.signal, previous && previous !== d.decision ? 'unknown' : d.decision);
  }
  return { test_data: true, mechanism: 'Synthetic team convention mapping; separate from game rules and match results. Not a neural or human experiment.',
    demonstrations, cases: heldOut.map(c => ({ ...c, words_only: 'unknown', with_examples: learned.get(c.signal) ?? 'unknown' })),
    human_reference: 'not_collected', hypothesis_status: 'not_validated',
    owner_hypothesis: 'Неявное знание человека и команды может оказаться биологическим рвом: часть знания может не передаваться агентам ни словесно, ни через примеры.',
    limit: 'Неполученное объяснение и остаток неизвестного не доказывают биологическую исключительность. Здесь нет данных людей, эмоционального интеллекта или коллективного бессознательного.' };
}
