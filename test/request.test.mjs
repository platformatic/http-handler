import { ok, throws, doesNotThrow, deepStrictEqual, strictEqual } from 'node:assert/strict'

import { Request } from '../index.js'

import { test } from 'node:test'

test('Request', async t => {
  await t.test('constructor', () => {
    ok(Request)
    throws(() => new Request(), {
      // TODO: This is a bad error message, can this be improved?
      message: 'Cannot convert undefined or null to object',
    })

    let request
    doesNotThrow(() => {
      request = new Request({
        method: 'GET',
        uri: '/test.php',
        headers: { 'Content-Type': 'application/json' },
        body: Buffer.from('Hello, World!')
      })
    }, 'should construct with an object argument')

    deepStrictEqual(request.toJSON(), {
      method: 'GET',
      uri: '/test.php',
      headers: { 'content-type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }, 'should convert to JSON correctly')
  })

  await t.test('docroot', (t) => {
    const docroot = '/var/www/html'
    const request = new Request({
      uri: '/test',
      docroot
    })

    strictEqual(request.docroot, docroot, 'should set the docroot correctly')
  })
})
