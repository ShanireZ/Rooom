import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type Edge = 'left' | 'right' | 'top' | 'bottom';
export type Surface = 'strip' | 'panel';
export type SettingsIssue = 'invalid' | 'unreadable' | 'unwritable';
export type ShortcutIssue = 'invalid' | 'unavailable';
export type Client = 'claude' | 'codex' | 'antigravity' | 'harness';
export type DeliveryState = 'queued' | 'received' | 'started' | 'uncertain' | 'expired' | 'rejected';
export interface SessionKey { client: Client; id: string }
export interface RequestKey { session: SessionKey; id: string }
export interface Session { key: SessionKey; title: string; sendBlocked: string | null }
export interface OwnerRequest {
  key: RequestKey;
  summary: string;
  workspace: string | null;
  detail: string;
  decisionBlocked: string | null;
  allowForSession?: boolean;
  canOpenSession: boolean;
}
export interface InboxSnapshot {
  adapters: { client: Client; reason: string | null; setup: string }[];
  sessions: Session[];
  requests: OwnerRequest[];
  timeline: { session: SessionKey; id: string; text: string }[];
  deliveries: { id: string; session: SessionKey; state: DeliveryState; externalId: string | null }[];
}

export interface Settings {
  edge: Edge;
  display: string | null;
  shortcut: string | null;
  captureExclusion: boolean;
}

export interface Snapshot {
  revision: number;
  surface: Surface;
  settings: Settings;
  settingsIssue: SettingsIssue | null;
  pinned: boolean;
  shortcutIssue: ShortcutIssue | null;
  inbox: InboxSnapshot;
}

export function createSnapshotStore() {
  let current: Snapshot | null = null;
  const listeners = new Set<() => void>();

  return {
    read: () => current,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    accept(snapshot: Snapshot) {
      if (current && snapshot.revision <= current.revision) return;
      current = snapshot;
      listeners.forEach((listener) => listener());
    },
  };
}

export const snapshotStore = createSnapshotStore();

export async function connectSnapshot() {
  await listen<Snapshot>('rooom:snapshot', ({ payload }) => snapshotStore.accept(payload));
  snapshotStore.accept(await invoke<Snapshot>('get_snapshot'));
}
