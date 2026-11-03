(() => {
  "use strict";

  const state = { customers: [], invoices: [], payments: [], sales: [], sync: null, preview: null, receivablesRequestId: 0 };
  const $ = (selector) => document.querySelector(selector);
  const $$ = (selector) => [...document.querySelectorAll(selector)];

  function clear(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  function valueOf(value, fallback = "—") {
    if (value === undefined || value === null || String(value) === "") return fallback;
    return String(value);
  }

  function textNode(value, fallback = "—") {
    return document.createTextNode(valueOf(value, fallback));
  }

  function element(tag, className, content) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (content !== undefined) node.appendChild(textNode(content));
    return node;
  }

  function cell(tag, content, strong = false) {
    const node = document.createElement(tag);
    if (strong) {
      const emphasis = document.createElement("strong");
      emphasis.appendChild(textNode(content));
      node.appendChild(emphasis);
    } else {
      node.appendChild(textNode(content));
    }
    return node;
  }

  function table(headers, rows) {
    const tableNode = document.createElement("table");
    const head = document.createElement("thead");
    const headRow = document.createElement("tr");
    headers.forEach((header) => headRow.appendChild(cell("th", header)));
    head.appendChild(headRow);
    tableNode.appendChild(head);
    const body = document.createElement("tbody");
    rows.forEach((row) => {
      const rowNode = document.createElement("tr");
      row.forEach((value) => {
        if (value instanceof Node) rowNode.appendChild(value);
        else rowNode.appendChild(cell("td", value));
      });
      body.appendChild(rowNode);
    });
    tableNode.appendChild(body);
    return tableNode;
  }

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
    clear(select);
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.appendChild(textNode("Sélectionner…"));
    select.appendChild(placeholder);
    state.customers.forEach((customer) => {
      const option = document.createElement("option");
      option.value = customer.id;
      option.selected = customer.id === selected;
      option.appendChild(textNode(`${valueOf(customer.customer_code)} — ${valueOf(customer.legal_name)}`));
      select.appendChild(option);
    });
  }

  async function populateReceivables(customerId) {
    const select = $("#payment-receivable");
    const requestId = state.receivablesRequestId + 1;
    state.receivablesRequestId = requestId;
    clear(select);
    select.disabled = !customerId;
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.appendChild(textNode(customerId ? "Chargement des créances…" : "Sélectionnez d’abord un client"));
    select.appendChild(placeholder);
    if (!customerId) return;
    try {
      const statement = await api(`/local/v1/customers/${encodeURIComponent(customerId)}/statement`);
      if (requestId !== state.receivablesRequestId) return;
      const available = (Array.isArray(statement.receivables) ? statement.receivables : []).filter((receivable) => Number.parseFloat(valueOf(receivable.balance, "0")) > 0 && ["OPEN", "OVERDUE"].includes(receivable.status));
      clear(select);
      if (!available.length) {
        const empty = document.createElement("option");
        empty.value = "";
        empty.appendChild(textNode("Aucune créance ouverte pour ce client"));
        select.appendChild(empty);
        select.disabled = true;
        return;
      }
      const prompt = document.createElement("option");
      prompt.value = "";
      prompt.appendChild(textNode("Sélectionner une créance…"));
      select.appendChild(prompt);
      available.forEach((receivable) => {
        const option = document.createElement("option");
        option.value = valueOf(receivable.id, "");
        option.appendChild(textNode(`${valueOf(receivable.status)} · solde ${valueOf(receivable.balance)} ${valueOf(receivable.currency)}`));
        select.appendChild(option);
      });
      select.disabled = false;
    } catch (error) {
      if (requestId !== state.receivablesRequestId) return;
      clear(select);
      const failed = document.createElement("option");
      failed.value = "";
      failed.appendChild(textNode("Créances indisponibles"));
      select.appendChild(failed);
      select.disabled = true;
      showNotice(error.message || "Impossible de charger les créances.", true);
    }
  }

  function renderMetrics() {
    const target = $("#metrics");
    clear(target);
    [["ABONNÉS", state.customers.length, "Enregistrés sur ce poste"], ["FACTURES", state.invoices.length, "Historique local"], ["PAIEMENTS", state.payments.length, "Encaissements conservés"], ["VENTES SOLAIRES", state.sales.length, "Opérations locales"]].forEach(([label, value, note]) => {
      const metric = element("div", "metric");
      metric.appendChild(element("div", "metric-label", label));
      metric.appendChild(element("div", "metric-value", value));
      metric.appendChild(element("div", "metric-note", note));
      target.appendChild(metric);
    });
  }

  function renderInvoices() {
    const target = $("#recent-invoices");
    clear(target);
    if (!state.invoices.length) {
      target.appendChild(element("div", "empty-state", "Aucune facture locale pour le moment."));
      return;
    }
    state.invoices.slice(0, 6).forEach((invoice) => {
      const row = element("div", "list-row");
      const summary = document.createElement("span");
      summary.appendChild(element("strong", "", invoice.invoice_number || invoice.invoice_id));
      summary.appendChild(document.createElement("br"));
      summary.appendChild(element("small", "", invoice.period || "Période non renseignée"));
      row.appendChild(summary);
      row.appendChild(element("span", "badge", invoice.status));
      target.appendChild(row);
    });
  }

  function renderCustomers() {
    $("#customer-count").textContent = state.customers.length;
    const target = $("#customers-list");
    clear(target);
    if (!state.customers.length) {
      target.appendChild(element("div", "empty-state", "Aucun abonné enregistré."));
      return;
    }
    target.appendChild(table(["Code", "Nom", "Adresse", "Statut"], state.customers.map((customer) => [
      cell("td", customer.customer_code, true),
      valueOf(customer.legal_name),
      customer.address || "—",
      customer.status,
    ])));
  }

  function renderPayments() {
    const target = $("#payments-list");
    clear(target);
    if (!state.payments.length) {
      target.appendChild(element("div", "empty-state", "Aucun encaissement local."));
      return;
    }
    target.appendChild(table(["Date", "Client", "Montant", "Mode", "État"], state.payments.map((payment) => [
      payment.paid_at,
      customerName(payment.customer_id),
      cell("td", `${valueOf(payment.amount_received)} ${valueOf(payment.currency)}`, true),
      payment.method,
      payment.status,
    ])));
  }

  function renderSales() {
    const target = $("#solar-list");
    clear(target);
    if (!state.sales.length) {
      target.appendChild(element("div", "empty-state", "Aucune vente solaire locale."));
      return;
    }
    target.appendChild(table(["Date", "Client", "Produit", "Principal", "État"], state.sales.map((sale) => [
      sale.sold_at,
      customerName(sale.customer_id),
      sale.product_code || sale.sale_type,
      cell("td", `${valueOf(sale.principal)} ${valueOf(sale.currency)}`, true),
      sale.status,
    ])));
  }

  function renderSync() {
    const sync = state.sync || {};
    const pending = sync.pending ?? 0;
    const status = sync.status || "LOCAL_ONLY";
    const summary = pending ? `${pending} opération(s) en attente de confirmation centrale.` : "Aucune opération en attente de synchronisation.";
    const summaryTarget = $("#sync-summary");
    const detailsTarget = $("#sync-details");
    clear(summaryTarget);
    clear(detailsTarget);
    summaryTarget.appendChild(element("strong", "", status));
    summaryTarget.appendChild(document.createElement("br"));
    summaryTarget.appendChild(textNode(summary));
    detailsTarget.appendChild(element("strong", "", `État local : ${status}`));
    detailsTarget.appendChild(document.createElement("br"));
    detailsTarget.appendChild(textNode(summary));
    detailsTarget.appendChild(document.createElement("br"));
    detailsTarget.appendChild(element("small", "", "Une confirmation locale n’est jamais présentée comme une validation centrale."));
    $("#connection-label").textContent = status === "LOCAL_ONLY" ? "Local · hors ligne" : "Service local prêt";
    $(".status-dot").classList.toggle("offline", Boolean(pending));
  }

  function renderPreview(response) {
    const target = $("#invoice-result");
    clear(target);
    target.hidden = false;
    const heading = element("div", "panel-heading");
    heading.appendChild(element("h3", "", "Aperçu de la facture"));
    heading.appendChild(element("span", "badge", response.status));
    target.appendChild(heading);
    target.appendChild(element("p", "muted", "Vérifiez les deux services, la TVA et le total avant l’émission définitive."));
    target.appendChild(table(["Service", "Quantité", "Prix unitaire", "Sous-total", "TVA", "Total"], response.lines.map((line) => [
      line.service_code,
      `${valueOf(line.quantity)} ${valueOf(line.unit)}`,
      line.unit_price === null || line.unit_price === undefined ? "0.00" : line.unit_price,
      line.subtotal,
      line.tax,
      cell("td", `${valueOf(line.total)} ${valueOf(response.currency)}`, true),
    ])));
    const totals = element("div", "preview-totals");
    totals.appendChild(element("span", "", `Sous-total : ${valueOf(response.subtotal)} ${valueOf(response.currency)}`));
    totals.appendChild(element("span", "", `TVA : ${valueOf(response.tax)} ${valueOf(response.currency)}`));
    totals.appendChild(element("strong", "", `Total : ${valueOf(response.total)} ${valueOf(response.currency)}`));
    target.appendChild(totals);
    const actions = element("div", "form-actions");
    const confirm = element("button", "button primary", "Émettre la facture");
    confirm.type = "button";
    confirm.addEventListener("click", confirmInvoice);
    actions.appendChild(confirm);
    target.appendChild(actions);
  }

  function invalidatePreview() {
    if (!state.preview) return;
    state.preview = null;
    const target = $("#invoice-result");
    clear(target);
    target.hidden = true;
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

  function invoicePayload() {
    const data = new FormData($("#invoice-form"));
    return { customer_id: data.get("customer_id"), billing_period: data.get("billing_period"), electricity: { quantity: data.get("electricity_quantity"), unit: "KWH" }, water: { quantity: data.get("water_quantity"), unit: "M3" }, water_zero_confirmed: data.get("water_zero_confirmed") === "on", currency: "USD", tariff_revision: data.get("tariff_revision"), due_date: data.get("due_date") || null };
  }

  async function previewInvoice(event) {
    event.preventDefault();
    try {
      const payload = invoicePayload();
      const response = await api("/local/v1/billing/utility/preview", "POST", payload);
      state.preview = { payload, response };
      renderPreview(response);
      showNotice("Prévisualisation calculée. Aucune facture n’est encore émise.");
    } catch (error) { showNotice(error.message || "Prévisualisation impossible.", true); }
  }

  async function confirmInvoice() {
    if (!state.preview?.payload) return;
    try {
      await api("/local/v1/billing/utility/issue", "POST", state.preview.payload);
      $("#invoice-form").reset();
      $("#invoice-result").hidden = true;
      clear($("#invoice-result"));
      state.preview = null;
      showNotice("Facture émise et enregistrée localement.");
      await refresh();
    } catch (error) { showNotice(error.message || "Émission impossible.", true); }
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
    $("#invoice-form").addEventListener("submit", previewInvoice);
    ["input", "change"].forEach((eventName) => $("#invoice-form").addEventListener(eventName, invalidatePreview));
    $("#payment-customer").addEventListener("change", (event) => populateReceivables(event.target.value));
    $("#payment-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/payments", (data) => ({ customer_id: data.get("customer_id"), received_amount: data.get("received_amount"), currency: data.get("currency"), method: data.get("method"), occurred_at: new Date().toISOString(), allocations: [{ receivable_id: data.get("receivable_id"), amount: data.get("received_amount") }] }), "Règlement enregistré localement."));
    $("#solar-form").addEventListener("submit", (event) => submitForm(event, "/local/v1/solar/sales", (data) => ({ customer_id: data.get("customer_id"), product_code: data.get("product_code"), principal: data.get("principal"), currency: data.get("currency"), terms_revision: "SEL-V2" }), "Vente solaire enregistrée localement."));
    $("#water-quantity").addEventListener("input", (event) => { $("#water-zero-confirmed").required = Number(event.target.value || 0) === 0; });
    $("#water-zero-confirmed").required = Number($("#water-quantity").value || 0) === 0;
  }

  window.addEventListener("pywebviewready", () => refresh());
  setupNavigation(); setupForms();
})();
