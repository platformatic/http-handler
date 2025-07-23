import { Headers } from '../index.js'

import { ok, doesNotThrow, deepStrictEqual, strictEqual } from 'node:assert/strict'
import { test } from 'node:test'

test('Headers', async t => {
  await t.test('Headers constructor', () => {
    ok(Headers)
    doesNotThrow(
      () => new Headers(),
      'should construct without arguments'
    )

    let headers
    doesNotThrow(() => {
      headers = new Headers({
        foo: 'bar',
        baz: ['buz', 'bux']
      })
    }, 'should construct with an object argument with mixed values')

    deepStrictEqual(headers.toJSON(), {
      foo: 'bar',
      baz: ['buz', 'bux']
    }, 'should convert to JSON correctly')
  })

  await t.test('Headers set', () => {
    const headers = new Headers()
    strictEqual(headers.get('not-exists'), null, 'should not have a header that does not exist')
    ok(!headers.set('foo', 'bar'), 'should return false when setting a new header')
    ok(headers.set('foo', 'baz'), 'should return true when replacing an existing header')
    strictEqual(headers.get('foo'), 'baz', 'should have stored the header value')

    ok(headers.set('foo', ['bar', 'baz']), 'should support setting multi-value headers')
    deepStrictEqual(headers.getAll('foo'), ['bar', 'baz'], 'should have kept both values for multi-value header')
  })

  await t.test('Headers has', () => {
    const headers = new Headers({
      foo: 'bar'
    })
    ok(headers.has('foo'), 'should return true for existing header')
    ok(!headers.has('not-exists'), 'should return false for non-existing header')
  })

  await t.test('Headers get', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    strictEqual(headers.get('foo'), 'bar', 'should return the value of an existing header')
    deepStrictEqual(headers.get('baz'), 'buz', 'should return first value for a multi-value header')
    strictEqual(headers.get('not-exists'), null, 'should return null for non-existing header')
  })

  await t.test('Headers getAll', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    deepStrictEqual(headers.getAll('foo'), ['bar'], 'should return all values for a single-value header')
    deepStrictEqual(headers.getAll('baz'), ['buz', 'bux'], 'should return all values for a multi-value header')
    deepStrictEqual(headers.getAll('not-exists'), [], 'should return empty array for non-existing header')
  })

  await t.test('Headers getLine', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    strictEqual(headers.getLine('foo'), 'bar', 'should return the correct line for a single-value header')
    strictEqual(headers.getLine('baz'), 'buz,bux', 'should return the correct line for a multi-value header')
    strictEqual(headers.getLine('not-exists'), null, 'should return null for non-existing header')
  })

  await t.test('Headers clear', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    ok(headers.size > 0, 'should have some headers initially')
    headers.clear()
    strictEqual(headers.size, 0, 'should be empty after clear')
    strictEqual(headers.get('foo'), null, 'should not have any headers after clear')
  })

  await t.test('Headers add', () => {
    const headers = new Headers()
    ok(!headers.add('foo', 'bar'), 'should return false when adding a new header')
    deepStrictEqual(headers.getAll('foo'), ['bar'], 'should have added the header value')
    ok(headers.add('foo', 'baz'), 'should return true when adding another value to an existing header')
    deepStrictEqual(headers.getAll('foo'), ['bar', 'baz'], 'should have both values for multi-value header')
  })

  await t.test('Headers delete', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    ok(headers.delete('foo'), 'should return true when deleting an existing header')
    strictEqual(headers.get('foo'), null, 'should not have the header after deletion')
    ok(!headers.delete('not-exists'), 'should return false when deleting a non-existing header')
  })

  await t.test('Headers size', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    strictEqual(headers.size, 3, 'should have correct size with multiple headers')
    headers.set('foo', 'new-value')
    strictEqual(headers.size, 3, 'should not change size when replacing a header value')
    headers.add('new-header', 'value')
    strictEqual(headers.size, 4, 'should increase size when adding a new header')
    headers.add('baz', 'new-baz-value')
    strictEqual(headers.size, 5, 'should increase size when adding a new value to an existing header')
    headers.clear()
    strictEqual(headers.size, 0, 'should be zero after clearing headers')
  })

  await t.test('Headers entries', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    const entries = new Set(headers.entries())
    deepStrictEqual(entries, new Set([
      ['foo', 'bar'],
      ['baz', 'buz'],
      ['baz', 'bux']
    ]), 'should return correct entries for all headers')
  })

  await t.test('Headers keys', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    const keys = new Set(headers.keys())
    deepStrictEqual(keys, new Set(['foo', 'baz']), 'should return correct keys for all headers')
  })

  await t.test('Headers values', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    const values = new Set(headers.values())
    deepStrictEqual(values, new Set(['bar', 'buz', 'bux']), 'should return correct values for all headers')
  })

  await t.test('Headers forEach', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    const calls = new Set()
    headers.forEach((value, key, inst) => {
      calls.add([value, key, inst])
    })
    deepStrictEqual(calls, new Set([
      ['bar', 'foo', headers],
      ['buz', 'baz', headers],
      ['bux', 'baz', headers]
    ]), 'should call forEach with correct arguments for each header')
  })

  await t.test('Headers toJSON', () => {
    const headers = new Headers({
      foo: 'bar',
      baz: ['buz', 'bux']
    })
    deepStrictEqual(headers.toJSON(), {
      foo: 'bar',
      baz: ['buz', 'bux']
    }, 'should convert to JSON correctly')
  })
})
