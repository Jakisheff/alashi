import { CylinderGeometry, ExtrudeGeometry, Group, Mesh, MeshStandardMaterial, Path, Shape, TorusGeometry } from 'three'
import { BRAND, SYMBOL_APERTURE, SYMBOL_HEIGHT, SYMBOL_OUTLINE, SYMBOL_WIDTH } from './geometry.ts'

/** Owned geometry, same contact envelope as the existing props; never mutates cached GLBs. */
export function createBrandCoin(radius: number, thickness: number, centerY = 0) {
  const root = new Group()
  root.name = 'alashi-forall-coin'
  const face = new MeshStandardMaterial({ color: BRAND.lilac, roughness: .5, metalness: .25, transparent: true })
  const silver = new MeshStandardMaterial({ color: BRAND.silver, roughness: .32, metalness: .75, transparent: true })
  const discGeometry = new CylinderGeometry(radius, radius, thickness, 48)
  const rimGeometry = new TorusGeometry(radius * .89, radius * .055, 8, 48)
  const shape = new Shape()
  const draw = (target: Shape | Path, points: readonly (readonly [number, number])[]) => {
    points.forEach(([x, y], i) => {
      const px = (x - SYMBOL_WIDTH / 2) / SYMBOL_WIDTH * radius * 1.3
      const py = (SYMBOL_HEIGHT / 2 - y) / SYMBOL_WIDTH * radius * 1.3
      if (i === 0) target.moveTo(px, py); else target.lineTo(px, py)
    })
    target.closePath()
  }
  draw(shape, SYMBOL_OUTLINE)
  const aperture = new Path(); draw(aperture, SYMBOL_APERTURE); shape.holes.push(aperture)
  const markGeometry = new ExtrudeGeometry(shape, { depth: radius * .025, bevelEnabled: false, steps: 1 })
  const disc = new Mesh(discGeometry, face); disc.name = 'alashi-coin-field'
  disc.rotation.x = Math.PI / 2; disc.position.y = centerY; root.add(disc)
  for (const side of [-1, 1]) {
    const rim = new Mesh(rimGeometry, silver); rim.name = 'alashi-coin-rim'
    rim.position.set(0, centerY, side * thickness / 2); root.add(rim)
    const mark = new Mesh(markGeometry, silver); mark.name = 'alashi-forall-symbol'
    mark.position.set(0, centerY, side * (thickness / 2 + radius * .006))
    if (side < 0) mark.rotation.y = Math.PI
    root.add(mark)
  }
  const materials = [face, silver]
  function fade(opacity: number) {
    for (const material of materials) { material.opacity = opacity; material.depthWrite = opacity > .98 }
  }
  function dispose() {
    discGeometry.dispose(); rimGeometry.dispose(); markGeometry.dispose()
    materials.forEach((material) => material.dispose())
  }
  return { root, materials, fade, dispose }
}
