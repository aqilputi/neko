// A tiny DOM, just enough for Preact to render InboxWalk under Vitest's
// node environment (no jsdom/happy-dom in the project). Test-only.

type Listener = (e: unknown) => void;

export class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  constructor(
    readonly nodeType: number,
    readonly localName: string,
    public data = '',
  ) {}
  get ownerDocument(): FakeDocument {
    return fakeDocument;
  }
  get firstChild(): FakeNode | null {
    return this.childNodes[0] ?? null;
  }
  get nextSibling(): FakeNode | null {
    const p = this.parentNode;
    if (!p) return null;
    return p.childNodes[p.childNodes.indexOf(this) + 1] ?? null;
  }
  insertBefore(n: FakeNode, ref: FakeNode | null): FakeNode {
    n.remove();
    const i = ref ? this.childNodes.indexOf(ref) : -1;
    if (i < 0) this.childNodes.push(n);
    else this.childNodes.splice(i, 0, n);
    n.parentNode = this;
    return n;
  }
  appendChild(n: FakeNode): FakeNode {
    return this.insertBefore(n, null);
  }
  removeChild(n: FakeNode): FakeNode {
    n.remove();
    return n;
  }
  remove(): void {
    const p = this.parentNode;
    if (!p) return;
    p.childNodes.splice(p.childNodes.indexOf(this), 1);
    this.parentNode = null;
  }
  get textContent(): string {
    return this.nodeType === 3 ? this.data : this.childNodes.map((c) => c.textContent).join('');
  }
  set textContent(v: string) {
    if (this.nodeType === 3) this.data = v;
    else for (const c of [...this.childNodes]) c.remove();
  }
}

export class FakeElement extends FakeNode {
  attributes: Record<string, string> = {};
  listeners: Record<string, Listener[]> = {};
  styleProps: Record<string, string> = {};
  style: Record<string, unknown>;
  constructor(
    tag: string,
    readonly namespaceURI: string | null = null,
  ) {
    super(1, tag);
    const props = this.styleProps;
    this.style = new Proxy(
      {
        setProperty: (k: string, v: string) => {
          props[k] = v;
        },
      } as Record<string, unknown>,
      {
        get: (t, k: string) => (k in t ? t[k] : (props[k] ?? '')),
        set: (_t, k: string, v) => {
          props[k] = String(v);
          return true;
        },
      },
    );
  }
  setAttribute(k: string, v: unknown) {
    this.attributes[k] = String(v);
  }
  removeAttribute(k: string) {
    delete this.attributes[k];
  }
  getAttribute(k: string): string | null {
    return this.attributes[k] ?? null;
  }
  addEventListener(type: string, fn: Listener) {
    (this.listeners[type] ??= []).push(fn);
  }
  removeEventListener(type: string, fn: Listener) {
    this.listeners[type] = (this.listeners[type] ?? []).filter((f) => f !== fn);
  }
  dispatch(type: string, e: Record<string, unknown> = {}) {
    for (const fn of this.listeners[type] ?? []) fn.call(this, { type, target: this, currentTarget: this, ...e });
  }
  getBoundingClientRect() {
    return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
  }
  // preact reads/writes some props directly (class → className etc.)
  set className(v: string) {
    this.attributes.class = v;
  }
  get className(): string {
    return this.attributes.class ?? '';
  }
  /** Descendants (self excluded) matching pred. */
  findAll(pred: (e: FakeElement) => boolean): FakeElement[] {
    const out: FakeElement[] = [];
    const walk = (n: FakeNode) => {
      for (const c of n.childNodes) {
        if (c instanceof FakeElement) {
          if (pred(c)) out.push(c);
          walk(c);
        }
      }
    };
    walk(this);
    return out;
  }
  byTestId(id: string): FakeElement[] {
    return this.findAll((e) => e.attributes['data-testid'] === id);
  }
}

export class FakeDocument {
  createElement(tag: string) {
    return new FakeElement(tag);
  }
  createElementNS(ns: string | null, tag: string) {
    return new FakeElement(tag, ns);
  }
  createTextNode(text: string) {
    return new FakeNode(3, '', text);
  }
}

export const fakeDocument = new FakeDocument();

export interface FakeWindow {
  innerWidth: number;
  reduced: boolean;
  mqListeners: Listener[];
  winListeners: Record<string, Listener[]>;
  setReduced(v: boolean): void;
  fire(type: string): void;
}

/** Installs document/window globals; returns the window knobs and an uninstaller. */
export function installFakeDom(): { win: FakeWindow; restore(): void } {
  const g = globalThis as Record<string, unknown>;
  const saved = ['document', 'innerWidth', 'matchMedia', 'addEventListener', 'removeEventListener'].map(
    (k) => [k, Object.getOwnPropertyDescriptor(g, k)] as const,
  );
  const win: FakeWindow = {
    innerWidth: 800,
    reduced: false,
    mqListeners: [],
    winListeners: {},
    setReduced(v) {
      win.reduced = v;
      for (const fn of win.mqListeners) fn({});
    },
    fire(type) {
      for (const fn of win.winListeners[type] ?? []) fn({ type });
    },
  };
  const def = (k: string, get: () => unknown) => Object.defineProperty(g, k, { configurable: true, get });
  def('document', () => fakeDocument);
  def('innerWidth', () => win.innerWidth);
  def('matchMedia', () => () => ({
    get matches() {
      return win.reduced;
    },
    addEventListener: (_t: string, fn: Listener) => win.mqListeners.push(fn),
    removeEventListener: (_t: string, fn: Listener) => {
      win.mqListeners = win.mqListeners.filter((f) => f !== fn);
    },
  }));
  def('addEventListener', () => (t: string, fn: Listener) => (win.winListeners[t] ??= []).push(fn));
  def('removeEventListener', () => (t: string, fn: Listener) => {
    win.winListeners[t] = (win.winListeners[t] ?? []).filter((f) => f !== fn);
  });
  return {
    win,
    restore() {
      for (const [k, d] of saved) {
        if (d) Object.defineProperty(g, k, d);
        else delete g[k];
      }
    },
  };
}
