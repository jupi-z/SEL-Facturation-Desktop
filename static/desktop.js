/* Actions de documents intégrées à la fenêtre SEL.
   Dans un navigateur de développement, les liens PDF restent ordinaires. */
(() => {
  const openModal = (modal, trigger) => {
    if (!modal) return;
    modal._lastTrigger = trigger;
    if (typeof modal.showModal === 'function') modal.showModal();
    else modal.setAttribute('open', '');
    const first = modal.querySelector('input, select, textarea, button');
    if (first) window.setTimeout(() => first.focus(), 0);
  };

  const closeModal = (modal) => {
    if (!modal) return;
    if (typeof modal.close === 'function') modal.close();
    else modal.removeAttribute('open');
    if (modal._lastTrigger) modal._lastTrigger.focus();
  };

  document.addEventListener('click', (event) => {
    const opener = event.target.closest('[data-modal-open]');
    if (opener) {
      const modal = document.getElementById(opener.dataset.modalOpen);
      if (modal) {
        event.preventDefault();
        openModal(modal, opener);
        return;
      }
    }
    const closer = event.target.closest('[data-modal-close]');
    if (closer) {
      event.preventDefault();
      closeModal(closer.closest('dialog'));
      return;
    }
    const dialog = event.target.closest('dialog.modal');
    if (dialog && event.target === dialog) closeModal(dialog);
  });

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      const openDialog = document.querySelector('dialog.modal[open]');
      if (openDialog) {
        event.preventDefault();
        closeModal(openDialog);
      }
    }
  });

  document.addEventListener('click', (event) => {
    const toggle = event.target.closest('[data-password-toggle]');
    if (!toggle) return;
    const input = document.getElementById(toggle.dataset.passwordToggle);
    if (!input) return;
    const visible = input.type === 'text';
    input.type = visible ? 'password' : 'text';
    toggle.textContent = visible ? 'Afficher' : 'Masquer';
    toggle.setAttribute('aria-label', visible ? 'Afficher le mot de passe' : 'Masquer le mot de passe');
  });

  document.addEventListener('submit', (event) => {
    const form = event.target.closest('form[data-confirm]');
    if (form && !window.confirm(form.dataset.confirm)) event.preventDefault();
  });

  document.addEventListener('click', async (event) => {
    const anchor = event.target.closest('a[data-document-action]');
    if (!anchor || !window.pywebview || !window.pywebview.api) return;
    event.preventDefault();
    if (anchor.dataset.busy) return;
    const action = anchor.dataset.documentAction;
    const actions = {open: 'open_pdf', save: 'save_pdf', print: 'print_pdf'};
    if (!(action in actions)) return;
    anchor.dataset.busy = '1';
    anchor.setAttribute('aria-busy', 'true');
    const previousText = anchor.textContent;
    anchor.textContent = 'Préparation du document…';
    try {
      const url = anchor.dataset.documentUrl;
      const response = await fetch(url, {credentials: 'same-origin', redirect: 'error'});
      if (!response.ok || !(response.headers.get('content-type') || '').includes('application/pdf')) {
        throw new Error('Document indisponible. Reconnectez-vous et réessayez.');
      }
      const blob = await response.blob();
      if (blob.size > 6 * 1024 * 1024) throw new Error('PDF trop volumineux.');
      const encoded = await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onerror = () => reject(new Error('Impossible de lire le PDF.'));
        reader.onload = () => resolve(String(reader.result).split(',')[1]);
        reader.readAsDataURL(blob);
      });
      const pdfName = anchor.dataset.documentName;
      const result = await window.pywebview.api[actions[action]](pdfName, encoded);
      if (result.fallback === 'open') {
        alert(result.message || 'Impression automatique indisponible. Le PDF est ouvert : utilisez Ctrl+P.');
        return;
      }
      if (!result.ok) throw new Error(result.message || 'Action impossible.');
    } catch (err) {
      alert('Erreur PDF : ' + (err.message || String(err)));
    } finally {
      anchor.textContent = previousText;
      delete anchor.dataset.busy;
      anchor.removeAttribute('aria-busy');
    }
  });
})();
