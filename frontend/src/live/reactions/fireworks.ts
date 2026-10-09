import { BufferAttribute, BufferGeometry, Group, LineBasicMaterial, LineSegments, Points, PointsMaterial } from 'three'

/** Three short, deterministic bursts behind the hero. Geometry is generated
 * locally once; no textures, model downloads, random per-frame allocations or
 * repeating flashes. Scrubbing backwards reconstructs the same exact burst. */
export function createVictoryFireworks() {
  const group = new Group()
  group.name = 'victory-fireworks'
  const bursts = [
    { at: 1.05, x: -.98, y: .75, spread: .85, color: '#f2cc83' },
    { at: 1.85, x: .98, y: .9, spread: .85, color: '#6de6ca' },
    { at: 2.65, x: -.98, y: 1.05, spread: .65, color: '#e8c3ef' },
  ].map((spec) => {
    const positions = new Float32Array(48 * 3)
    const geometry = new BufferGeometry()
    geometry.setAttribute('position', new BufferAttribute(positions, 3))
    const material = new PointsMaterial({ color: spec.color, size: .06, transparent: true,
      depthWrite: false, toneMapped: false, sizeAttenuation: true })
    material.color.multiplyScalar(2.4)
    const points = new Points(geometry, material)
    points.position.set(spec.x, spec.y, -.8)
    points.frustumCulled = false
    group.add(points)
    const trails = new Float32Array(48 * 6)
    const trailGeometry = new BufferGeometry()
    trailGeometry.setAttribute('position', new BufferAttribute(trails, 3))
    const trailMaterial = new LineBasicMaterial({ color: material.color, transparent: true, depthWrite: false, toneMapped: false })
    const lines = new LineSegments(trailGeometry, trailMaterial)
    lines.position.copy(points.position)
    lines.frustumCulled = false
    group.add(lines)
    return { ...spec, positions, geometry, material, points, trails, trailGeometry, trailMaterial, lines }
  })
  function update(time: number, enabled: boolean) {
    group.visible = enabled
    for (const burst of bursts) {
      const age = time - burst.at
      burst.points.visible = enabled && age >= 0 && age < 1.3
      burst.lines.visible = burst.points.visible
      if (!burst.points.visible) continue
      burst.material.opacity = Math.pow(1 - age / 1.3, 1.5)
      burst.trailMaterial.opacity = burst.material.opacity * .5
      for (let i = 0; i < 48; i++) {
        const angle = i * Math.PI * (3 - Math.sqrt(5))
        const radius = burst.spread * (.38 + (i % 7) * .032) * Math.sin(Math.min(1, age / 1.1) * Math.PI / 2)
        burst.positions[i * 3] = Math.cos(angle) * radius
        burst.positions[i * 3 + 1] = Math.sin(angle) * radius - .13 * age * age
        burst.positions[i * 3 + 2] = Math.sin(i * 1.7) * .1 * age
        for (let axis = 0; axis < 3; axis++) {
          burst.trails[i * 6 + axis] = burst.positions[i * 3 + axis]
          burst.trails[i * 6 + axis + 3] = burst.positions[i * 3 + axis] * .87
        }
      }
      burst.geometry.attributes.position.needsUpdate = true
      burst.trailGeometry.attributes.position.needsUpdate = true
    }
  }
  function dispose() { for (const burst of bursts) { burst.geometry.dispose(); burst.material.dispose(); burst.trailGeometry.dispose(); burst.trailMaterial.dispose() } }
  update(0, false)
  return { group, update, dispose }
}
