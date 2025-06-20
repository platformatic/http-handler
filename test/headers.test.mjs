import { NapiHeaders } from '../index.cjs'

import { test } from 'node:test'

test('NapiHeaders', async t => {
  t.assert.ok(NapiHeaders)

  const headers = new NapiHeaders({
    foo: 'bar',
    baz: ['buz', 'bux']
  })

  t.assert.ok(headers instanceof NapiHeaders)

  t.assert.ok(headers.set('foo', 'bar'))
  t.assert.ok(headers.set('baz', ['buz', 'bux']))

  t.assert.deepStrictEqual(headers.getAll('baz'), ['buz', 'bux'])
  t.assert.deepStrictEqual(headers.get('foo'), 'bar')
  t.assert.deepStrictEqual(headers.get('not-exists'), null)
  t.assert.deepStrictEqual(headers.has('foo'), true)
  t.assert.deepStrictEqual(headers.has('not-exists'), false)

  // Validate iterables
  t.assert.deepStrictEqual(new Set(headers.keys()), new Set(['foo', 'baz']))
  t.assert.deepStrictEqual(new Set(headers.values()), new Set(['bar', 'buz', 'bux']))
  t.assert.deepStrictEqual(new Set(headers.entries()), new Set([['foo', 'bar'], ['baz', 'buz'], ['baz', 'bux']]))

  // Has a forEach method
  let calls = new Set()
  headers.forEach((value, key, inst) => {
    calls.add([value, key, inst])
  })
  t.assert.deepStrictEqual(calls, new Set([
    ['bar', 'foo', headers],
    ['buz', 'baz', headers],
    ['bux', 'baz', headers]
  ]))
})
