import type { HomeSearch } from './routes/index'

export function homePresentation(search: HomeSearch, feedMode: 'live' | 'sample' | 'off'): 'preview' | 'live' | 'waiting' {
  if (search.demo || search.pose) return 'preview'
  return feedMode === 'live' ? 'live' : 'waiting'
}
