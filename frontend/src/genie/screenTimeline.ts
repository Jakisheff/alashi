const PAGE_CHARS = 46
const CHARS_PER_SECOND = 32
const HOLD_SECONDS = 1.8
// Jura has no emoji; omit glyphs that would fetch an external fallback font.
const NO_GLYPH = /[\p{Extended_Pictographic}️‍]/gu

export function speechPages(text: string) {
  const pages: string[] = []
  let page = ''
  for (const word of text.replace(NO_GLYPH, '').split(/\s+/).filter(Boolean)) {
    if (page && (page + ' ' + word).length > PAGE_CHARS) {
      pages.push(page)
      page = word
    } else page = page ? `${page} ${word}` : word
  }
  if (page) pages.push(page)
  return pages
}

export function speechFrame(pages: readonly string[], time: number, holdFinal: boolean, reducedMotion: boolean) {
  let elapsed = time
  for (let i = 0; i < pages.length; i++) {
    const page = pages[i], duration = page.length / CHARS_PER_SECOND + HOLD_SECONDS
    if (elapsed <= duration || (holdFinal && i === pages.length - 1)) {
      return { text: reducedMotion ? page : page.slice(0, Math.floor(elapsed * CHARS_PER_SECOND)), of: page.length }
    }
    elapsed -= duration
  }
  return { text: '', of: 0 }
}
