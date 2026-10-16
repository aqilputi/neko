import { effect } from '@preact/signals';
import { describe, expect, it } from 'vitest';
import { GW, snapshot, tree } from '../test/fixtures.ts';
import { applySnapshot, applyTree, patchViewState } from './reducers.ts';
import { createStore } from './store.ts';

describe('createStore', () => {
  it('updates through reducers and notifies subscribers', () => {
    const st = createStore();
    const seen: number[] = [];
    const dispose = effect(() => {
      seen.push(st.rows.value.length);
    });
    st.update(applySnapshot, snapshot());
    st.update(applyTree, tree());
    dispose();
    expect(seen).toEqual([0, 4]);
    expect(st.sessionsById.value.get(GW)?.label).toBe('api-gateway');
  });

  it('cursorRow follows cursor_key, then cursor', () => {
    const st = createStore();
    expect(st.cursorRow.value).toBeUndefined();
    st.update(applyTree, tree());
    expect(st.cursorRow.value?.key).toBe('ctx:visio');
    st.update(patchViewState, { cursor_key: `sess:${GW}` });
    expect(st.cursorRow.value?.label).toBe('api-gateway');
    st.update(patchViewState, { cursor_key: 'gone', cursor: 2 });
    expect(st.cursorRow.value?.label).toBe('migração do banco');
    st.update(patchViewState, { cursor_key: '', cursor: undefined });
    expect(st.cursorRow.value?.key).toBe('ctx:visio');
  });
});
