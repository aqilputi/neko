import { beforeEach, describe, expect, it } from 'vitest';
import { FakeWebSocket } from '../test/fakes.ts';
import { PaneSocket, type PaneHandlers } from './pane.ts';
import type { Control } from './types.ts';

function setup() {
  const log: Array<[string, unknown]> = [];
  const h: PaneHandlers = {
    onOpen: () => log.push(['open', null]),
    onData: (b) => log.push(['data', new TextDecoder().decode(b)]),
    onTitle: (t) => log.push(['title', t]),
    onExit: (e) => log.push(['exit', e]),
    onControl: (c: Control) => log.push(['control', c.type]),
    onClose: (code, reason) => log.push(['close', [code, reason]]),
  };
  const p = new PaneSocket('p1', 'ws://h/v1/panes/p1/ws?token=t', FakeWebSocket, h);
  return { p, ws: FakeWebSocket.last, log };
}

beforeEach(() => FakeWebSocket.reset());

describe('PaneSocket', () => {
  it('asks for array buffers', () => {
    const { ws } = setup();
    expect(ws.binaryType).toBe('arraybuffer');
    expect(ws.url).toBe('ws://h/v1/panes/p1/ws?token=t');
  });

  it('queues input until open, then flushes in order', () => {
    const { p, ws, log } = setup();
    expect(p.readyState).toBe(0);
    p.write('ls\r');
    p.resize(120, 40);
    expect(ws.sent).toHaveLength(0);
    ws.open();
    expect(log).toEqual([['open', null]]);
    expect(ws.texts).toEqual(['ls\r']);
    expect(ws.controls).toEqual([{ type: 'resize', cols: 120, rows: 40 }]);
    expect(typeof ws.sent[1]).toBe('string');
  });

  it('sends strings and bytes as binary frames, resize as text', () => {
    const { p, ws } = setup();
    ws.open();
    p.write('ã');
    p.write(new Uint8Array([27, 91, 65]));
    p.resize(80, 24);
    expect(ws.sent[0]).toBeInstanceOf(Uint8Array);
    expect(ws.texts).toEqual(['ã', '\x1b[A']);
    expect(ws.controls).toEqual([{ type: 'resize', cols: 80, rows: 24 }]);
  });

  type In = { name: string; frame: string | ArrayBuffer | Uint8Array; want: Array<[string, unknown]> };
  const inputs: In[] = [
    { name: 'binary ArrayBuffer', frame: new TextEncoder().encode('olá').buffer as ArrayBuffer, want: [['data', 'olá']] },
    { name: 'binary view', frame: new TextEncoder().encode('xyz').subarray(1), want: [['data', 'yz']] },
    { name: 'title', frame: '{"type":"title","text":"claude · api"}', want: [['control', 'title'], ['title', 'claude · api']] },
    { name: 'title without text', frame: '{"type":"title"}', want: [['control', 'title'], ['title', '']] },
    { name: 'exit ok', frame: '{"type":"exit"}', want: [['control', 'exit'], ['exit', undefined]] },
    { name: 'exit with err', frame: '{"type":"exit","err":"boom"}', want: [['control', 'exit'], ['exit', 'boom']] },
    { name: 'unknown control', frame: '{"type":"later"}', want: [['control', 'later']] },
    { name: 'bad JSON', frame: '{', want: [] },
  ];
  it.each(inputs)('receives $name', ({ frame, want }) => {
    const { ws, log } = setup();
    ws.open();
    log.length = 0;
    ws.receive(frame);
    expect(log).toEqual(want);
  });

  it('ignores frames of unknown kinds', () => {
    const { ws, log } = setup();
    ws.onmessage?.(new MessageEvent('message', { data: 42 }));
    expect(log).toEqual([]);
  });

  it('reports the close code (1013 = reconnect)', () => {
    const { p, ws, log } = setup();
    p.write('lost');
    ws.serverClose(1013, 'try again');
    expect(log).toEqual([['close', [1013, 'try again']]]);
    p.write('nowhere');
    expect(ws.sent).toHaveLength(0);
  });

  it('close is idempotent and drops queued input', () => {
    const { p, ws } = setup();
    p.write('x');
    p.close();
    p.close();
    expect(ws.closeCalls).toEqual([[1000, '']]);
    ws.open();
    expect(ws.sent).toHaveLength(0);
  });

  it('works without handlers', () => {
    new PaneSocket('p2', 'ws://h', FakeWebSocket);
    const ws = FakeWebSocket.last;
    ws.open();
    ws.receive('{"type":"exit"}');
    ws.receive('{"type":"title","text":"t"}');
    ws.receive(new Uint8Array([1]));
    ws.serverClose();
  });
});
