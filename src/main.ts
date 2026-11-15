import { invoke } from '@tauri-apps/api/core';
import { createElement, Fragment, useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import { connectSnapshot, snapshotStore, type SettingsIssue, type ShortcutIssue } from './snapshot.js';

const settingsMessage: Record<SettingsIssue, string> = {
  invalid: 'Settings could not be parsed. Rooom is using defaults.',
  unreadable: 'Settings could not be read. Rooom is using defaults.',
  unwritable: 'The default settings could not be saved.',
};
const shortcutMessage: Record<ShortcutIssue, string> = {
  invalid: 'The shortcut in settings.json is invalid.',
  unavailable: 'The shortcut could not be registered. It may be in use.',
};

function Shell() {
  const snapshot = useSyncExternalStore(snapshotStore.subscribe, snapshotStore.read);
  const [error, setError] = useState<string | null>(null);

  if (!snapshot) return null;

  async function windowAction(command: 'open_panel' | 'close_panel' | 'focus_panel' | 'toggle_pin') {
    try {
      setError(null);
      await invoke(command);
    } catch {
      setError('The window action could not be completed.');
    }
  }

  const content = snapshot.surface === 'strip'
    ? createElement(
      'button',
      {
        className: 'rm-strip grid place-items-center',
        type: 'button',
        'aria-label': 'Open Rooom',
        title: error ?? 'Open Rooom',
        onClick: () => void windowAction('open_panel'),
      },
      createElement('span', { className: 'rm-strip-mark', 'aria-hidden': true }),
    )
    : createElement(
      Fragment,
      null,
      createElement(
        'header',
        { className: 'rm-panel-header' },
        createElement('h1', { id: 'rooom-title' }, 'Rooom'),
        createElement(
          'div',
          { className: 'rm-panel-actions' },
          createElement(
            'button',
            { className: 'rm-collapse', type: 'button', 'aria-pressed': snapshot.pinned, onClick: () => void windowAction('toggle_pin') },
            'Pin',
          ),
          createElement(
            'button',
            { className: 'rm-collapse', type: 'button', onClick: () => void windowAction('close_panel') },
            'Collapse',
          ),
        ),
      ),
      createElement('p', { className: 'rm-empty', role: 'status' }, 'No active sessions'),
      snapshot.settingsIssue
        ? createElement('p', { className: 'rm-notice', role: 'status' }, settingsMessage[snapshot.settingsIssue])
        : null,
      snapshot.shortcutIssue
        ? createElement('p', { className: 'rm-notice', role: 'status' }, shortcutMessage[snapshot.shortcutIssue])
        : null,
      error ? createElement('p', { className: 'rm-notice', role: 'alert' }, error) : null,
    );

  return createElement('main', {
    className: 'rm-shell',
    'data-surface': snapshot.surface,
    'data-edge': snapshot.settings.edge,
    'data-revision': snapshot.revision,
    onPointerDown: (event: PointerEvent) => {
      if (snapshot.surface === 'panel' && event.isTrusted && event.button === 0) {
        void windowAction('focus_panel');
      }
    },
  }, content);
}

const target = document.getElementById('root');
if (!target) throw new Error('Missing Rooom root element.');
const root = createRoot(target);
root.render(createElement(Shell));
void connectSnapshot().catch(() => {
  root.render(createElement('p', { role: 'alert' }, 'Could not connect to Rooom.'));
});
