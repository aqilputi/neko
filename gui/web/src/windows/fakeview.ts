// A TermView without a DOM, for the window tests.

import type { Disposable, TermView } from './term.ts';

type Cb<T> = (v: T) => void;

export class FakeView implements TermView {
  cols = 90;
  rows = 25;
  written: string[] = [];
  focused = false;
  private readonly cbs = { data: [] as Cb<string>[], binary: [] as Cb<string>[], resize: [] as Cb<{ cols: number; rows: number }>[], title: [] as Cb<string>[] };
  private readonly dec = new TextDecoder();

  private on<K extends keyof FakeView['cbs']>(k: K, cb: FakeView['cbs'][K][number]): Disposable {
    const list = this.cbs[k] as unknown[];
    list.push(cb);
    return { dispose: () => list.splice(list.indexOf(cb), 1) };
  }

  write(d: string | Uint8Array): void {
    this.written.push(typeof d === 'string' ? d : this.dec.decode(d));
  }
  onData(cb: Cb<string>) {
    return this.on('data', cb);
  }
  onBinary(cb: Cb<string>) {
    return this.on('binary', cb);
  }
  onResize(cb: Cb<{ cols: number; rows: number }>) {
    return this.on('resize', cb);
  }
  onTitleChange(cb: Cb<string>) {
    return this.on('title', cb);
  }
  focus(): void {
    this.focused = true;
  }
  blur(): void {
    this.focused = false;
  }
  text(): string {
    return this.written.join('');
  }

  // driving helpers
  type(s: string): void {
    for (const cb of [...this.cbs.data]) cb(s);
  }
  binary(s: string): void {
    for (const cb of [...this.cbs.binary]) cb(s);
  }
  resize(cols: number, rows: number): void {
    this.cols = cols;
    this.rows = rows;
    for (const cb of [...this.cbs.resize]) cb({ cols, rows });
  }
  title(t: string): void {
    for (const cb of [...this.cbs.title]) cb(t);
  }
  get listeners(): number {
    return Object.values(this.cbs).reduce((n, l) => n + l.length, 0);
  }
}
