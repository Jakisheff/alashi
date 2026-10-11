// Admission only: historical journal entries keep their existing projection limits.
export const OWNER_WISH_LIMIT = 140
export function inspectOwnerWish(text: string) {
  const scalars = Array.from(text)
  const count = scalars.length
  const error = !text.trim() ? 'Enter a private instruction.'
    : count > OWNER_WISH_LIMIT ? 'Use no more than 140 characters.'
    : scalars.some((char) => { const point = char.codePointAt(0)!; return point < 32 || (point >= 127 && point <= 159) || (point >= 0xd800 && point <= 0xdfff) })
      ? 'Use valid characters on one line.' : null
  return { count, error, valid: error === null }
}
