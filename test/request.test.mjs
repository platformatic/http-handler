import { ok, throws, doesNotThrow, deepStrictEqual, strictEqual } from 'node:assert/strict'
import { test } from 'node:test'

import { Request } from '../index.js'

test('Request', async t => {
  await t.test('constructor', () => {
    ok(Request)

    throws(() => new Request(), {
      message: 'Missing `options` argument',
    })

    throws(() => new Request({}), {
      message: 'Missing field `uri`',
    })

    doesNotThrow(() => new Request({
      method: 'GET',
      uri: '/test.php',
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }), 'should construct with an object argument')
  })

  await t.test('method', () => {
    const request = new Request({
      method: 'POST',
      uri: '/test'
    })

    strictEqual(request.method, 'POST', 'should set the method correctly')
    request.method = 'PUT'
    strictEqual(request.method, 'PUT', 'should allow method to be changed')
  })

  await t.test('uri', () => {
    const request = new Request({
      method: 'GET',
      uri: '/test'
    })

    strictEqual(request.uri, '/test', 'should set the URI correctly')
    request.uri = '/new-test'
    strictEqual(request.uri, '/new-test', 'should allow URI to be changed')
  })

  await t.test('headers', () => {
    const request = new Request({
      method: 'GET',
      uri: '/test',
      headers: {
        'Content-Type': 'application/json',
        'X-Custom-Header': 'CustomValue'
      }
    })

    // TODO: Types returned by getters in napi-rs seem to fail instanceof checks
    // ok(request.headers instanceof Headers, 'should create Headers instance for headers')
    strictEqual(request.headers.get('content-type'), 'application/json', 'should set the Content-Type header correctly')
    strictEqual(request.headers.get('x-custom-header'), 'CustomValue', 'should set the custom header correctly')

    // Object reassignment constructs a new Headers instance internally
    request.headers = {
      'Content-Type': 'text/plain',
    }
    strictEqual(request.headers.get('content-type'), 'text/plain', 'should allow headers to be changed')
    ok(!request.headers.has('x-custom-header'), 'should remove old headers when replacing headers')
  })

  await t.test('docroot', () => {
    const docroot = '/var/www/html'
    const request = new Request({
      uri: '/test',
      docroot
    })

    strictEqual(request.docroot, docroot, 'should set the docroot correctly')
    request.docroot = '/new/docroot'
    strictEqual(request.docroot, '/new/docroot', 'should allow docroot to be changed')
  })

  await t.test('body', () => {
    const body = Buffer.from('Hello, World!')
    const request = new Request({
      method: 'POST',
      uri: '/test',
      body
    })

    ok(request.body instanceof Buffer, 'should create Buffer instance for body')
    deepStrictEqual(request.body, body, 'should set the body correctly')

    request.body = Buffer.from('New Body')
    deepStrictEqual(request.body, Buffer.from('New Body'), 'should update the body correctly')
  })

  await t.test('toJSON', () => {
    const request = new Request({
      method: 'GET',
      uri: '/test',
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('Hello, World!')
    })

    deepStrictEqual(request.toJSON(), {
      method: 'GET',
      uri: '/test',
      headers: { 'content-type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }, 'should convert to JSON correctly')
  })
})
