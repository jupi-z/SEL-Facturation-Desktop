(() => {
  "use strict";

  const state = { customers: [], invoices: [], payments: [], sales: [], sync: null };
  const $ = (selector) => document.querySelector(selector);
  const $$ = (selector) => [...document.querySelectorAll(selector)];

  function showNotice(message, error = false) {
    const notice = $("#notice");
    notice.textContent = message;
    notice.hidden = !message;
    notice.classList.toggle("error", error);
  }

  async function api(path, method = "GET", body = undefined) {
    if (!window.pywebview?.api?.request) {
      throw new Error("Le pont local SEL n’est pas encore disponible.");
    }
    return window.pywebview.api.request(path, method, body);
  }

  function customerName(id) {
    return state.customers.find((customer) => customer.id === id)?.legal_name || id;
  }

  function optionList(select, selected = "") {
    select.innerHTML = `<option value="">Sélectionner…</option>` + state.customers
      .map((customer) => `<option value="${customer.id}" ${customer.id === selected ? "selected" : ""}>${customer.customer_code} — ${customer.legal_name}</option>`)
      .join("");
  }

  function renderMetrics() {
    $("#metrics").innerHTML = [
      ["ABONNÉS", state.customers.length, "Enregistrés sur ce poste"],
      ["FACTURES", state.invoices.length, "Historique local"],
      ["PAIEMENTS", state.payments.length, "Encaissements conservés"],
      ["VENTES SOLAIRES", state.sales.length, "Opérations locales"],
    ].map(([label, value, note]) => `<div class="metric"><div class="metric-label">${label}</div><div class="metric-value">${value}</div><div class="metric-note">${note}</div></div>`).join("");
  }

  function renderInvoices() {
    const target = $("#recent-invoices");
    if (!state.invoices.length) { target.innerHTML = `<div class="empty-state">Aucune facture locale pour le moment.</div>`; return; }
    target.innerHTML = state.invoices.slice(0, 6).map((invoice) => `<div class="list-row"><span><strong>${invoice.invoice_number || invoice.invoice_id}</strong><br><small>${invoice.period || "Période non renseignée"}</small></span><span class="badge">${invoice.status}</span></div>`).join("");
  }

  function renderCustomers() {
    $("#customer-count").textContent = state.customers.length;
    $("#customers-list").innerHTML = state.customers.length ? `<table><thead><tr><th>Code</th><th>Nom</th><th>Adresse</th><th>Statut</th></tr></thead><tbody>${state.customers.map((customer) => `<tr><td><strong>${customer.customer_code}</strong></td><td>${customer.legal_name}</td><td>${customer.address || "—"}</td><td>${customer.status}</td></tr>`).join("")}</tbody></table>` : `<div class="empty-state">Aucun abonné enregistré.</div>`;
  }

  function renderPayments() {
    $("#payments-list").innerHTML = state.payments.length ? `<table><thead><tr><th>Date</th><th>Client</th><th>Montant</th><th>Mode</th><th>État</th></tr></thead><tbody>${state.payments.map((payment) => `<tr><td>${payment.paid_at}</td><td>${customerName(payment.customer_id)}</td><td><strong>${payment.amount_received} ${payment.currency}</strong></td><td>${payment.method}</td><td>${payment.status}</td></tr>`).join("")}</tbody></table>` : `<div class="empty-state">Aucun encaissement local.</div>`;
  }

  function renderSales() {
    $("#solar-list").innerHTML = state.sales.length ? `<table><thead><tr><th>Date</th><th>Client</th><th>Produit</th><th>Principal</th><th>État</th></tr></thead><tbody>${state.sales.map((sale) => `<tr><td>${sale.sold_at}</td><td>${customerName(sale.customer_id)}</td><td>${sale.product_code || sale.sale_type}</td><td><strong>${sale.principal} ${sale.currency}</strong></td><td>${sale.status}</td></tr>`).join("")}</tbody></table>` : `<div class="empty-state">Aucune vente solaire locale.</div>`;
  }

  function renderSync() {
    const sync = state.sync || {};
    const pending = sync.pending ?? 0;
    const status = sync.status || "LOCAL_ONLY";
    const text = pending ? `${pending} opération(s) en attente de confirmation centrale.` : "Aucune opération en attente de synchronisation.";
    $("#sync-summary").innerHTML = `<strong>${status}</strong><br>${text}`;
    $("#sync-details").innerHTML = `<strong>État local : ${status}</strong><br>${text}<br><small>Une confirmation locale n’est jamais présentée comme une validation centrale.</small>`;
    $("#connection-label").textContent = status === "LOCAL_ONLY" ? "Local · hors ligne" : "Service local prêt";
    $(".status-dot").classList.toggle("offline", Boolean(pending));
  }

  async function refresh() {
    try {
      const [customers, invoices, payments, sales, sync] = await Promise.all([
        api("/local/v1/customers"), api("/local/v1/invoices?limit=50"), api("/local/v1/payments?limit=50"), api("/local/v1/solar/sales?limit=50"), api("/local/v1/sync/status"),
      ]);
      state.customers = customers; state.invoices = invoices; state.payments = payments; state.sales = sales; state.sync = sync;
      renderMetrics(); renderInvoices(); renderCustomers(); renderPayments(); renderSales(); renderSync();
      optionList($("#invoice-customer")); optionList($("#payment-customer")); optionList($("#solar-customer"));
      showNotice("");
    } catch (error) { showNotice(error.message || "Impossible de lire les données locales.", true); }
  }

  async function submitForm(event, path, makePayload, success) {
    event.preventDefault();
    try { const body = makePayload(new FormData(event.target)); await api(path, "POST", body); showNotice(success); event.target.reset(); await refresh(); }
    catch (error) { showNotice(error.message || "Opération impossible.", true); }
  }

  function setupNavigation() {
    $$('[data-view]').forEach((button) => button.addEventListener("click", () => {
      const view = button.dataset.view;
      $$('[data-view]').forEach((item) => item.classList.toggle("active", item.dataset.view === view));
      $$('.view').forEach((item) => { item.hidden = item.id !== `view-${view}`; item.classList.toggle("active-view", item.id === `view-${view}`); });
      $("#page-title").textContent = button.textContent;
      if (view === "dashboard") refresh();
    }));
    $$('[data-action="refresh"]').forEach((button) => button.addEventListener("click", refresh));
    $$('[data-action="sync"]').forEach((button) => button.addEventListener("click", async () => { try { await api("/local/v1/sync/trigger", "POST", {}); showNotice("Synchronisation lancée. Les opérations restent contrôlées par le serveur central."); await refresh(); } catch (error) { showNotice(error.message, true); } }));
  }

  function setupForms() {
    $("#customer-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/customers", (data) => Object.fromEntries(data), "Abonné enregistré localement."));
    $("#invoice-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/billing/utility/issue", (data) => ({ customer_id: data.get("customer_id"), billing_period: data.get("billing_period"), electricity: { quantity: data.get("electricity_quantity"), unit: "KWH" }, water: { quantity: data.get("water_quantity"), unit: "M3" }, water_zero_confirmed: data.get("water_zero_confirmed") === "on", currency: "USD", tariff_revision: data.get("tariff_revision"), due_date: data.get("due_date") || null }), "Facture combinée enregistrée localement."));
    $("#payment-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/payments", (data) => ({ customer_id: data.get("customer_id"), received_amount: data.get("received_amount"), currency: data.get("currency"), method: data.get("method"), occurred_at: new Date().toISOString(), allocations: [{ receivable_id: data.get("receivable_id"), amount: data.get("received_amount") }] }), "Règlement enregistré localement."));
    $("#solar-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/solar/sales", (data) => ({ customer_id: data.get("customer_id"), product_code: data.get("product_code"), principal: data.get("principal"), currency: data.get("currency"), terms_revision: "SEL-V2" }), "Vente solaire enregistrée localement."));
    $("#water-quantity").addEventListener("input", (event) => { $("#water-zero-confirmed").required = Number(event.target.value || 0) === 0; });
  }

  window.addEventListener("pywebviewready", () => refresh());
  setupNavigation(); setupForms();
})();
