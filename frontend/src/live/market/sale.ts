const ramp = (t: number, a: number, b: number) => {
  const x = Math.min(1, Math.max(0, (t - a) / (b - a)))
  return x * x * (3 - 2 * x)
}

/** Look at the stock, lift its weight, offer it, then notice the payment. */
export function saleMotionAt(time: number, reduced = false) {
  const t = Number.isFinite(time) ? Math.max(0, time) : 0
  const look = ramp(t, .25, .65) * (1 - ramp(t, 1.8, 2.35))
  const turn = ramp(t, .65, 1.1) * (1 - ramp(t, 3.25, 3.65))
  const prepare = ramp(t, .95, 1.3) * (1 - ramp(t, 1.45, 1.75))
  const load = ramp(t, 1.45, 1.75) * (1 - ramp(t, 2.8, 3.25))
  const offer = ramp(t, 2.15, 2.8) * (1 - ramp(t, 3.25, 3.65))
  const payment = ramp(t, 2.8, 3.25) * (1 - ramp(t, 4.8, 5.4))
  const nod = ramp(t, 3.75, 4.05) * (1 - ramp(t, 4.05, 4.5))
  return {
    pitch: reduced ? 0 : .035 * prepare - .09 * load + .055 * offer + .065 * nod,
    yaw: reduced ? 0 : .12 * turn,
    roll: reduced ? 0 : -.025 * load + .02 * offer,
    x: reduced ? 0 : .025 * turn + .025 * offer,
    y: reduced ? 0 : -.025 * prepare - .045 * load + .025 * offer,
    z: reduced ? 0 : -.025 * load + .025 * offer,
    tailPitch: reduced ? 0 : .07 * ramp(t, 1.75, 2.1) * (1 - ramp(t, 3.1, 3.55)) - .035 * ramp(t, 4.05, 4.3) * (1 - ramp(t, 4.5, 4.9)),
    gazeX: .85 * Math.max(look, payment),
    gazeY: -.7 * look - .35 * payment,
    gazeWeight: Math.max(look, payment),
    idleWeight: reduced ? 0 : 1 - .65 * Math.max(load, offer),
  }
}
