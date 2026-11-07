import assert from 'node:assert/strict'
import { test } from 'node:test'
import { homePresentation } from '../src/homePresentation.ts'

test('/?demo&game=1 keeps the illustrative preview label', () => {
  assert.equal(homePresentation({ demo: true, game: '1' }, 'live'), 'preview')
})

test('/?pose=accepted&game=1 keeps the frozen preview label', () => {
  assert.equal(homePresentation({ pose: 'accepted', t: .55, game: '1' }, 'live'), 'preview')
})

test('ordinary game and agent watches retain the live label', () => {
  assert.equal(homePresentation({ game: '1' }, 'live'), 'live')
  assert.equal(homePresentation({ agent: 'a'.repeat(64) }, 'live'), 'live')
})

test('standalone previews and an idle owner view retain their labels', () => {
  assert.equal(homePresentation({ demo: true }, 'off'), 'preview')
  assert.equal(homePresentation({ pose: 'idle' }, 'off'), 'preview')
  assert.equal(homePresentation({ agent: 'a'.repeat(64) }, 'off'), 'waiting')
})
