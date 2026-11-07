import { CylinderGeometry, DataTexture, ExtrudeGeometry, Group, LatheGeometry, LinearFilter, LinearMipmapLinearFilter, Mesh, MeshStandardMaterial, Path, RGBAFormat, Shape, Vector2 } from 'three'
import { BRAND, SYMBOL_APERTURE, SYMBOL_HEIGHT, SYMBOL_OUTLINE, SYMBOL_WIDTH } from './geometry.ts'

/** Owned geometry, same contact envelope as the existing props; never mutates cached GLBs. */
export function createBrandCoin(radius: number, thickness: number, centerY = 0) {
  const root = new Group()
  root.name = 'alashi-forall-coin'
  // Local, deterministic machining grain; mipmaps keep the tiny payment prop quiet.
  const size = 256, pixels = new Uint8Array(size * size * 4)
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const dx = (x + .5) / size * 2 - 1, dy = (y + .5) / size * 2 - 1
    const rings = Math.sin(Math.hypot(dx, dy) * 520 + Math.sin(Math.atan2(dy, dx) * 9) * .7)
    const grain = ((x * 73 + y * 151 + x * y * 19) % 23) - 11
    const value = 232 + Math.round(rings * 13 + grain * .4), offset = (y * size + x) * 4
    pixels[offset] = pixels[offset + 1] = pixels[offset + 2] = value; pixels[offset + 3] = 255
  }
  const texture = new DataTexture(pixels, size, size, RGBAFormat)
  texture.generateMipmaps = true; texture.minFilter = LinearMipmapLinearFilter
  texture.magFilter = LinearFilter; texture.needsUpdate = true
  const face = new MeshStandardMaterial({ color: BRAND.lilac, roughness: .48, metalness: .22, map: texture, bumpMap: texture, bumpScale: radius * .003, transparent: true })
  const silver = new MeshStandardMaterial({ color: BRAND.silver, roughness: .3, metalness: .82, map: texture, bumpMap: texture, bumpScale: radius * .0015, transparent: true })
  const half = thickness / 2, bevel = radius * .014
  const discGeometry = new CylinderGeometry(radius * .872, radius * .872, thickness - bevel * 2, 96)
  // The reference has a broad flat annulus and a short bevel.
  const rimGeometry = new LatheGeometry([
    new Vector2(radius * .87, -half + bevel), new Vector2(radius * .885, -half),
    new Vector2(radius - bevel, -half), new Vector2(radius, -half + bevel),
    new Vector2(radius, half - bevel), new Vector2(radius - bevel, half),
    new Vector2(radius * .885, half), new Vector2(radius * .87, half - bevel),
    new Vector2(radius * .87, -half + bevel),
  ], 96)
  const vertices = rimGeometry.getAttribute('position'), normals = rimGeometry.getAttribute('normal'), uv = rimGeometry.getAttribute('uv')
  for (let i = 0; i < vertices.count; i++) {
    if (Math.abs(Math.abs(vertices.getY(i)) - half) < 1e-6) normals.setXYZ(i, 0, Math.sign(vertices.getY(i)), 0)
    if (Math.abs(normals.getY(i)) > .7) uv.setXY(i, vertices.getX(i) / radius * .5 + .5, vertices.getZ(i) / radius * .5 + .5)
  }
  const shape = new Shape()
  const draw = (target: Shape | Path, points: readonly (readonly [number, number])[]) => {
    points.forEach(([x, y], i) => {
      const px = (x - SYMBOL_WIDTH / 2) / SYMBOL_WIDTH * radius * 1.25
      const py = (SYMBOL_HEIGHT / 2 - y) / SYMBOL_WIDTH * radius * 1.25
      if (i === 0) target.moveTo(px, py); else target.lineTo(px, py)
    })
    target.closePath()
  }
  draw(shape, SYMBOL_OUTLINE)
  const aperture = new Path(); draw(aperture, SYMBOL_APERTURE); shape.holes.push(aperture)
  const markGeometry = new ExtrudeGeometry(shape, { depth: radius * .04, bevelEnabled: true, bevelSize: radius * .005, bevelThickness: radius * .005, bevelSegments: 1, steps: 1 })
  const markVertices = markGeometry.getAttribute('position'), markNormals = markGeometry.getAttribute('normal'), markUV = markGeometry.getAttribute('uv')
  for (let i = 0; i < markVertices.count; i++) if (Math.abs(markNormals.getZ(i)) > .7) {
    markUV.setXY(i, markVertices.getX(i) / radius * .5 + .5, markVertices.getY(i) / radius * .5 + .5)
  }
  const disc = new Mesh(discGeometry, face); disc.name = 'alashi-coin-field'
  disc.rotation.x = Math.PI / 2; disc.position.y = centerY; root.add(disc)
  const rim = new Mesh(rimGeometry, silver); rim.name = 'alashi-coin-rim'
  rim.rotation.x = Math.PI / 2; rim.position.y = centerY; root.add(rim)
  for (const side of [-1, 1]) {
    const mark = new Mesh(markGeometry, silver); mark.name = 'alashi-forall-symbol'
    mark.position.set(0, centerY, side * (half - bevel + radius * .004))
    if (side < 0) mark.rotation.y = Math.PI
    root.add(mark)
  }
  const materials = [face, silver]
  function fade(opacity: number) {
    for (const material of materials) { material.opacity = opacity; material.depthWrite = opacity > .98 }
  }
  function dispose() {
    discGeometry.dispose(); rimGeometry.dispose(); markGeometry.dispose()
    materials.forEach((material) => material.dispose()); texture.dispose()
  }
  return { root, materials, fade, dispose }
}
