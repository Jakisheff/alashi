/** Hold the last shown frame on pause, while allowing explicit frame seeks. */
export function playbackFrame(actual: number, selected: number, previous: number | null, playing: boolean, explicitSeek: boolean) {
  return !playing && !explicitSeek ? previous ?? actual : selected
}
