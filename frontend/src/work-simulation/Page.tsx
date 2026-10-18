import { useState } from 'react'
import saved from './report.json'
import './style.css'

type Report = typeof saved
const local = import.meta.env.DEV && !!import.meta.env.WORK_SIMULATION_LOCAL
const passCount = (checks: Report['runs'][number]['checks'], category: string) => checks.filter(c => c.category === category && c.passed).length

export function WorkSimulationPage() {
  const [report, setReport] = useState(saved), [selected, setSelected] = useState('D'), [busy, setBusy] = useState(false), [error, setError] = useState('')
  const run = report.runs.find(r => r.id === selected) ?? report.runs[0]
  const words = report.knowledge_probe.cases.filter(c => c.words_only === c.expected).length
  const examples = report.knowledge_probe.cases.filter(c => c.expected !== null && c.with_examples === c.expected).length
  async function repeat() {
    if (busy) return
    setBusy(true); setError('')
    try {
      const response = await fetch('/work-simulation-api/run', { method: 'POST', signal: AbortSignal.timeout(30000) })
      if (!response.ok) throw new Error('test_failed')
      const next: Report = await response.json()
      if (next.schema !== saved.schema || next.test_data !== true || next.runs.length !== 4) throw new Error('invalid_test_receipt')
      setReport(next)
    } catch { setError('TEST-прогон не завершён. Сохранённый результат остаётся доступен. Проверьте локальный сервис и повторите запуск.') }
    finally { setBusy(false) }
  }
  return <main className="work-page">
    <div className="work-shell">
      <header className="work-header"><a href="/">ALASHI</a><span>РАБОЧАЯ СИМУЛЯЦИЯ</span><a href="/strategies">Арена агентов ↗</a></header>
      <div className="work-test-banner"><b>TEST · СИНТЕТИЧЕСКИЕ ДАННЫЕ</b><span>Вымышленная библиотека. Скриптовые исполнители. Людей и реальные LLM не тестировали.</span></div>
      <section className="work-intro">
        <div><p className="work-eyebrow">ПРОВЕРКА РЕШЕНИЯ О НАЙМЕ</p><h1>Какого вклада<br />не хватает команде?</h1><p>Одна рабочая задача. Четыре конфигурации.<br />Проверяем компетенцию отдельно от доступного контекста.</p></div>
        <div className="work-start"><button onClick={() => void repeat()} disabled={busy || !local}>{busy ? 'Исполняем TEST-задачи…' : 'Повторить TEST-прогон'}</button><small>{local ? 'Локальные инструменты · платных вызовов нет' : 'Показан сохранённый TEST-прогон. Повтор доступен в локальном режиме.'}</small><small>Последний результат: {new Date(report.generated_at).toLocaleString('ru-RU', { timeZone: 'Asia/Almaty' })} · Астана</small></div>
      </section>
      {error && <p role="alert" className="work-error">{error}</p>}
      <ol className="work-map" aria-label="Весь путь проверки"><li><b>01</b> Рабочий кейс</li><li><b>02</b> Четыре конфигурации</li><li><b>03</b> Проверки результата</li><li><b>04</b> Профиль для проверки</li></ol>
      <section className="work-case"><span className="work-eyebrow">КЕЙС · TEST</span><h2>Доступ к книге после отмены подписки</h2><p>Нужно изменить сервис доступа и сохранить старого клиента. В дополненном контексте есть правила отдельных покупок и льготного периода.</p><p className="work-muted">Добавленная компетенция: исполнитель проверяет API на старом клиенте и исправляет обнаруженные нарушения. Это заданное поведение TEST-инструмента.</p></section>
      <div className="work-comparison">
        <section aria-labelledby="matrix-title"><div className="work-section-heading"><h2 id="matrix-title">Четыре конфигурации</h2><span>Выберите ячейку, чтобы увидеть основания</span></div>
          <div className="work-matrix">{report.runs.map(r => <button key={r.id} className={`work-cell ${r.id === selected ? 'is-selected' : ''}`} aria-pressed={r.id === selected} onClick={() => setSelected(r.id)}>
            <div className="work-cell-top"><b>{r.id}</b><span>{r.context ? 'Дополненный контекст' : 'Исходный контекст'}</span></div><h3>{r.competence ? 'С проверкой контракта' : 'Текущий исполнитель'}</h3><strong>{r.passed}<small> / {r.total}</small></strong><p>пройденных проверок</p><div className="work-cell-foot"><span>Отложенные: {r.held_out_passed}/{r.held_out_total}</span><span>{r.tool_calls} действий</span></div>
          </button>)}</div>
          <p className="work-muted">Контрасты заданы специально для проверки механики. Эти числа не оценивают сотрудников и не прогнозируют найм.</p>
        </section>
        <aside className="work-profile" aria-labelledby="profile-title"><span className="work-eyebrow">ВЫВОД ИЗ ЭТОГО TEST-КЕЙСА</span><h2 id="profile-title">Профиль для проверки</h2><p>{report.profile.claim}</p>
          <div className="work-effects"><div><strong>+{report.profile.competence_gains.length}</strong><span>проверки за счёт компетенции</span></div><div><strong>+{report.profile.context_gains.length}</strong><span>проверки за счёт контекста</span></div></div>
          <h3>Что должен уметь участник</h3><ul>{report.profile.skills.map(s => <li key={s}>{s}</li>)}</ul><h3>Что передать команде</h3><p>{report.profile.context_action}</p><div className="work-profile-limit"><b>Решение о найме ещё не получено</b><p>{report.profile.hiring_decision}</p></div>
        </aside>
      </div>
      <section className="work-evidence" aria-labelledby="evidence-title"><div className="work-section-heading"><h2 id="evidence-title">Основания: конфигурация {run.id}</h2><span>{run.competence ? 'Контрактная проверка включена' : 'Без контрактной проверки'} · {run.context ? 'С бизнес-правилами' : 'Исходное описание'}</span></div>
        <div className="work-categories">{[['feature', 'Основная функция'], ['context', 'Бизнес-контекст'], ['competence', 'Контракт и входные данные']].map(([category, title]) => <div key={category}><b>{passCount(run.checks, category)}/{run.checks.filter(c => c.category === category).length}</b><span>{title}</span></div>)}</div>
        <div className="work-table-wrap"><table><thead><tr><th>Проверка</th><th>Набор</th><th>Результат</th></tr></thead><tbody>{run.checks.map(c => <tr key={c.id}><td>{c.title}</td><td>{c.held_out ? 'Отложенный TEST' : 'Основной TEST'}</td><td className={c.passed ? 'work-pass' : 'work-fail'}>{c.passed ? '✓ Пройдена' : '× Не пройдена'}</td></tr>)}</tbody></table></div>
        <details><summary>Условия сравнения и исполнившиеся задачи</summary><p>C против A и D против B показывают вклад контрактной проверки. B против A и D против C показывают вклад бизнес-контекста. У всех один набор приёмки.</p><ol>{run.plan.map(t => <li key={t.id}><code>{t.id}</code> · {t.depends_on.length ? `после ${t.depends_on.join(', ')}` : 'может начаться сразу'}</li>)}</ol><p>{run.tool_calls} вызовов локальных инструментов. {run.model_calls} LLM-вызовов. Повторных вызовов при открытии завершённого журнала: {run.journal_resume_repeated_calls}. Трудозатраты людей не измерены.</p><pre>{JSON.stringify({ policy: run.policy, events: run.events }, null, 2)}</pre></details>
      </section>
      <section className="work-knowledge" aria-labelledby="knowledge-title"><div><span className="work-eyebrow">ГИПОТЕЗА АМИРА · НЕ ПОДТВЕРЖДЕНА</span><h2 id="knowledge-title">«Биологический ров»<br />и то, что не сказано вслух</h2><p>{report.knowledge_probe.owner_hypothesis}</p><p>В прототипе различаем непереданное словесное правило и возможность учиться по примерам действий. Остаток неизвестного сохраняем для исследования.</p></div>
        <div className="work-probe"><h3>Проба передачи через действия · TEST</h3><p>Вымышленные решения о доступе по обещанию поддержки. Таблица соответствий строится по четырём размеченным демонстрациям. Проверка использует три новых случая.</p><div className="work-probe-scores"><div><b>{words}/3</b><span>Решены по общему описанию</span></div><div><b>{examples}/3</b><span>Решены с примерами действий</span></div></div><p><b>Один случай остаётся неразрешённым.</b> Для противоречащих обещаний требуется дополнительное знание ситуации.</p><p className="work-muted">Референсные решения человека не собраны. Скриптовая проба не измеряет эмоциональный интеллект и не доказывает биологическое преимущество.</p><details><summary>Посмотреть исходные примеры и новые случаи</summary><pre>{JSON.stringify({ demonstrations: report.knowledge_probe.demonstrations, cases: report.knowledge_probe.cases }, null, 2)}</pre></details></div>
      </section>
      <section className="work-next"><h2>Что проверять с реальной командой</h2><p>Записать решения эксперта на рабочих эпизодах. Сравнить передачу через описание с передачей через действия. Проверить результат на новых задачах и учесть помощь людей.</p><p>«Коллективное бессознательное» и «Jungian Paradox» здесь обозначают исследовательскую аналогию владельца. Объяснение остатка неизвестного пока не установлено.</p></section>
      <footer className="work-footer"><span>TEST · Все проверки открыты. Перенос в реальную команду не подтверждён.</span><a download="alashi-work-simulation-TEST.json" href={`data:application/json;charset=utf-8,${encodeURIComponent(JSON.stringify(report, null, 2))}`}>Скачать результат с основаниями ↓</a></footer>
    </div>
  </main>
}
