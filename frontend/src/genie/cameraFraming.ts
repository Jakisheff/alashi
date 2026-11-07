export const HOME_CAMERA_TARGET: [number, number, number] = [.15, -.4, 0]
const VIEW_DIRECTION = [1.05, .3, 5.8]

export function cameraPlacement(aspect: number, studio = false) {
  // Legacy gestures extend farther than idle; the smallest home canvas also
  // needs 38px above the face for its preview badge. Full GLB frames bound this.
  const scale = Math.max(1, .85 / aspect) * (studio ? 1.14 : 1.36)
  const target: [number, number, number] = studio ? [.15, -.7, 0] : HOME_CAMERA_TARGET
  const position: [number, number, number] = [
    target[0] + VIEW_DIRECTION[0] * scale,
    target[1] + VIEW_DIRECTION[1] * scale,
    target[2] + VIEW_DIRECTION[2] * scale,
  ]
  const distance = Math.hypot(...VIEW_DIRECTION) * scale
  return { position, target, maxOrbitDistance: Math.max(14, distance) }
}
