// Run: npm run optimize:glb. art/desk-genie.glb (raw Blender export) -> public/models/desk-genie.glb, meshopt-compressed.
// drei's useGLTF decodes meshopt out of the box, so the page needs no new code. Skin, clips and text-anchor stay intact.
import { NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS } from '@gltf-transform/extensions'
import { dedup, meshopt, prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'

const [src, dst] = ['art/desk-genie.glb', 'public/models/desk-genie.glb']
await MeshoptEncoder.ready
await MeshoptDecoder.ready
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({
  'meshopt.encoder': MeshoptEncoder,
  'meshopt.decoder': MeshoptDecoder,
})
const doc = await io.read(src)
// 12-bit normals: the default 8 bits bands the subsurf highlights on the body
await doc.transform(dedup(), prune({ keepLeaves: true }), meshopt({ encoder: MeshoptEncoder, level: 'medium', quantizeNormal: 12 }))
await io.write(dst, doc)

const root = doc.getRoot()
const anchor = root.listNodes().some((n) => n.getName() === 'text-anchor')
const clips = root.listAnimations().map((a) => a.getName()).sort()
if (!anchor || clips.join() !== 'accepted,act,fuckOff,idle,rejected') throw new Error(`lost data: anchor ${anchor}, clips ${clips}`)
console.log(`${dst}: anchor ok, clips ${clips.join(', ')}`)
