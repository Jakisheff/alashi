import type { Run } from './client'
export type Stage = { task: string; route: number; start: number; work: number; end: number | null; ok: boolean; tool: boolean }
export function timeline(run: Run, origin?: number): Stage[] {
  const out: Stage[] = []
  const starts = Object.values(run.calls).map(c => c.started_at)
  const first = origin ?? (starts.length ? Math.min(...starts) : run.created_at)
  for (const [i, spec] of run.plan.entries()) {
    const c = run.calls[spec.id]; if (!c) continue
    const barrier = Math.max(0, ...out.filter(s => spec.depends_on.includes(s.task)).map(s => s.end ?? s.work))
    const start = Math.max(barrier + .12, c.started_at - first), work = start + .8
    const end = c.finished_at === null ? null : Math.max(work + .45, c.finished_at - first) + .8
    out.push({ task: spec.id, route: i % 4, start, work, end, ok: c.quality_passed === true, tool: spec.kind === 'tool' })
  }
  return out
}
export function point(route: number[][], p: number) { p = Math.max(0, Math.min(1, p)); const q = 1 - p, w = [q ** 3, 3 * q * q * p, 3 * q * p * p, p ** 3]; return [0, 1].map(axis => route.reduce((sum, v, i) => sum + v[axis] * w[i], 0)) }
export const smooth = (p: number) => { p = Math.max(0, Math.min(1, p)); return p * p * (3 - 2 * p) }
