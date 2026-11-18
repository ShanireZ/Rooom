import { invoke } from '@tauri-apps/api/core';
import { createElement, Fragment, useState } from 'react';
import type { Client, InboxSnapshot, OwnerRequest, SessionKey } from './snapshot.js';

const label: Record<Client, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
  antigravity: 'Antigravity',
  harness: 'DeepSeek Harness',
};
const keyFor = ({ client, id }: SessionKey) => `${client}/${id}`;
const decisionLabel = {
  allow: 'Allow once', deny: 'Deny', allowForSession: 'Allow these paths for this session',
} as const;
type Decision = keyof typeof decisionLabel;

const previewEndLength = 48;
const whitespaceEscapes: Record<string, string> = { '\r': '\\r', '\n': '\\n', '\t': '\\t' };

function RequestDetails({ detail }: { detail: string }) {
  const characters = [...detail];
  const length = characters.length;
  const excerpt = length > 2 * previewEndLength
    ? `${characters.slice(0, previewEndLength).join('')} … ${characters.slice(-previewEndLength).join('')}`
    : detail;
  const preview = excerpt.replace(
    /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/gu,
    character => whitespaceEscapes[character] ?? `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`,
  );
  return createElement('details', null,
    createElement('summary', null,
      createElement('span', { className: 'rm-request-length' }, `${length} ${length === 1 ? 'character' : 'characters'}`),
      createElement('code', { className: 'rm-request-preview' }, preview),
    ),
    createElement('pre', null, detail),
  );
}

export function InboxPanel(inbox: InboxSnapshot) {
  return createElement(Inbox, { inbox });
}

function Inbox({ inbox }: { inbox: InboxSnapshot }) {
  const [recipient, setRecipient] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState(false);
  const selected = inbox.sessions.find(({ key }) => keyFor(key) === recipient);

  async function send(event: SubmitEvent) {
    event.preventDefault();
    if (pending || !selected || selected.sendBlocked || !(event.currentTarget instanceof HTMLFormElement)) return;
    const form = event.currentTarget;
    const text = new FormData(form).get('message');
    if (typeof text !== 'string' || !text.trim()) return;
    setPending(true);
    setError(null);
    setConfirmation(false);
    try {
      await invoke('send_message', { target: selected.key, text });
      form.reset();
    } catch (err) {
      setError(typeof err === 'string' ? err : 'Could not send to the selected session.');
    } finally {
      setPending(false);
    }
  }

  async function answer(request: OwnerRequest, decision: Decision) {
    if (pending || request.decisionBlocked) return;
    setPending(true);
    setError(null);
    setConfirmation(false);
    try {
      await invoke('answer_request', { request, decision });
      if (request.key.session.client === 'codex') setConfirmation(true);
    } catch (err) {
      setError(typeof err === 'string' ? err : 'The request could not be answered.');
    } finally {
      setPending(false);
    }
  }

  async function openRequestSession(request: OwnerRequest) {
    if (pending || !request.canOpenSession) return;
    setPending(true);
    setError(null);
    try {
      await invoke('open_request_session', { request: request.key });
    } catch (err) {
      setError(typeof err === 'string' ? err : 'Could not open the original session.');
    } finally {
      setPending(false);
    }
  }

  return createElement(Fragment, null,
    ...inbox.requests.map(request => createElement('article', {
      key: `${keyFor(request.key.session)}/${request.key.id}`,
      className: 'rm-request',
      'data-request': request.key.id,
      'data-session': keyFor(request.key.session),
    },
    createElement('div', { className: 'rm-request-heading' },
      createElement('strong', null,
        label[request.key.session.client],
        request.workspace ? createElement(Fragment, null,
          ' · ', createElement('bdi', { 'data-request-workspace': true }, request.workspace),
        ) : null,
        ` · ${request.summary}`,
      ),
      request.canOpenSession ? createElement('button', {
        type: 'button', className: 'rm-collapse',
        disabled: pending,
        'data-open-session': true,
        onClick: () => void openRequestSession(request),
      }, 'Open session') : null,
    ),
    request.detail ? createElement(RequestDetails, { detail: request.detail }) : null,
    createElement('div', { className: 'rm-panel-actions' },
      ...((request.allowForSession ? ['allow', 'allowForSession', 'deny'] : ['allow', 'deny']) as Decision[]).map(decision => createElement('button', {
        key: decision, type: 'button', className: 'rm-collapse',
        disabled: pending || Boolean(request.decisionBlocked),
        title: request.decisionBlocked,
        'data-decision': decision,
        onClick: () => void answer(request, decision),
      }, decisionLabel[decision])),
    ),
    request.decisionBlocked ? createElement('small', null, request.decisionBlocked) : null,
    )),
    inbox.sessions.length ? createElement('ol', { className: 'rm-session-list', 'aria-label': 'Sessions' },
      ...inbox.sessions.map(session => createElement('li', {
        key: keyFor(session.key), 'data-inbox-session': keyFor(session.key),
      }, `${label[session.key.client]} · ${session.title}`)),
    ) : createElement('p', { className: 'rm-empty', role: 'status' }, 'No active sessions'),
    createElement('ol', { className: 'rm-timeline', 'aria-label': 'Timeline' },
      ...inbox.timeline.map(entry => createElement('li', {
        key: `${keyFor(entry.session)}/${entry.id}`,
        'data-timeline': entry.id, 'data-session': keyFor(entry.session),
      }, `${label[entry.session.client]} · ${entry.text}`)),
      ...inbox.deliveries.map(entry => createElement('li', {
        key: entry.id, 'data-delivery': entry.id, 'data-session': keyFor(entry.session), 'data-state': entry.state,
      }, `${label[entry.session.client]} · ${entry.state}`)),
    ),
    inbox.sessions.length ? createElement('form', { className: 'rm-compose', onSubmit: send },
      createElement('select', {
        'aria-label': 'Recipient', value: selected ? recipient : '',
        onChange: (event: Event) => setRecipient((event.target as HTMLSelectElement).value),
      }, createElement('option', { value: '' }, 'Choose a session'),
      ...inbox.sessions.map(({ key, title }) => createElement('option', {
        key: keyFor(key), value: keyFor(key),
      }, `${label[key.client]} · ${title}`))),
      createElement('input', {
        name: 'message', type: 'text', 'aria-label': 'Message', placeholder: 'Reply…', required: true,
      }),
      createElement('button', {
        type: 'submit', className: 'rm-collapse', disabled: !selected || Boolean(selected.sendBlocked) || pending,
        title: selected?.sendBlocked ?? 'Send',
      }, 'Send'),
    ) : null,
    error ? createElement('p', { role: 'alert' }, error) : null,
    confirmation ? createElement('p', { role: 'status', 'data-approval-confirmed': true }, 'Codex confirmed your decision.') : null,
    createElement('details', { className: 'rm-setup' },
      createElement('summary', null, 'Client setup'),
      ...inbox.adapters.map(adapter => createElement('p', {
        key: adapter.client, 'data-adapter': adapter.client, 'data-blocked': Boolean(adapter.reason),
      }, `${label[adapter.client]}: ${adapter.reason ? `${adapter.reason} ` : ''}${adapter.setup}`)),
    ),
  );
}
