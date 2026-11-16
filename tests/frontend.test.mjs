import assert from 'node:assert/strict';
import { test, afterEach } from 'node:test';
import React from 'react';
import Renderer from 'react-test-renderer';
import { loadTs } from './load-ts.mjs';

const { consumeChatStream } = loadTs('src/api/sse.ts');
const { readPersona, readLore, saveLocal } = loadTs('src/lib/storage.ts');
const { useConversation } = loadTs('src/hooks/useConversation.ts');
const { api } = loadTs('src/api/client.ts');
const { act } = Renderer;
const encode = value => new TextEncoder().encode(value);
const stream = chunks => new ReadableStream({ start(c) { for (const chunk of chunks) c.enqueue(chunk); c.close(); } });
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
let root;
afterEach(async () => { if (root) await act(async () => root.unmount()); root = undefined; });

test('SSE handles byte-by-byte Chinese, CRLF, multiline JSON and final DONE without newline', async () => {
  const data = 'data: {"token":\r\ndata: "你好🌍"}\r\n\r\ndata: [DONE]';
  const events = [];
  await consumeChatStream(stream([...encode(data)].map(byte => new Uint8Array([byte]))), e => events.push(e));
  assert.deepEqual(events, [{ token: '你好🌍' }]);
});

test('SSE reports truncated and malformed streams and stops at DONE', async () => {
  await assert.rejects(consumeChatStream(stream([encode('data: {"token":"partial"}\n\n')]), () => {}), /连接提前结束/);
  await assert.rejects(consumeChatStream(stream([encode('data: {broken}\n\n')]), () => {}));
  const events = [];
  await consumeChatStream(stream([encode('data: [DONE]\n\ndata: {broken}\n\n')]), e => events.push(e));
  assert.deepEqual(events, []);
});

test('corrupt or unavailable local storage cannot crash startup', () => {
  globalThis.localStorage = { getItem: () => 'null', setItem: () => { throw new Error('quota'); } };
  assert.deepEqual(readPersona(), { name: '旅人', description: '' });
  localStorage.getItem = () => '{"oops":true}';
  assert.deepEqual(readLore(), []);
  localStorage.getItem = () => '[{"id":1},null,{"id":"a","keys":"x","content":"y","enabled":true}]';
  assert.equal(readLore().length, 1);
  assert.doesNotThrow(() => saveLocal('test', {}));
});

const context = { persona: { name: 'user', description: '' }, lore_entries: [] };
const scripts = [];
const errors = [];
const error = value => errors.push(value);
const notice = () => {};
let hook;
function Harness({ card }) { hook = useConversation(card, context, scripts, error, notice); return null; }
async function render(card) { await act(async () => { root = Renderer.create(React.createElement(Harness, { card })); }); }
async function settle() { await act(async () => {}); }

test('late chat and history responses never overwrite the selected character', async () => {
  const a = deferred(), b = deferred(), oldHistory = deferred();
  api.chats.list = card => card === 'a' ? a.promise : b.promise;
  api.history.get = chat => chat === 'old' ? oldHistory.promise : Promise.resolve([{ role: 'assistant', content: 'B' }]);
  await render('a');
  await act(async () => a.resolve([{ id: 'old', card: 'a', title: 'A' }]));
  await act(async () => root.update(React.createElement(Harness, { card: 'b' })));
  await act(async () => b.resolve([{ id: 'new', card: 'b', title: 'B' }]));
  await act(async () => oldHistory.resolve([{ role: 'assistant', content: 'stale' }]));
  assert.equal(hook.chatId, 'new');
  assert.equal(hook.messages[0].content, 'B');
  assert.equal(hook.loading, false);
});

test('rapid send creates one conversation and preserves its greeting; regenerate sends a replace request', async () => {
  api.chats.list = async () => [];
  const creation = deferred();
  let creates = 0, calls = [];
  api.chats.create = () => { creates++; return creation.promise; };
  api.history.get = async () => [{ role: 'assistant', content: 'greeting' }];
  api.chat.stream = async input => { calls.push(input); return stream([encode('data: {"token":"reply"}\n\ndata: [DONE]\n\n')]); };
  await render('a');
  await act(async () => { hook.setInput('hello'); });
  let first;
  await act(async () => { first = hook.send(); void hook.send(); });
  assert.equal(creates, 1);
  await act(async () => { creation.resolve({ id: 'new', card: 'a', title: 'new' }); await first; });
  assert.deepEqual(hook.messages.map(m => m.content), ['greeting', 'hello', 'reply']);
  await act(async () => { hook.regenerate(); });
  await settle();
  assert.equal(calls[1].regenerate, true);
  assert.deepEqual(hook.messages.map(m => m.content), ['greeting', 'hello', 'reply']);
});

test('failed deletion keeps the conversation and generation failure restores input', async () => {
  globalThis.confirm = () => true;
  api.chats.list = async () => [{ id: 'chat', card: 'a', title: 'A' }];
  api.history.get = async () => [];
  api.chats.delete = async () => ({ error: 'database unavailable' });
  api.chat.stream = async () => { throw new Error('offline'); };
  await render('a'); await settle();
  await act(async () => { await hook.deleteChat('chat'); });
  assert.equal(hook.chatId, 'chat'); assert.equal(hook.chats.length, 1);
  await act(async () => { hook.setInput('retry me'); });
  await act(async () => { await hook.send(); });
  assert.equal(hook.busy, false); assert.equal(hook.input, 'retry me');
  assert.deepEqual(hook.messages, []);
});
