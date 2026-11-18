window.__ModuleLoader__.load({
  id: 'rooom-receiver',
  factory() {
    return {
      name: 'rooom-receiver-client',
      inject: ['uiWorkspace', 'sessions', 'workspaces'],
      apply(ctx) {
        if (location.protocol !== 'dsh-app:' || location.hostname !== 'app'
            || !window.dshDesktopBoot) return;
        const navigation = ctx.uiWorkspace;
        const sessions = ctx.sessions.list;
        const workspaces = ctx.workspaces.list;
        const endpoint = '/rooom/open-session';
        ctx.effect(() => {
          const lifetime = new AbortController();
          const options = {
            headers: { 'x-rooom-navigation': '1' },
            cache: 'no-store',
            signal: lifetime.signal,
          };
          let selectedNonce;
          async function receive() {
            while (!lifetime.signal.aborted) {
              try {
                const response = await fetch(endpoint, options);
                if (response.status === 204) continue;
                if (response.status === 401 || response.status === 403) break;
                if (!response.ok) throw new Error('Harness navigation is temporarily unavailable');
                const { sessionId, nonce, expiresAt } = await response.json();
                if (Date.now() >= expiresAt) continue;
                const catalog = sessions.getSnapshot();
                if (catalog.phase !== 'ready' || !catalog.byId[sessionId]
                    || workspaces.getSnapshot().archivedSessionIds.includes(sessionId)) {
                  await new Promise(resolve => setTimeout(resolve, 125));
                  continue;
                }
                if (selectedNonce !== nonce) {
                  navigation.openSession(sessionId);
                  if (navigation.selection.getSnapshot().sessionId !== sessionId) continue;
                  selectedNonce = nonce;
                }
                if (navigation.selection.getSnapshot().sessionId === sessionId) {
                  await fetch(`${endpoint}?nonce=${encodeURIComponent(nonce)}`, {
                    ...options, method: 'POST',
                  });
                }
              } catch {
                if (!lifetime.signal.aborted) await new Promise(resolve => setTimeout(resolve, 500));
              }
            }
          }
          void receive();
          return () => lifetime.abort();
        });
      },
    };
  },
});
