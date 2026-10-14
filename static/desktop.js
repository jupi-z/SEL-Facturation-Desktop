/* Actions de documents intégrées à la fenêtre SEL.
   Dans un navigateur de développement, les liens PDF restent ordinaires. */
(() => {
  document.addEventListener('click', async (event) => {
    const anchor = event.target.closest('a[data-document-action]');
    if (!anchor || !window.pywebview || !window.pywebview.api) return;
    event.preventDefault();
    if (anchor.dataset.busy) return;
    const action = anchor.dataset.documentAction;
    const actions = {open: 'open_pdf', save: 'save_pdf', print: 'print_pdf'};
    if (!(action in actions)) return;
    anchor.dataset.busy = '1';
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
      if (!result.ok) throw new Error(result.message || 'Action impossible.');
    } catch (err) {
      alert('Erreur PDF : ' + (err.message || String(err)));
    } finally {
      anchor.textContent = previousText;
      delete anchor.dataset.busy;
    }
  });
})();
