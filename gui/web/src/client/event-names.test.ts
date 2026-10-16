import { describe, expect, it } from 'vitest';
import { EVENT_NAMES, Ev } from './types';

// EventSource has no wildcard: an event name missing here never reaches the
// GUI. inbox.summoned is published by the core when inbox.summon runs — also
// from another client, e.g. `nk run inbox.summon` on a compositor shortcut —
// and the inbox cat must open that session.
describe('EVENT_NAMES', () => {
  it('includes inbox.summoned', () => {
    expect(Ev.InboxSummoned).toBe('inbox.summoned');
    expect(EVENT_NAMES).toContain('inbox.summoned');
  });
});
