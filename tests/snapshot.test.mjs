import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createSnapshotStore } from '../.frontend-build/snapshot.js';

const initial = {
  revision: 0,
  surface: 'strip',
  settings: { edge: 'right', display: null, shortcut: null, captureExclusion: true },
  settingsIssue: null,
  pinned: false,
  shortcutIssue: null,
};

test('a delayed initial snapshot cannot replace a panel event', () => {
  const store = createSnapshotStore();
  const states = [];
  const unsubscribe = store.subscribe(() => states.push(store.read()));
  const opened = { ...initial, revision: 1, surface: 'panel' };
  store.accept(opened);
  store.accept(initial);
  store.accept(opened);
  assert.deepEqual(states, [opened]);
  unsubscribe();
  store.accept({ ...initial, revision: 2 });
  assert.deepEqual(states, [opened]);
});
