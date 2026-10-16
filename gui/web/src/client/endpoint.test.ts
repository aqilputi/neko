import { describe, expect, it } from 'vitest';
import { endpointFromQuery, getEndpoint, normalizeURL, type EndpointEnv } from './endpoint.ts';

function env(href: string, nkEndpoint?: EndpointEnv['nkEndpoint']): EndpointEnv {
  const e: EndpointEnv = { location: { href, origin: new URL(href).origin } };
  if (nkEndpoint) e.nkEndpoint = nkEndpoint;
  return e;
}

describe('getEndpoint', () => {
  const cases: Array<{ name: string; env: EndpointEnv; want: object }> = [
    {
      name: 'query endpoint and token',
      env: env('http://localhost:5173/?endpoint=http://127.0.0.1:4321/&token=abc'),
      want: { url: 'http://127.0.0.1:4321', token: 'abc' },
    },
    {
      name: 'query endpoint wins over the shell',
      env: env('http://localhost:5173/?endpoint=http%3A%2F%2F127.0.0.1%3A9', () => ({ url: 'http://shell' })),
      want: { url: 'http://127.0.0.1:9' },
    },
    {
      name: 'shell object',
      env: env('wails://wails/', () => ({ url: 'http://127.0.0.1:7/', token: 't' })),
      want: { url: 'http://127.0.0.1:7', token: 't' },
    },
    {
      name: 'shell object without token',
      env: env('wails://wails/', async () => ({ url: 'http://127.0.0.1:7' })),
      want: { url: 'http://127.0.0.1:7' },
    },
    {
      name: 'shell string with token',
      env: env('wails://wails/', async () => 'http://127.0.0.1:8/?token=zz'),
      want: { url: 'http://127.0.0.1:8', token: 'zz' },
    },
    {
      name: 'shell string without token',
      env: env('wails://wails/', () => 'http://127.0.0.1:8'),
      want: { url: 'http://127.0.0.1:8' },
    },
    {
      name: 'same origin with token (dev proxy)',
      env: env('http://localhost:5173/?token=q'),
      want: { url: 'http://localhost:5173', token: 'q' },
    },
    {
      name: 'same origin without token',
      env: env('http://localhost:5173/app'),
      want: { url: 'http://localhost:5173' },
    },
  ];
  it.each(cases)('$name', async ({ env, want }) => {
    expect(await getEndpoint(env)).toEqual(want);
  });
});

describe('helpers', () => {
  it('normalizeURL strips trailing slashes', () => {
    expect(normalizeURL('http://a//')).toBe('http://a');
    expect(normalizeURL('http://a')).toBe('http://a');
  });
  it('endpointFromQuery is null without endpoint', () => {
    expect(endpointFromQuery('http://a/?token=x')).toBeNull();
  });
});
