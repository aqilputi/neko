// Starts `nk serve --demo` on tcp://127.0.0.1:0 in an isolated temp world
// (XDG_* and HOME under a temp dir, so the user's config/state are never
// read or written) and hands back its endpoint and token. Used by the
// Playwright global setup and by `npm run demo`.
//
// The binary comes from $NK_BIN, or is built with $GO (default "go") from
// the repo root. Plain erasable TypeScript: runs under `node` directly.

import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

export interface Demo {
  /** http://127.0.0.1:PORT */
  endpoint: string;
  token: string;
  dir: string;
  stop(): Promise<void>;
}

const here = import.meta.dirname;
export const repoRoot = resolve(here, '..', '..', '..');

/** Builds the nk binary into dir (or returns $NK_BIN). */
export function nkBinary(dir: string): string {
  const fromEnv = process.env.NK_BIN;
  if (fromEnv) return fromEnv;
  const out = join(dir, 'nk');
  const go = process.env.GO || 'go';
  execFileSync(go, ['build', '-o', out, './cmd/nekuroddo'], { cwd: repoRoot, stdio: 'inherit' });
  return out;
}

export async function startDemo(timeoutMs = 20_000): Promise<Demo> {
  const dir = mkdtempSync(join(tmpdir(), 'nk-gui-e2e-'));
  const bin = nkBinary(dir);
  const env: Record<string, string> = { ...(process.env as Record<string, string>) };
  for (const k of ['XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'XDG_STATE_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'HOME']) {
    const d = join(dir, k.toLowerCase());
    mkdirSync(d, { recursive: true, mode: 0o700 });
    env[k] = d;
  }
  const child = spawn(bin, ['serve', '--demo', '--listen', 'tcp://127.0.0.1:0'], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stderr = '';
  child.stderr?.on('data', (b: Buffer) => {
    stderr += b.toString();
  });

  const addr = await new Promise<string>((ok, fail) => {
    const timer = setTimeout(() => fail(new Error(`nk serve --demo não respondeu em ${timeoutMs} ms\n${stderr}`)), timeoutMs);
    const rl = createInterface({ input: child.stdout! });
    rl.on('line', (line) => {
      const m = /servindo em tcp:\/\/(\S+)/.exec(line);
      if (m?.[1]) {
        clearTimeout(timer);
        ok(m[1]);
      }
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      fail(new Error(`nk serve --demo saiu (${code})\n${stderr}`));
    });
  });

  const uid = process.getuid?.() ?? 0;
  const tokenPath = join(env.XDG_RUNTIME_DIR!, `nekuroddo-${uid}.token`);
  if (!existsSync(tokenPath)) throw new Error(`token não encontrado em ${tokenPath}`);
  const token = readFileSync(tokenPath, 'utf8').trim();

  return {
    endpoint: `http://${addr}`,
    token,
    dir,
    stop: () => stopChild(child).finally(() => rmSync(dir, { recursive: true, force: true })),
  };
}

function stopChild(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((ok) => {
    const kill = setTimeout(() => child.kill('SIGKILL'), 5000);
    child.once('exit', () => {
      clearTimeout(kill);
      ok();
    });
    child.kill('SIGTERM');
  });
}
