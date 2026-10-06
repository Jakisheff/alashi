// Run: npm run export:poses. Samples every clip from pose.ts at 30 fps into art/poses.json,
// so the Blender rig (art/desk_genie.py) animates exactly like the R3F preview.
import { writeFileSync } from 'node:fs'
import { CLIP_SECONDS, poseAt, type GenieClip } from '../src/genie/pose.ts'

const FPS = 30
const clips = Object.fromEntries(
  (Object.keys(CLIP_SECONDS) as GenieClip[]).map((clip) => {
    const frames = Math.round(CLIP_SECONDS[clip] * FPS)
    return [clip, Array.from({ length: frames + 1 }, (_, f) => poseAt(clip, f / FPS, f / FPS))]
  }),
)
writeFileSync(new URL('../art/poses.json', import.meta.url), JSON.stringify({ fps: FPS, clips }))
console.log('art/poses.json:', Object.entries(clips).map(([k, v]) => `${k} ${v.length}f`).join(', '))
