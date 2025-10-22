import { ok, throws, doesNotThrow, deepStrictEqual, strictEqual, rejects } from 'node:assert/strict'
import { test } from 'node:test'

import { Request } from '../index.js'

test('Request', async t => {
  await t.test('constructor', () => {
    ok(Request)

    throws(() => new Request(), {
      message: 'Missing `options` argument',
    })

    throws(() => new Request({}), {
      message: 'Missing field `url`',
    })

    doesNotThrow(() => new Request({
      method: 'GET',
      url: 'https://example.com/test.php',
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }), 'should construct with an object argument')
  })

  await t.test('method', () => {
    const request = new Request({
      method: 'POST',
      url: 'https://example.com/test'
    })

    strictEqual(request.method, 'POST', 'should set the method correctly')
    request.method = 'PUT'
    strictEqual(request.method, 'PUT', 'should allow method to be changed')
  })

  await t.test('url', () => {
    const request = new Request({
      method: 'GET',
      url: 'https://example.com/test'
    })

    strictEqual(request.url, 'https://example.com/test', 'should set the URL correctly')
    request.url = 'https://example.com/new-test'
    strictEqual(request.url, 'https://example.com/new-test', 'should allow URL to be changed')
  })

  await t.test('path', () => {
    const request = new Request({
      method: 'GET',
      url: 'https://example.com/api/users?id=123'
    })

    strictEqual(request.path, '/api/users', 'should return just the path portion')
    
    // Test with a simple path-only URL
    const simpleRequest = new Request({
      method: 'GET',
      url: '/simple-path'
    })
    strictEqual(simpleRequest.path, '/simple-path', 'should handle path-only URLs')
  })

  await t.test('url reconstruction from Host header', () => {
    // Test that path-only URLs are reconstructed using Host header
    const request = new Request({
      method: 'GET',
      url: '/api/data?param=value',
      headers: {
        'Host': 'api.example.com',
        'Content-Type': 'application/json'
      }
    })

    strictEqual(request.url, 'https://api.example.com/api/data?param=value', 'should reconstruct full URL from Host header')
    strictEqual(request.path, '/api/data', 'should still return correct path portion')
    
    // Test that full URLs are not modified even with Host header
    const fullUrlRequest = new Request({
      method: 'GET', 
      url: 'https://original.com/test',
      headers: {
        'Host': 'different.com'
      }
    })
    
    strictEqual(fullUrlRequest.url, 'https://original.com/test', 'should not modify full URLs based on Host header')
    strictEqual(fullUrlRequest.path, '/test', 'should return correct path for full URL')
  })

  await t.test('headers', () => {
    const request = new Request({
      method: 'GET',
      url: 'https://example.com/test',
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
      url: 'https://example.com/test',
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
      url: 'https://example.com/test',
      body
    })

    ok(request.body instanceof Buffer, 'should create Buffer instance for body')
    deepStrictEqual(request.body, body, 'should set the body correctly')
  })

  await t.test('toJSON', () => {
    const request = new Request({
      method: 'GET',
      url: 'https://example.com/test',
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('Hello, World!')
    })

    deepStrictEqual(request.toJSON(), {
      method: 'GET',
      url: 'https://example.com/test',
      headers: { 'content-type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }, 'should convert to JSON correctly')
  })

  await t.test('write() should error when body is already provided', async () => {
    // Create a request with a body already provided
    const request = new Request({
      method: 'POST',
      url: 'https://example.com/test',
      body: Buffer.from('initial body')
    })

    // Trying to write should throw an error
    await rejects(
      async () => {
        await request.write(Buffer.from('more data'))
      },
      {
        message: 'Cannot write to request: body has already been provided'
      }
    )
  })

  await t.test('end() should succeed silently when body is already provided', async () => {
    // Create a request with a body already provided
    const request = new Request({
      method: 'POST',
      url: 'https://example.com/test',
      body: Buffer.from('initial body')
    })

    // Trying to end should not throw (returns silently)
    await doesNotThrow(async () => {
      await request.end()
    }, 'should not throw error when calling end() on request with existing body buffer')
  })

  await t.test('write() and end() should work when body is not provided', async () => {
    // Create a request without a body
    const request = new Request({
      method: 'POST',
      url: 'https://example.com/test'
    })

    // These should not throw
    await doesNotThrow(async () => {
      await request.write(Buffer.from('chunk 1'))
      await request.write(Buffer.from('chunk 2'))
      await request.end()
    }, 'should allow write() and end() when no body buffer is present')
  })
})
