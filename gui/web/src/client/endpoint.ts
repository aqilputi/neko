// Endpoint discovery: where the core's protocol lives and the bearer token
// for it. Three sources, in order:
//
//  1. `?endpoint=http://127.0.0.1:PORT&token=…` in the page URL (dev, tests);
//  2. `window.nkEndpoint()`, provided by the Wails shell (WP-6.5);
//  3. the page's own origin (+ `?token=` if present) — the Vite dev server
//     proxies /v1 to NK_ENDPOINT, so this is what `npm run demo` uses.
//
// The core's RPC server sends no CORS headers: a cross-origin `endpoint`
// only works when the page is served from a host that allows it (or the
// endpoint is the page's origin behind a proxy).

/** Where the protocol is served and how to authenticate. */
export interface Endpoint {
  /** Base URL without trailing slash, e.g. http://127.0.0.1:41234. */
  url: string;
  /** Bearer token (TCP loopback); absent for a trusting proxy. */
  token?: string;
}

/** What the Wails shell's Endpoint() binding may hand back. */
export type ShellEndpoint = Endpoint | string;

/** The globals getEndpoint looks at (injectable for tests). */
export interface EndpointEnv {
  location: { href: string; origin: string };
  nkEndpoint?: () => ShellEndpoint | Promise<ShellEndpoint>;
}

declare global {
  interface Window {
    nkEndpoint?: () => ShellEndpoint | Promise<ShellEndpoint>;
  }
}

/** Strips trailing slashes so paths can be appended with "/v1/…". */
export function normalizeURL(url: string): string {
  return url.replace(/\/+$/, '');
}

/** Parses `endpoint`/`token` from a page URL; null when there is no endpoint. */
export function endpointFromQuery(href: string): Endpoint | null {
  const q = new URL(href).searchParams;
  const url = q.get('endpoint');
  if (!url) return null;
  const token = q.get('token');
  return token ? { url: normalizeURL(url), token } : { url: normalizeURL(url) };
}

function fromShell(v: ShellEndpoint): Endpoint {
  if (typeof v === 'string') {
    // "http://host:port?token=…" or a bare base URL
    const u = new URL(v);
    const token = u.searchParams.get('token');
    u.search = '';
    const url = normalizeURL(u.toString());
    return token ? { url, token } : { url };
  }
  const out: Endpoint = { url: normalizeURL(v.url) };
  if (v.token) out.token = v.token;
  return out;
}

/** Resolves the endpoint from the query, the shell or the page's origin. */
export async function getEndpoint(env: EndpointEnv = window): Promise<Endpoint> {
  const q = endpointFromQuery(env.location.href);
  if (q) return q;
  if (typeof env.nkEndpoint === 'function') {
    return fromShell(await env.nkEndpoint());
  }
  const token = new URL(env.location.href).searchParams.get('token');
  const url = normalizeURL(env.location.origin);
  return token ? { url, token } : { url };
}
