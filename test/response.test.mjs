import { ok, doesNotThrow, deepStrictEqual, strictEqual } from 'node:assert/strict'
import { test } from 'node:test'

import { Response } from '../index.js'

test('Response', async t => {
  await t.test('constructor', () => {
    ok(Response)

    doesNotThrow(() => new Response(), 'should construct with no arguments')
    doesNotThrow(() => new Response({}), 'should construct with an empty object')

    doesNotThrow(() => new Response({
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('Hello, World!')
    }), 'should construct with an object argument')
  })

  await t.test('status', () => {
    const response = new Response({
      status: 404,
      headers: { 'Content-Type': 'text/plain' }
    })

    strictEqual(response.status, 404, 'should set the status correctly')
    response.status = 500
    strictEqual(response.status, 500, 'should allow status to be changed')
  })

  await t.test('headers', () => {
    const response = new Response({
      status: 200,
      headers: {
        'Content-Type': 'application/json',
        'X-Custom-Header': 'CustomValue'
      }
    })

    // TODO: Types returned by getters in napi-rs seem to fail instanceof checks
    // ok(response.headers instanceof Headers, 'should create Headers instance for headers')
    strictEqual(response.headers.get('content-type'), 'application/json', 'should set the Content-Type header correctly')
    strictEqual(response.headers.get('x-custom-header'), 'CustomValue', 'should set the custom header correctly')

    response.headers = {
      'Content-Type': 'text/plain',
    }
    strictEqual(response.headers.get('content-type'), 'text/plain', 'should allow headers to be changed')
    ok(!response.headers.has('x-custom-header'), 'should remove old headers when replacing headers')
  })

  await t.test('body', () => {
    const response = new Response({
      status: 200,
      body: Buffer.from('Hello, World!')
    })

    ok(response.body instanceof Buffer, 'should create Buffer instance for body')
    strictEqual(response.body.toString('utf8'), 'Hello, World!', 'should set the body correctly')

    response.body = Buffer.from('New body content')
    strictEqual(response.body.toString('utf8'), 'New body content', 'should update the body content correctly')
  })

  await t.test('toJSON', () => {
    const response = new Response({
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: Buffer.from('{"message": "Hello, World!"}')
    })

    deepStrictEqual(response.toJSON(), {
      status: 200,
      headers: {
        'content-type': 'application/json'
      },
      body: Buffer.from('{"message": "Hello, World!"}')
    }, 'should serialize to JSON correctly')
  })
})
