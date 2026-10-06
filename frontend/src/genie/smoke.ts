import { Color, MeshStandardMaterial } from 'three'

// Glowing genie smoke for the tail: drifting fbm wisps, a fresnel rim and soft transparency,
// injected into MeshStandardMaterial so skinning and lighting keep working.
const NOISE = /* glsl */ `
  varying vec3 vObjPos;
  uniform float uTime;
  float hash(vec3 p) { return fract(sin(dot(p, vec3(17.1, 31.7, 47.3))) * 43758.5453); }
  float noise(vec3 p) {
    vec3 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
      mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y),
      f.z);
  }
  float fbm(vec3 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
    return v;
  }
`

export function createSmokeMaterial() {
  const material = new MeshStandardMaterial({
    name: 'tail-smoke',
    color: new Color('#178f7c'),
    emissive: new Color('#1ccfb0'),
    roughness: 0.25,
    transparent: true,
    depthWrite: false,
  })
  const uniforms = { uTime: { value: 0 } }
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms)
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vObjPos;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvObjPos = position;')
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\n${NOISE}`)
      .replace(
        '#include <emissivemap_fragment>',
        `#include <emissivemap_fragment>
        vec3 flow = vec3(0.0, uTime * 0.45, uTime * 0.15);
        float wisp = fbm(vObjPos * 3.2 + flow);
        float vein = smoothstep(0.55, 0.8, fbm(vObjPos * 6.5 - flow * 1.7));
        float rim = pow(1.0 - abs(dot(normalize(vNormal), normalize(vViewPosition))), 2.2);
        totalEmissiveRadiance *= 0.25 + 1.1 * wisp * wisp + 0.9 * vein;
        totalEmissiveRadiance += vec3(0.35, 0.95, 0.85) * rim * 0.8;
        diffuseColor.a = clamp(0.45 + 0.45 * rim + 0.3 * wisp + 0.25 * vein, 0.0, 0.95);`,
      )
  }
  return { material, uniforms }
}
