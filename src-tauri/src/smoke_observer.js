(() => {
  let observedRevision = -1;
  const report = () => {
    const shell = document.querySelector('main[data-revision][data-surface]');
    if (!shell) return;
    const { revision, surface } = shell.dataset;
    const number = Number(revision);
    if (number <= observedRevision) return;

    observedRevision = number;
    window.__TAURI_INTERNALS__.invoke('smoke_rendered', {
      revision: number,
      surface,
      contentVisible: Boolean(shell.querySelector(surface === 'panel' ? 'h1' : 'button')),
      warningVisible: Boolean(shell.querySelector('.rm-notice')),
    }).catch((error) => console.error('Rooom smoke receipt failed', error));
  };
  new MutationObserver(report).observe(document.getElementById('root'), {
    attributes: true,
    childList: true,
    subtree: true,
  });
  report();
})();
