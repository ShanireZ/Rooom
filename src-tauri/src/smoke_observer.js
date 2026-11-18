(() => {
  let observedRevision = -1;
  let observedConfirmation = false;
  const report = () => {
    const shell = document.querySelector('main[data-revision][data-surface]');
    if (!shell) return;
    const { revision, surface } = shell.dataset;
    const number = Number(revision);
    const approvalConfirmed = Boolean(shell.querySelector('[data-approval-confirmed]'));
    if (number < observedRevision || (number === observedRevision && approvalConfirmed === observedConfirmation)) return;

    observedRevision = number;
    observedConfirmation = approvalConfirmed;
    window.__TAURI_INTERNALS__.invoke('smoke_rendered', {
      revision: number,
      surface,
      contentVisible: Boolean(shell.querySelector(surface === 'panel' ? 'h1' : 'button')),
      warningVisible: Boolean(shell.querySelector('.rm-notice')),
      approvalConfirmed,
      inboxView: surface === 'strip' ? null : {
        sessions: [...shell.querySelectorAll('[data-inbox-session]')].map(e => e.dataset.inboxSession),
        requests: [...shell.querySelectorAll('[data-request]')].map(e => [e.dataset.session, e.dataset.request]),
        requestWorkspaces: [...shell.querySelectorAll('[data-request-workspace]')].map(workspace => {
          const card = workspace.closest('[data-request]');
          return [card.dataset.session, card.dataset.request, workspace.textContent];
        }),
        sessionGrants: [...shell.querySelectorAll('[data-request]')]
          .filter(e => e.querySelector('[data-decision="allowForSession"]'))
          .map(e => [e.dataset.session, e.dataset.request]),
        deliveries: [...shell.querySelectorAll('[data-delivery]')].map(e => [e.dataset.session, e.dataset.delivery, e.dataset.state]),
        timeline: [...shell.querySelectorAll('[data-timeline]')].map(e => [e.dataset.session, e.dataset.timeline]),
        blocked: [...shell.querySelectorAll('[data-blocked="true"]')].map(e => e.dataset.adapter),
      },
    }).catch(() => {});
  };
  new MutationObserver(report).observe(document.getElementById('root'), {
    attributes: true,
    childList: true,
    subtree: true,
  });
  report();
  let busy = false;
  setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      const action = await window.__TAURI_INTERNALS__.invoke('smoke_next_action');
      if (!action) return;
      const shell = document.querySelector('main');
      if (action.open) {
        shell.querySelector('.rm-strip')?.click();
      } else if (action.decision) {
        const card = [...shell.querySelectorAll('[data-request]')].find(
          e => e.dataset.session === action.target && e.dataset.request === action.request,
        );
        [...(card?.querySelectorAll('[data-decision]') ?? [])].find(
          e => e.dataset.decision === action.decision,
        )?.click();
      } else {
        const form = shell.querySelector('.rm-compose');
        const picker = form?.querySelector('select');
        if (!picker || ![...picker.options].some(o => o.value === action.target)) return;
        picker.value = action.target;
        picker.dispatchEvent(new Event('change', { bubbles: true }));
        await new Promise(resolve => setTimeout(resolve, 40));
        form.elements.namedItem('message').value = action.text;
        form.requestSubmit(form.querySelector('button'));
      }
    } catch {} finally {
      busy = false;
    }
  }, 100);
})();
