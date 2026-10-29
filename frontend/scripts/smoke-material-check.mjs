import { assignSmokeMaterial } from '../src/live/reactions/smokeMaterial.ts'
// Exercise the actual smoke setup effect with real Three objects and shared GLB-like materials.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { BoxGeometry, Group, Mesh, MeshBasicMaterial } from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import ts from 'typescript'

const source = new URL('../src/live/reactions/LivingGenie.tsx', import.meta.url)
const ast = ts.createSourceFile(source.pathname, readFileSync(source, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
let effect
function visit(node) {
  if (ts.isCallExpression(node) && node.expression.getText(ast) === 'useEffect' && node.arguments[0]?.getText(ast).includes('assignSmokeMaterial')) effect = node.arguments[0]
  ts.forEachChild(node, visit)
}
visit(ast)
assert.ok(effect, 'Smoke setup must remain an effect with cleanup')
const code = ts.transpileModule(`module.exports = ${effect.getText(ast)}`, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2023}}).outputText
const oldSmoke = new MeshBasicMaterial(); oldSmoke.name = 'tail-smoke'
const other = new MeshBasicMaterial(); other.name = 'casing'
const replacement = new MeshBasicMaterial(); replacement.name = 'new-tail-smoke'
let smokeDisposals = 0, oldDisposals = 0, otherDisposals = 0
replacement.dispose = () => { smokeDisposals++ }; oldSmoke.dispose = () => { oldDisposals++ }; other.dispose = () => { otherDisposals++ }
const original = new Group(), geometry = new BoxGeometry()
const single = new Mesh(geometry, oldSmoke); single.name = 'single'
const multi = new Mesh(geometry, [other, oldSmoke, other]); multi.name = 'multi'
const plain = new Mesh(geometry, other); plain.name = 'plain'; plain.renderOrder = 7
original.add(single, multi, plain)
const hero = clone(original), module = {exports:null}, smoke = {material:replacement}
runInNewContext(code, {module, hero, smoke, assignSmokeMaterial})
const cleanup = module.exports()
assert.equal(hero.getObjectByName('single').material, replacement)
assert.deepEqual(hero.getObjectByName('multi').material, [other, replacement, other])
assert.equal(hero.getObjectByName('plain').material, other)
assert.equal(hero.getObjectByName('single').renderOrder, 1)
assert.equal(hero.getObjectByName('multi').renderOrder, 1)
assert.equal(hero.getObjectByName('plain').renderOrder, 7)
assert.equal(single.material, oldSmoke, 'The cached source mesh keeps its original material')
assert.deepEqual(multi.material, [other, oldSmoke, other], 'The cached multi-material mesh stays untouched')
assert.equal(smokeDisposals, 0, 'A live scene must not dispose its smoke')
cleanup()
assert.equal(smokeDisposals, 1, 'Effect cleanup disposes the scene-owned material')
assert.equal(oldDisposals, 0, 'Cached smoke material is not disposed')
assert.equal(otherDisposals, 0, 'Unrelated material is not disposed')
console.log('PASS: smoke assignment preserves shared materials and effect cleanup')
