import { useState } from 'react'

type Receipt = { round: number; phase: string; action: string; ok: boolean; err: string | null; cash_after: number | null; goods_after: number | null; detail: Record<string, unknown> }
type Participant = { actor: number; cash: number; goods: number; payout: number; actions: Receipt[] }
const money = (value: number) => (value / 1_000_000).toLocaleString('ru-RU', { maximumFractionDigits: 2 })

export function ReasoningHelper({ game }: { game: { seed: number; seat: number; participants: Participant[] } }) {
  const [subject, setSubject] = useState('self'), [direction, setDirection] = useState('general'), [step, setStep] = useState(-1)
  const [goal, setGoal] = useState('Получить большую выплату по итогам партии')
  const [plan, setPlan] = useState('Превращать произведённый товар в казну через продажи')
  const participant = game.participants.find(p => p.actor === (subject === 'self' ? game.seat : Number(subject))) ?? game.participants[game.seat]
  const action = participant.actions[step] ?? participant.actions.find(a => a.action === 'sell' && a.ok) ?? participant.actions[0]
  const actionText = `Раунд ${action.round}, ${action.phase}: ${action.action}${typeof action.detail.units === 'number' ? ` · ${action.detail.units} товара` : ''}. ${action.ok ? 'Движок принял ход.' : `Движок отклонил ход: ${action.err}.`}`
  const effect = `После хода: казна ${action.cash_after === null ? 'не указана' : money(action.cash_after)}, товар ${action.goods_after ?? 'не указан'}. Итоговая выплата за всю партию: ${money(participant.payout)}. Вклад одного хода в выплату отдельно не установлен.`
  const steps = [
    { name: 'Цель', status: 'Гипотеза для обсуждения', text: goal },
    { name: 'План', status: 'Гипотеза для обсуждения', text: plan },
    { name: 'Конкретный ход', status: 'Квитанция движка', text: actionText },
    { name: 'Наблюдаемый результат', status: 'Квитанция и итог партии', text: effect },
  ]
  const ordered = direction === 'general' ? steps : [...steps].reverse()
  return <section className="work-evidence work-reasoning" aria-labelledby="reasoning-title">
    <span className="work-eyebrow">ПОМОЩНИК ПОНИМАНИЯ · TEST</span><h2 id="reasoning-title">Понять ход коллеги или соперника</h2>
    <p>Один эпизод можно разбирать начиная с общей цели или с конкретного наблюдения. Выберите удобный маршрут. Человек может менять его в зависимости от задачи; направление разбора не является оценкой личности.</p>
    <div className="work-helper-controls">
      <label>Чей ход рассматриваем<select value={subject} onChange={e => { setSubject(e.target.value); setStep(-1) }}><option value="self">Своя фракция · разбор с коллегой</option>{game.participants.filter(p => p.actor !== game.seat).map(p => <option key={p.actor} value={p.actor}>Соперник · фракция {p.actor + 1}</option>)}</select></label>
      <label>Эпизод · seed {game.seed}<select value={participant.actions.indexOf(action)} onChange={e => setStep(Number(e.target.value))}>{participant.actions.map((a, i) => <option key={i} value={i}>Раунд {a.round} · {a.phase} · {a.action}</option>)}</select></label>
      <label>Предполагаемая цель<input value={goal} onChange={e => setGoal(e.target.value)} maxLength={300} /></label>
      <label>Предполагаемый план<input value={plan} onChange={e => setPlan(e.target.value)} maxLength={300} /></label>
    </div>
    <div className="work-direction" role="group" aria-label="Направление разбора"><button aria-pressed={direction === 'general'} onClick={() => setDirection('general')}>От общего к частному</button><button aria-pressed={direction === 'detail'} onClick={() => setDirection('detail')}>От частного к общему</button></div>
    <ol className="work-reasoning-path">{ordered.map(s => <li key={s.name}><small>{s.status}</small><h3>{s.name}</h3><p>{s.text}</p></li>)}</ol>
    <div className="work-probe"><h3>Место встречи двух объяснений</h3><p>Свяжите выбранную цель с этим конкретным ходом. Затем договоритесь, какое наблюдение подтвердит связь. Игровой критерий для проверки: выплата в сопоставимых партиях; казна помогает разобрать промежуточный эффект.</p>
      <p className="work-bridge">{direction === 'general' ? `«Я предлагаю цель: ${goal}. Возьмём конкретный ход: ${action.action} в раунде ${action.round}. Как он помогает этой цели? Какой результат проверим вместе?»` : `«Я вижу ход: ${action.action} в раунде ${action.round}. Что ты хотел им получить? Я предполагаю цель: ${goal}. Правильно ли я связал этот эпизод с общей задачей?»`}</p>
      <p className="work-muted">О намерениях соперника здесь нет данных. Цель и план введены вами как гипотезы. Смена маршрута переставляет объяснение, игровые действия и результаты сохраняются. Эта TEST-проверка подтверждает работу помощника; улучшение общения людей ещё не проверено.</p>
    </div>
  </section>
}
