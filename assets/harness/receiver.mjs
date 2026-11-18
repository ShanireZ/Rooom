import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { setTimeout as wait } from 'node:timers/promises';

export const name = 'rooom-receiver';
export const inject = ['agents'];

const version = '0.2.2-rc.3';
const folders = ['events', 'answers', 'decided', 'prompts', 'receipts', 'navigation'];
const id = value => typeof value === 'string' && /^[A-Za-z0-9_-]{1,80}$/.test(value)
  && !/^(CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])$/i.test(value);
const rootAgent = agent => id(agent?.id) && !agent.parent;

function read(file) {
  const info = fs.lstatSync(file);
  if (!info.isFile() || info.size > 256 * 1024) throw new Error('Invalid channel record');
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function publish(file, record) {
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    fs.writeFileSync(temp, JSON.stringify(record), { flag: 'wx' });
    fs.renameSync(temp, file);
  } finally {
    fs.rmSync(temp, { force: true });
  }
}

function installedVersion() {
  const resources = path.join(path.dirname(process.execPath), 'resources');
  return read(path.join(resources, 'app.asar', 'package.json')).version;
}

function registerNavigation(ctx, { held, available, file, read, publish, id }) {
  let seenAt = 0;
  const renderer = () => Date.now() - seenAt < 5_000
    ? { pid: process.ppid, at: seenAt }
    : undefined;

  function waiting() {
    for (const key of held) {
      const [sessionId, callId] = key.split('/');
      if (!available(sessionId, callId)) continue;
      const location = file(sessionId, 'navigation', callId);
      try {
        const request = read(location);
        if (request.sessionId === sessionId && request.callId === callId
            && request.hostPid === process.pid && id(request.nonce) && request.opened === false
            && Number.isSafeInteger(request.expiresAt) && Date.now() < request.expiresAt) {
          return { location, request };
        }
      } catch {}
    }
  }

  ctx.inject(['connection', 'webServer'], scope => {
    const { connection, webServer } = scope;
    if (webServer.host !== '127.0.0.1') return;
    scope.effect(() => webServer.register({
      kind: 'exact',
      path: '/rooom/open-session',
      async handler(req, res) {
        if (req.socket.remoteAddress !== '127.0.0.1'
            || req.headers['x-rooom-navigation'] !== '1'
            || connection.admit(req).peer !== connection.browserPeer) {
          res.writeHead(403).end();
          return;
        }
        res.setHeader('cache-control', 'no-store');
        if (req.method === 'GET') {
          seenAt = Date.now();
          // A renderer can keep receiving while Electron throttles background timers.
          const deadline = Date.now() + 1_000;
          do {
            const item = waiting();
            if (item) {
              const { sessionId, nonce, expiresAt } = item.request;
              res.writeHead(200, { 'content-type': 'application/json' })
                .end(JSON.stringify({ sessionId, nonce, expiresAt }));
              return;
            }
            await wait(125);
          } while (!res.destroyed && Date.now() < deadline);
          if (!res.destroyed) res.writeHead(204).end();
          return;
        }
        if (req.method === 'POST') {
          const nonce = new URL(req.url ?? '/', 'http://localhost').searchParams.get('nonce');
          const item = waiting();
          if (!item || item.request.nonce !== nonce) {
            res.writeHead(409).end();
            return;
          }
          publish(item.location, { ...item.request, opened: true });
          res.writeHead(204).end();
          return;
        }
        res.writeHead(405).end();
      },
    }));
  });
  return renderer;
}

export function apply(ctx, { channel, session } = {}) {
  try {
    if (typeof channel !== 'string' || !path.isAbsolute(channel)
        || (session !== undefined && !id(session))
        || !fs.lstatSync(path.join(channel, 'sessions')).isDirectory()
        || installedVersion() !== version) return;
  } catch {
    return;
  }
  const pending = new Map();
  const held = new Set();
  let active = true;
  let scanning = false;

  const eligible = agent => rootAgent(agent) && (session === undefined || session === agent.id);
  const directory = sid => path.join(channel, 'sessions', sid);
  const file = (sid, kind, key) => path.join(directory(sid), kind, `${key}.json`);
  const agentId = agent => ({ id: agent.id, parent: null });

  function residentReady(sid) {
    try {
      const { at, hostPid } = read(path.join(directory(sid), 'resident.json'));
      return hostPid === process.pid && Number.isSafeInteger(at)
        && at <= Date.now() + 1000 && Date.now() - at < 5000;
    } catch {
      return false;
    }
  }

  const renderer = registerNavigation(ctx, {
    held, file, read, publish, id,
    available: (sid, callId) => active && residentReady(sid)
      && !fs.existsSync(file(sid, 'answers', callId))
      && !fs.existsSync(file(sid, 'decided', callId)),
  });

  function checkIn(agent) {
    const dir = directory(agent.id);
    fs.mkdirSync(dir, { recursive: true });
    if (!fs.lstatSync(dir).isDirectory()) throw new Error('Invalid session folder');
    for (const folder of folders) {
      const child = path.join(dir, folder);
      fs.mkdirSync(child, { recursive: true });
      if (!fs.lstatSync(child).isDirectory()) throw new Error('Invalid channel folder');
    }
    publish(path.join(dir, 'check-in.json'), {
      desktopVersion: version, channelVersion: 1, agent: agentId(agent),
      at: Date.now(), title: 'DeepSeek Harness', pid: process.pid, renderer: renderer(),
    });
  }

  const receipt = (sid, requestId, stage) => publish(file(sid, 'receipts', requestId), {
    sessionId: sid, requestId, stage,
  });

  async function receive(agent) {
    const sid = agent.id;
    const outstanding = pending.get(sid);
    if (outstanding && outstanding.expiresAt > Date.now()) return;
    pending.delete(sid);
    const controller = ctx.get('sessionController');
    if (typeof controller?.prompt !== 'function') return;
    for (const entry of fs.readdirSync(path.join(directory(sid), 'prompts'), { withFileTypes: true })) {
      if (!entry.isFile() || !entry.name.endsWith('.json')) continue;
      const key = entry.name.slice(0, -5);
      if (!id(key) || fs.existsSync(file(sid, 'receipts', key))) continue;
      const packet = file(sid, 'prompts', key);
      const value = read(packet);
      if (value.requestId !== key || value.sessionId !== sid || value.mode !== 'queue'
          || !Number.isSafeInteger(value.expiresAt) || value.expiresAt <= Date.now()
          || !Array.isArray(value.content) || value.content.length !== 1
          || value.content[0]?.type !== 'text' || typeof value.content[0].text !== 'string'
          || !value.content[0].text.trim()) continue;
      // Only the process that removes the packet can submit it. Pending proof
      // carries the RPC ID and expiry in memory.
      fs.unlinkSync(packet);
      const flight = { key, expiresAt: value.expiresAt, started: false, accepted: false };
      pending.set(sid, flight);
      const answer = await controller.prompt({
        sessionId: sid, requestId: key, mode: 'queue', content: value.content,
      }, AbortSignal.timeout(5000));
      if (answer?.accepted === false) {
        receipt(sid, key, 'refused');
        pending.delete(sid);
      } else if (answer?.accepted === true) {
        flight.accepted = true;
        receipt(sid, key, flight.started ? 'started' : 'accepted');
        if (flight.started) pending.delete(sid);
      }
      return;
    }
  }

  ctx.on('agent/pre-step', (event, next) => {
    try {
      const agent = event.agent;
      const flight = eligible(agent) ? pending.get(agent.id) : undefined;
      if (flight && flight.expiresAt > Date.now() && event.messages.some(message =>
        message.role === 'user' && message.source?.rpcId === flight.key)) {
        flight.started = true;
        if (flight.accepted) {
          receipt(agent.id, flight.key, 'started');
          pending.delete(agent.id);
        }
      }
    } catch {}
    return next();
  }, { prepend: true });

  ctx.on('approval/request', async (request, next) => {
    const { agent, callId, toolName, reason, signal } = request;
    if (!eligible(agent) || !id(callId) || !residentReady(agent.id)) return next();
    const hold = `${agent.id}/${callId}`;
    if (held.has(hold)) return next();
    const key = file(agent.id, 'events', callId);
    const staged = file(agent.id, 'answers', callId);
    held.add(hold);
    try {
      checkIn(agent);
      const at = Date.now();
      const header = agent.session?.header;
      const cwd = header?.id === agent.id ? header.cwd : undefined;
      publish(key, {
        event: 'approval/request', agent: agentId(agent), callId, at,
        ...(typeof cwd === 'string' && path.isAbsolute(cwd) ? { cwd } : {}),
        toolName: String(toolName || 'Tool approval').slice(0, 512),
        reason: String(reason || 'Harness is waiting for permission.').slice(0, 1024),
      });
      while (active && !signal?.aborted && Date.now() - at < 600_000 && residentReady(agent.id)) {
        if (fs.existsSync(staged)) {
          const answer = read(staged);
          if (answer.sessionId !== agent.id || answer.callId !== callId) break;
          const decision = { 'allowed-once': 'allowed-once', denied: 'rejected' }[answer.outcome];
          if (!['allowed-once', 'rejected'].includes(decision)) break;
          fs.unlinkSync(staged);
          publish(file(agent.id, 'decided', callId), {
            sessionId: agent.id, callId, outcome: decision,
          });
          return decision;
        }
        await wait(100);
      }
    } catch {
      // A failed transport passes the approval back to the native stack.
    } finally {
      held.delete(hold);
      for (const item of [key, staged]) {
        try { fs.rmSync(item, { force: true }); } catch {}
      }
    }
    return next();
  }, { prepend: true });

  ctx.on('agent/disposed', event => {
    try {
      if (eligible(event.agent)) {
        fs.rmSync(path.join(directory(event.agent.id), 'check-in.json'), { force: true });
        pending.delete(event.agent.id);
      }
    } catch {}
  });

  const timer = setInterval(() => {
    if (!active || scanning) return;
    scanning = true;
    void (async () => {
      try {
        const open = ctx.agents.list().filter(eligible);
        for (const agent of open) {
          try {
            checkIn(agent);
            await receive(agent);
          } catch {}
        }
      } catch {} finally {
        scanning = false;
      }
    })();
  }, 1000);
  ctx.effect(() => () => { active = false; clearInterval(timer); });
}
