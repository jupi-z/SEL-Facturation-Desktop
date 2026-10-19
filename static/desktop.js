/* Actions de documents intégrées à la fenêtre SEL.
   Dans un navigateur de développement, les liens PDF restent ordinaires. */
(() => {
  // Les liens externes ne doivent jamais remplacer l'interface SEL dans la
  // WebView2. En développement, sans pont pywebview, les liens restent natifs
  // au navigateur utilisé pour servir l'application.
  document.addEventListener('click', async (event) => {
    const link = event.target.closest('a[data-external-link]');
    if (!link || !window.pywebview) return;
    event.preventDefault();
    const openExternal = window.pywebview.api && window.pywebview.api.open_external_url;
    if (!openExternal) {
      alert('Le navigateur externe n’est pas disponible. Réessayez dans un instant.');
      return;
    }
    if (link.dataset.busy) return;
    link.dataset.busy = '1';
    try {
      const result = await openExternal(link.href);
      if (!result.ok) throw new Error(result.message || 'Impossible d’ouvrir le lien externe.');
    } catch (error) {
      alert('Lien externe : ' + (error.message || String(error)));
    } finally {
      delete link.dataset.busy;
    }
  });

  const openModal = (modal, trigger) => {
    if (!modal) return;
    modal._lastTrigger = trigger;
    if (!modal._focusTrapReady) {
      modal.addEventListener('keydown', (event) => {
        if (event.key !== 'Tab') return;
        const focusable = Array.from(modal.querySelectorAll(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        )).filter((element) => element.getClientRects().length > 0);
        if (!focusable.length) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      });
      modal._focusTrapReady = true;
    }
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
    const form = event.target.closest('form');
    if (!form) return;
    if (form.getAttribute('data-confirm') && !window.confirm(form.getAttribute('data-confirm'))) {
      event.preventDefault();
      return;
    }
    // La confirmation est passée : la saisie a été acceptée par l'utilisateur.
    // La copie locale peut être supprimée avant la navigation ; les erreurs
    // serveur sont, elles, sauvegardées dans la session Flask.
    if (form.dataset.draftKey) {
      try { window.sessionStorage.removeItem(`sel.form-draft.${form.dataset.draftKey}`); } catch (_error) { /* best effort */ }
    }
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

  document.addEventListener('click', async (event) => {
    const trigger = event.target.closest('[data-update-install]');
    if (!trigger) return;
    event.preventDefault();
    if (!window.pywebview || !window.pywebview.api) {
      alert('Cette action nécessite la fenêtre Windows native.');
      return;
    }
    if (!window.confirm('Une sauvegarde vérifiée est prête. Lancer maintenant l’installation ?')) return;
    trigger.disabled = true;
    try {
      const result = await window.pywebview.api.install_update(trigger.dataset.updateFile, trigger.dataset.updateSha256);
      if (!result.ok) throw new Error(result.message || 'Impossible de lancer la mise à jour.');
      trigger.textContent = 'Installation lancée…';
    } catch (error) {
      trigger.disabled = false;
      alert('Mise à jour : ' + (error.message || String(error)));
    }
  });

  document.querySelectorAll('dialog[data-modal-autopen="true"]').forEach((modal) => openModal(modal, null));
})();

// Conservation temporaire des saisies non envoyées.
// sessionStorage couvre les actualisations dans la session courante sans
// persister de données sensibles sur le disque. Les formulaires de connexion
// et d'initialisation ne portent volontairement pas data-draft-key.
(() => {
  const storagePrefix = 'sel.form-draft.';
  const excludedTypes = new Set(['password', 'file', 'submit', 'button', 'reset']);

  const storage = (() => {
    try {
      const probe = `${storagePrefix}probe`;
      window.sessionStorage.setItem(probe, '1');
      window.sessionStorage.removeItem(probe);
      return window.sessionStorage;
    } catch (_error) {
      return null;
    }
  })();

  if (!storage) return;

  const keyFor = (form) => `${storagePrefix}${form.dataset.draftKey}`;

  const fieldsFor = (form) => Array.from(form.elements).filter((field) => {
    const type = (field.type || '').toLowerCase();
    return field.name && field.name !== '_csrf' && type !== 'hidden' && !excludedTypes.has(type) && !field.disabled;
  });

  const serialize = (form) => fieldsFor(form).map((field) => ({
    name: field.name,
    type: (field.type || '').toLowerCase(),
    value: field.value,
    checked: field.type === 'checkbox' || field.type === 'radio' ? field.checked : undefined,
  }));

  const save = (form) => {
    try {
      storage.setItem(keyFor(form), JSON.stringify(serialize(form)));
    } catch (_error) {
      // Le formulaire reste pleinement utilisable si le stockage est plein ou bloqué.
    }
  };

  const restore = (form) => {
    let saved;
    try {
      saved = JSON.parse(storage.getItem(keyFor(form)) || 'null');
    } catch (_error) {
      saved = null;
    }
    if (!Array.isArray(saved)) return;

    const byName = new Map();
    fieldsFor(form).forEach((field) => {
      if (!byName.has(field.name)) byName.set(field.name, []);
      byName.get(field.name).push(field);
    });
    saved.forEach((item) => {
      const fields = byName.get(item.name) || [];
      const field = fields.find((candidate) => (candidate.type || '').toLowerCase() === item.type) || fields[0];
      if (!field) return;
      if (field.type === 'checkbox' || field.type === 'radio') field.checked = Boolean(item.checked);
      else field.value = item.value ?? '';
    });
  };

  document.querySelectorAll('form[data-draft-key]').forEach((form) => {
    const key = form.dataset.draftKey;
    if (!key) return;

    // Une valeur reconstruite par le serveur après une erreur est prioritaire
    // sur une ancienne copie locale.
    if (form.dataset.draftServer === 'true') storage.removeItem(keyFor(form));
    else restore(form);

    form.addEventListener('input', () => save(form));
    form.addEventListener('change', () => save(form));
    form.addEventListener('reset', () => storage.removeItem(keyFor(form)));
  });
})();

// Navigation clavier et persistance légère de l'onglet Paramètres.
(() => {
  const root = document.querySelector('[data-settings-root]');
  if (!root) return;

  const tabs = Array.from(root.querySelectorAll('[data-settings-tab]'));
  const panels = new Map(
    tabs.map((tab) => [tab.dataset.settingsTab, document.getElementById(tab.getAttribute('aria-controls'))])
  );
  const ids = tabs.map((tab) => tab.dataset.settingsTab);
  const knownIds = new Set(ids);

  const activate = (id, {focus = false, updateHash = true} = {}) => {
    const selected = knownIds.has(id) ? id : (root.dataset.initialTab || ids[0]);
    tabs.forEach((tab) => {
      const isSelected = tab.dataset.settingsTab === selected;
      tab.setAttribute('aria-selected', String(isSelected));
      tab.tabIndex = isSelected ? 0 : -1;
    });
    panels.forEach((panel, panelId) => {
      if (!panel) return;
      panel.hidden = panelId !== selected;
    });
    if (updateHash && window.history && window.history.replaceState) {
      window.history.replaceState(null, '', `#${selected}`);
    }
    if (focus) {
      const tab = tabs.find((item) => item.dataset.settingsTab === selected);
      if (tab) tab.focus();
    }
  };

  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => activate(tab.dataset.settingsTab));
    tab.addEventListener('keydown', (event) => {
      if (!['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      let nextIndex = index;
      if (event.key === 'ArrowRight' || event.key === 'ArrowDown') nextIndex = (index + 1) % tabs.length;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') nextIndex = (index - 1 + tabs.length) % tabs.length;
      if (event.key === 'Home') nextIndex = 0;
      if (event.key === 'End') nextIndex = tabs.length - 1;
      activate(tabs[nextIndex].dataset.settingsTab, {focus: true});
    });
  });

  const hashId = window.location.hash.slice(1);
  activate(knownIds.has(hashId) ? hashId : (root.dataset.initialTab || ids[0]), {updateHash: false});
  window.addEventListener('hashchange', () => {
    const nextId = window.location.hash.slice(1);
    if (knownIds.has(nextId)) activate(nextId, {updateHash: false});
  });
})();
