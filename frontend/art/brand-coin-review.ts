import { ACESFilmicToneMapping, Color, DirectionalLight, DoubleSide, Fog, HemisphereLight, Mesh, MeshBasicMaterial, MeshStandardMaterial, PCFSoftShadowMap, PerspectiveCamera, PlaneGeometry, PMREMGenerator, Scene, WebGLRenderer } from 'three'
import { createBrandCoin } from '../src/brand/coin.ts'
import { BRAND } from '../src/brand/geometry.ts'

const canvas = document.querySelector('canvas')!
const renderer = new WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true })
renderer.setSize(1600, 1600, false)
renderer.toneMapping = ACESFilmicToneMapping
renderer.toneMappingExposure = .72
renderer.shadowMap.enabled = true; renderer.shadowMap.type = PCFSoftShadowMap
const scene = new Scene(); scene.background = new Color(BRAND.dark)
scene.fog = new Fog(BRAND.dark, 4.5, 8)
const camera = new PerspectiveCamera(35, 1, .1, 100)
camera.position.set(.48, .16, 4.25); camera.lookAt(0, -.06, 0)
const room = new Scene(); room.background = new Color('#090909')
const panelGeometry = new PlaneGeometry(1, 1), panelMaterials: MeshBasicMaterial[] = []
for (const [x, y, z, width, height, intensity] of [[-1.2, 1, 5, 2.8, 6, 2], [-3, 3, 4, 3, 5, 4], [4, 1, 3, 2, 4, 1.8], [0, -3, 4, 4, 1, .5]]) {
  const material = new MeshBasicMaterial({ color: new Color('#ffffff').multiplyScalar(intensity), side: DoubleSide, toneMapped: false })
  panelMaterials.push(material)
  const panel = new Mesh(panelGeometry, material); panel.position.set(x, y, z); panel.scale.set(width, height, 1); panel.lookAt(0, 0, 0); room.add(panel)
}
const pmrem = new PMREMGenerator(renderer), environment = pmrem.fromScene(room, .015)
scene.environment = environment.texture; scene.environmentIntensity = .7
panelGeometry.dispose(); panelMaterials.forEach(material => material.dispose()); pmrem.dispose()
scene.add(new HemisphereLight('#ffffff', '#111018', .12))
const light = new DirectionalLight('#ffffff', .95)
light.position.set(-3, 4, 5); light.castShadow = true
light.shadow.mapSize.set(2048, 2048); light.shadow.normalBias = .01
scene.add(light)
const fill = new DirectionalLight('#ffffff', .15); fill.position.set(4, 1, 3); scene.add(fill)
const coin = createBrandCoin(1, .22)
coin.root.traverse(node => { if (node instanceof Mesh) { node.castShadow = true; node.receiveShadow = true } })
scene.add(coin.root)
const floor = new Mesh(new PlaneGeometry(200, 200), new MeshStandardMaterial({ color: BRAND.dark, emissive: BRAND.dark, emissiveIntensity: .75, roughness: .72, metalness: .1, toneMapped: false }))
floor.rotation.x = -Math.PI / 2; floor.position.y = -1.002; floor.receiveShadow = true; scene.add(floor)
const render = () => renderer.render(scene, camera)
document.querySelectorAll<HTMLButtonElement>('[data-view]').forEach(button => button.addEventListener('click', () => {
  coin.root.rotation.y = button.dataset.view === 'back' ? Math.PI : 0
  camera.position.set(button.dataset.view === 'front' ? 0 : .48, button.dataset.view === 'front' ? 0 : .16, 4.25)
  camera.lookAt(0, -.06, 0); render()
}))
document.getElementById('download')!.addEventListener('click', () => canvas.toBlob(blob => {
  if (!blob) return
  const url = URL.createObjectURL(blob), link = document.createElement('a')
  link.href = url; link.download = 'alashi-coin-render.png'; link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}, 'image/png'))
render()
window.addEventListener('pagehide', () => { coin.dispose(); floor.geometry.dispose(); floor.material.dispose(); environment.dispose(); renderer.dispose() }, { once: true })
