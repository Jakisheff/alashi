import assert from 'node:assert/strict'
import { Box3, DataTexture, Mesh, MeshStandardMaterial, Raycaster, Vector3, type BufferGeometry } from 'three'
import { createBrandCoin } from '../src/brand/coin.ts'
import { BRAND } from '../src/brand/geometry.ts'

for (const [radius, thickness, centerY] of [[.145, .032, 0], [.12, .035, .125], [1, .22, 0]]) {
  const coin = createBrandCoin(radius, thickness, centerY), other = createBrandCoin(radius, thickness, centerY)
  coin.root.updateMatrixWorld(true)
  const box = new Box3().setFromObject(coin.root)
  assert(Math.abs(box.max.x - radius) < 1e-6 && Math.abs(box.min.x + radius) < 1e-6)
  assert(Math.abs(box.getCenter(new Vector3()).y - centerY) < 1e-6)
  assert(box.max.z < thickness / 2 + radius * .055 && box.min.z > -thickness / 2 - radius * .055, 'Coin grew beyond the original contact envelope')
  const field = coin.root.getObjectByName('alashi-coin-field') as Mesh<BufferGeometry, MeshStandardMaterial>
  const rim = coin.root.getObjectByName('alashi-coin-rim') as Mesh<BufferGeometry, MeshStandardMaterial>
  assert.equal(field.material.color.getHexString(), BRAND.lilac.slice(1).toLowerCase())
  assert.equal(rim.material.color.getHexString(), BRAND.silver.slice(1).toLowerCase())
  const texture = field.material.bumpMap!
  assert(texture instanceof DataTexture)
  assert(texture.generateMipmaps && texture !== other.materials[0].bumpMap)
  const values = texture.image.data as Uint8Array
  let min = 255, max = 0
  for (let i = 0; i < values.length; i += 4) { min = Math.min(min, values[i]); max = Math.max(max, values[i]); assert.equal(values[i + 3], 255) }
  assert(max - min > 20 && min > 200, 'Subtle machining grain is missing')
  const marks = coin.root.children.filter(node => node.name === 'alashi-forall-symbol')
  assert.equal(marks.length, 2)
  for (const side of [-1, 1]) {
    const ray = new Raycaster(new Vector3(0, centerY + radius * .22, side), new Vector3(0, 0, -side))
    assert.equal(ray.intersectObject(coin.root, true)[0]?.object.name, 'alashi-forall-symbol', 'Raised crossbar must face out on both sides')
    ray.set(new Vector3(0, centerY - radius * .08, side), new Vector3(0, 0, -side))
    assert.equal(ray.intersectObject(coin.root, true)[0]?.object.name, 'alashi-coin-field', 'Triangular aperture must stay open')
  }
  coin.fade(.4)
  assert(coin.materials.every(material => material.opacity === .4 && !material.depthWrite))
  assert(other.materials.every(material => material.opacity === 1), 'Coins share mutable materials')
  coin.fade(1); assert(coin.materials.every(material => material.depthWrite))
  let released = false; texture.addEventListener('dispose', () => { released = true })
  coin.dispose(); other.dispose(); assert(released)
}
console.log('PASS: coin palette, both raised symbols, open apertures, subtle texture, contact envelope, fade, instance isolation and disposal')
