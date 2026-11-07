import type { Material, Mesh, Object3D } from 'three'

/** Change only the cloned hero's smoke slots; cached GLB materials remain untouched. */
export function assignSmokeMaterial(hero: Object3D, material: Material) {
  hero.traverse((o) => {
    const mesh = o as Mesh
    if (!mesh.isMesh) return
    if (Array.isArray(mesh.material)) {
      mesh.material = mesh.material.map((m) => m.name === 'tail-smoke' ? material : m)
    } else if (mesh.material.name === 'tail-smoke') mesh.material = material
    if (Array.isArray(mesh.material) ? mesh.material.includes(material) : mesh.material === material) mesh.renderOrder = 1
  })
}
