import type { Register } from 'claude-code';

const channel: string = __ROOOM_CHANNEL_DIRECTORY__;
const engine = '2.1.325';
const validId = (value: string) => /^[A-Za-z0-9_-]{1,128}$/.test(value);

export const register: Register = on => {
  let ended = false;
  on('session.end', async (_$, event, next) => {
    ended = true;
    return await next(event);
  });
  on('session.start', async ($, event, next) => {
    const started = await next(event);
    const { version } = await $.session.version();
    if (version !== engine) return started;

    let scanning = false;
    const submitted = new Set<string>();
    $.clock.every(1000, () => {
      if (scanning || ended) return;
      scanning = true;
      void (async () => {
        try {
          const sessionId = await $.session.id();
          if (!validId(sessionId)) return;
          const at = await $.clock.now();
          await $.fs.write(`${channel}/check-ins/${sessionId}.json`,
            JSON.stringify({ sessionId, at, version, channelVersion: 1 }));

          const folder = `${channel}/pending/${sessionId}`;
          if (!(await $.fs.exists(folder))) return;
          for (const entry of await $.fs.list(folder)) {
            if (entry.kind !== 'file' || !entry.name.endsWith('.json')) continue;
            const id = entry.name.slice(0, -5);
            if (!validId(id) || submitted.has(id)) continue;
            const receipt = `${channel}/receipts/${sessionId}/${id}.json`;
            if (await $.fs.exists(receipt)) continue;
            const item = JSON.parse(await $.fs.read(`${folder}/${entry.name}`));
            if (item.id !== id || item.sessionId !== sessionId ||
                typeof item.text !== 'string' || !item.text.trim() ||
                !Number.isSafeInteger(item.expiresAt) ||
                item.expiresAt <= await $.clock.now() || ended ||
                await $.session.id() !== sessionId) continue;

            const record = (state: 'taken' | 'started' | 'refused') =>
              $.fs.write(receipt, JSON.stringify({ sessionId, id, state }));
            await record('taken');
            submitted.add(id);
            if (ended || item.expiresAt <= await $.clock.now() ||
                await $.session.id() !== sessionId) {
              await record('refused');
              continue;
            }
            // The API waits for idle. Keep polling independently of submission.
            void $.prompt.submit({ text: item.text, asUser: true }).then(
              result => record((result as { drop?: unknown }).drop === undefined
                ? 'started' : 'refused'),
              () => record('refused'),
            ).catch(() => {});
          }
        } catch {
          // Records may disappear between listing and reading.
        } finally {
          scanning = false;
        }
      })();
    });
    return started;
  });
};
