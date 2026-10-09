// Technical trace of Amir's selected concept (docs 014711da, PNG 9e1236c0).
// One symmetric, straight-edged contour for the website, exports and 3D coins.
export const BRAND = { lilac: '#A78BFA', silver: '#D9D9E2', dark: '#111018', light: '#F4F2F8' } as const
export const SYMBOL_WIDTH = 364
export const SYMBOL_HEIGHT = 332
export const SYMBOL_OUTLINE = [[0, 0], [59, 0], [98, 88], [266, 88], [305, 0], [364, 0], [210, 332], [154, 332]] as const
export const SYMBOL_APERTURE = [[116, 124], [248, 124], [182, 264]] as const
const path = (points: readonly (readonly [number, number])[]) => `M${points.map(([x, y]) => `${x} ${y}`).join('L')}Z`
export const SYMBOL_PATH = path(SYMBOL_OUTLINE) + path(SYMBOL_APERTURE)
