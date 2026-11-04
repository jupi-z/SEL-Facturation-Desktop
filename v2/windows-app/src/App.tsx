import { useCallback, useEffect, useMemo, useState, type ButtonHTMLAttributes, type FormEvent, type ReactNode } from "react";
import { localRequest, localStatus, openMonarqueSite } from "./api";
import "./App.css";

type View = "dashboard" | "customers" | "invoices" | "payments" | "solar" | "receivables" | "sync" | "admin";
type Row = Record<string, unknown>;
type Notice = { message: string; error?: boolean } | null;

const navigation: { id: View; label: string; icon: string }[] = [
  { id: "dashboard", label: "Vue d’ensemble", icon: "grid" },
  { id: "customers", label: "Abonnés", icon: "people" },
  { id: "invoices", label: "Factures", icon: "document" },
  { id: "payments", label: "Encaissements", icon: "cash" },
  { id: "solar", label: "Kits solaires", icon: "sun" },
  { id: "receivables", label: "Créances", icon: "balance" },
  { id: "sync", label: "Synchronisation", icon: "sync" },
  { id: "admin", label: "Administration", icon: "settings" },
];

const pageTitle: Record<View, { title: string; context: string }> = {
  dashboard: { title: "Vue d’ensemble", context: "SEL / OPÉRATIONS" },
  customers: { title: "Répertoire des abonnés", context: "SEL / CLIENTS" },
  invoices: { title: "Factures", context: "SEL / FACTURATION" },
  payments: { title: "Journal de caisse", context: "SEL / COMMERCE & FINANCE" },
  solar: { title: "Catalogue des kits solaires", context: "SEL / COMMERCE & FINANCE" },
  receivables: { title: "Suivi des créances", context: "SEL / COMMERCE & FINANCE" },
  sync: { title: "Centre de synchronisation", context: "SEL / ADMINISTRATION" },
  admin: { title: "Administration", context: "SEL / ADMINISTRATION" },
};

function text(value: unknown, fallback = "—"): string {
  return value === undefined || value === null || String(value) === "" ? fallback : String(value);
}

function formatMoney(value: unknown, currency: unknown): string {
  const raw = text(value, "").trim();
  const match = /^(-?)(\d+)(?:[.,](\d+))?$/.exec(raw);
  if (!match) return raw ? `${raw} ${text(currency, "")}`.trim() : "—";
  const [, sign, integer, fraction = ""] = match;
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, "\u202f");
  return `${sign}${grouped},${fraction.padEnd(2, "0")} ${text(currency, "")}`.trim();
}

function rows<T extends Row>(value: unknown): T[] {
  return Array.isArray(value) ? value as T[] : [];
}

function humanError(error: unknown): string {
  return error instanceof Error ? error.message : "L’opération n’a pas abouti.";
}

function Button({ children, primary = false, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean }) {
  return <button className={`button${primary ? " button-primary" : ""}`} {...props}>{children}</button>;
}

function Panel({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return <section className="panel"><div className="panel-heading"><h2>{title}</h2>{action}</div>{children}</section>;
}

function Table({ headers, data, empty }: { headers: string[]; data: ReactNode[][]; empty: string }) {
  if (!data.length) return <div className="empty-state">{empty}</div>;
  return <div className="table-scroll"><table><thead><tr>{headers.map((header) => <th key={header} scope="col">{header}</th>)}</tr></thead><tbody>{data.map((row, index) => <tr key={index}>{row.map((value, column) => <td key={column}>{value}</td>)}</tr>)}</tbody></table></div>;
}

function NavIcon({ name }: { name: string }) {
  void name;
  return <svg className="nav-icon" viewBox="0 0 20 20" aria-hidden="true" focusable="false"><rect x="4" y="4" width="11" height="11" rx="1.5" fill="currentColor" /></svg>;
}

export default function App() {
  const [view, setView] = useState<View>("dashboard");
  const [customers, setCustomers] = useState<Row[]>([]);
  const [invoices, setInvoices] = useState<Row[]>([]);
  const [payments, setPayments] = useState<Row[]>([]);
  const [sales, setSales] = useState<Row[]>([]);
  const [solarProducts, setSolarProducts] = useState<Row[]>([]);
  const [solarQuote, setSolarQuote] = useState<Row | null>(null);
  const [solarSaleDraft, setSolarSaleDraft] = useState<Row | null>(null);
  const [sync, setSync] = useState<Row | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [loading, setLoading] = useState(false);
  const [selectedCustomer, setSelectedCustomer] = useState("");
  const [selectedCustomerData, setSelectedCustomerData] = useState<Row | null>(null);
  const [statement, setStatement] = useState<Row | null>(null);
  const [preview, setPreview] = useState<Row | null>(null);
  const [invoiceDraft, setInvoiceDraft] = useState<Row | null>(null);
  const [selectedInvoice, setSelectedInvoice] = useState<Row | null>(null);
  const [zeroConfirmed, setZeroConfirmed] = useState(false);
  const [invoiceFilter, setInvoiceFilter] = useState("");
  const [customerFilter, setCustomerFilter] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const local = await localStatus<Row>();
      setSync(local);
      if (local.status === "ACTIVATION_REQUIRED") {
        setCustomers([]); setInvoices([]); setPayments([]); setSales([]); setSolarProducts([]);
        setNotice({ message: text(local.message, "Ce poste doit être activé par un administrateur SEL."), error: true });
        return;
      }
      const [customerResult, invoiceResult, paymentResult, saleResult, productsResult, syncResult] = await Promise.allSettled([
        localRequest<unknown>("/local/v1/customers"),
        localRequest<unknown>("/local/v1/invoices?limit=100"),
        localRequest<unknown>("/local/v1/payments?limit=100"),
        localRequest<unknown>("/local/v1/solar/sales?limit=100"),
        localRequest<unknown>("/local/v1/solar/products"),
        Promise.resolve(local),
      ]);
      if (customerResult.status === "fulfilled") setCustomers(rows(customerResult.value));
      if (invoiceResult.status === "fulfilled") setInvoices(rows(invoiceResult.value));
      if (paymentResult.status === "fulfilled") setPayments(rows(paymentResult.value));
      if (saleResult.status === "fulfilled") setSales(rows(saleResult.value));
      if (productsResult.status === "fulfilled") setSolarProducts(rows(productsResult.value));
      if (syncResult.status === "fulfilled") setSync(syncResult.value);
      const failure = [customerResult, invoiceResult, paymentResult, saleResult, productsResult, syncResult].find((result) => result.status === "rejected");
      setNotice(failure?.status === "rejected" ? { message: humanError(failure.reason), error: true } : null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    if (!selectedCustomer) { setStatement(null); return; }
    let active = true;
    localRequest<Row>(`/local/v1/customers/${encodeURIComponent(selectedCustomer)}/statement`)
      .then((data) => { if (active) setStatement(data); })
      .catch((error: unknown) => { if (active) setNotice({ message: humanError(error), error: true }); });
    return () => { active = false; };
  }, [selectedCustomer]);

  const filteredInvoices = useMemo(() => invoices.filter((invoice) => {
    const query = invoiceFilter.trim().toLocaleLowerCase("fr");
    return !query || [invoice.invoice_number, invoice.invoice_id, invoice.customer_name, invoice.period]
      .some((value) => text(value, "").toLocaleLowerCase("fr").includes(query));
  }), [invoices, invoiceFilter]);
  const filteredCustomers = useMemo(() => customers.filter((customer) => {
    const query = customerFilter.trim().toLocaleLowerCase("fr");
    return !query || [customer.customer_code, customer.legal_name, customer.address, customer.phone]
      .some((value) => text(value, "").toLocaleLowerCase("fr").includes(query));
  }), [customers, customerFilter]);
  const recentActivity = useMemo(() => {
    const customerNames = new Map(customers.map((customer) => [
      text(customer.customer_id ?? customer.id, ""),
      text(customer.legal_name, text(customer.customer_code)),
    ]));
    const activity = [
      ...invoices.map((row) => ({
        row, kind: "Eau + électricité", reference: text(row.invoice_number ?? row.invoice_id),
        customer: text(row.customer_name, customerNames.get(text(row.customer_id, "")) ?? text(row.customer_id)),
        amount: text(row.total), date: text(row.issued_at ?? row.created_at),
      })),
      ...payments.map((row) => ({
        row, kind: "Encaissement", reference: text(row.payment_number ?? row.payment_id),
        customer: text(row.customer_name, customerNames.get(text(row.customer_id, "")) ?? text(row.customer_id)),
        amount: text(row.amount_received ?? row.received_amount), date: text(row.paid_at ?? row.occurred_at),
      })),
      ...sales.map((row) => ({
        row, kind: "Kit solaire", reference: text(row.sale_number ?? row.sale_id),
        customer: text(row.customer_name, customerNames.get(text(row.customer_id, "")) ?? text(row.customer_id)),
        amount: text(row.principal), date: text(row.sold_at ?? row.created_at),
      })),
    ];
    return activity.sort((left, right) => Date.parse(right.date) - Date.parse(left.date)).slice(0, 8);
  }, [customers, invoices, payments, sales]);

  async function submit(event: FormEvent<HTMLFormElement>, path: string, payload: Row, success: string) {
    event.preventDefault();
    const form = event.currentTarget;
    try {
      await localRequest(path, "POST", payload);
      setNotice({ message: success });
      form.reset();
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function previewInvoice(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const water = String(form.get("water_quantity") ?? "0");
    if (Number(water) === 0 && !zeroConfirmed) {
      setNotice({ message: "Confirmez le relevé d’eau à 0 m³ avant de demander le calcul.", error: true });
      return;
    }
    const payload = {
      customer_id: form.get("customer_id"),
      billing_period: form.get("billing_period"),
      electricity: { quantity: form.get("electricity_quantity"), unit: "KWH" },
      water: { quantity: water, unit: "M3" },
      water_zero_confirmed: zeroConfirmed,
      currency: "USD",
      tariff_revision: form.get("tariff_revision"),
      due_date: form.get("due_date") || null,
    };
    try {
      const result = await localRequest<Row>("/local/v1/billing/utility/preview", "POST", payload);
      setInvoiceDraft(payload);
      setPreview(result);
      setNotice({ message: "Calcul reçu du moteur Rust. La facture n’est pas encore émise." });
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function confirmInvoice() {
    if (!invoiceDraft) return;
    try {
      await localRequest("/local/v1/billing/utility/issue", "POST", invoiceDraft);
      setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
      setNotice({ message: "Facture émise et enregistrée sur ce poste." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function openInvoice(invoice: Row) {
    const id = text(invoice.invoice_id ?? invoice.id, "");
    if (!id) return;
    try {
      const details = await localRequest<Row>(`/local/v1/invoices/${encodeURIComponent(id)}`);
      setSelectedInvoice(details);
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function updateCustomer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedCustomerData) return;
    const form = new FormData(event.currentTarget);
    const payload = {
      legal_name: String(form.get("legal_name") ?? "").trim(),
      address: String(form.get("address") ?? "").trim() || null,
      phone: String(form.get("phone") ?? "").trim() || null,
    };
    try {
      const updated = await localRequest<Row>(`/local/v1/customers/${encodeURIComponent(selectedCustomer)}`, "PATCH", payload, Number(selectedCustomerData.revision));
      setSelectedCustomerData(updated);
      setNotice({ message: "Dossier abonné modifié. La révision locale a été vérifiée." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function syncNow() {
    try {
      await localRequest("/local/v1/sync/trigger", "POST", {});
      setNotice({ message: "Demande de synchronisation transmise à l’agent local. L’acceptation sera affichée après l’accusé central." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function quoteSolar(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const draft = {
      customer_id: form.get("customer_id"),
      product_code: form.get("product_code"),
      currency: form.get("currency"),
      terms_revision: form.get("terms_revision"),
    };
    try {
      const result = await localRequest<Row>("/local/v1/solar/quotes", "POST", {
        product_code: draft.product_code,
        currency: draft.currency,
        terms_revision: draft.terms_revision,
      });
      setSolarQuote(result);
      setSolarSaleDraft({ ...draft, principal: result.principal });
      setNotice({ message: "Le moteur Rust a validé le prix du catalogue. Aucune vente n’a encore été créée." });
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function confirmSolarSale() {
    if (!solarSaleDraft) return;
    try {
      await localRequest("/local/v1/solar/sales", "POST", solarSaleDraft);
      setSolarQuote(null);
      setSolarSaleDraft(null);
      setNotice({ message: "Vente enregistrée localement avec sa créance. Un paiement comptant se saisit dans le journal de caisse." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  const activePage = pageTitle[view];
  const pending = Number(sync?.pending ?? 0);
  const link = (id: View, label: string) => <button className="text-button" onClick={() => setView(id)}>{label}</button>;

  return <div className="app-shell">
    <aside className="sidebar">
      <button className="brand" aria-label="Accueil SEL" onClick={() => setView("dashboard")}><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /></button>
      <nav aria-label="Navigation principale">
        {navigation.map((item) => <button key={item.id} className={`nav-item${view === item.id ? " active" : ""}`} aria-current={view === item.id ? "page" : undefined} onClick={() => setView(item.id)}>
          <NavIcon name={item.icon} /><span>{item.label}</span>
        </button>)}
      </nav>
      <div className="sidebar-foot"><span className="eyebrow">POSTE LOCAL</span><p>Données conservées sur cet ordinateur</p></div>
    </aside>

    <main className="content">
      <header className="topbar"><p className="eyebrow">{activePage.context}</p>
        <div className={`connection-status${pending ? " pending" : ""}`}><span className="status-dot" aria-hidden="true" />
          {sync ? (pending ? `${pending} opération(s) en attente` : sync.status === "LOCAL_READY" ? "Service local prêt" : "Mode local") : "État local indisponible"}
        </div>
      </header>
      {notice && <div className={`notice${notice.error ? " error" : ""}`} role={notice.error ? "alert" : "status"}>{notice.message}<button aria-label="Fermer le message" onClick={() => setNotice(null)}>×</button></div>}
      {loading && <div className="loading-line" role="status">Actualisation des données locales…</div>}

      {view === "dashboard" && <>
        <div className="page-heading"><div><p className="eyebrow">ACTIVITÉ DU POSTE</p><h1>Vue d’ensemble</h1><p className="muted">Opérations enregistrées localement et état de leur synchronisation.</p></div><Button primary onClick={() => setView("invoices")}>Nouvelle facture</Button></div>
        <div className="metrics">
          <Metric label="Abonnés" value={customers.length} note="Dossiers disponibles sur ce poste" />
          <Metric label="Factures" value={invoices.length} note="Documents présents localement" />
          <Metric label="Encaissements" value={payments.length} note="Règlements enregistrés" />
          <Metric label="À synchroniser" value={pending} note={pending ? "En attente de confirmation centrale" : "État renvoyé par l’agent local"} />
        </div>
        <Panel title="Dossiers / activité récente" action={link("sync", "État de synchronisation →")}>
          <Table headers={["Référence", "Abonné", "Opération", "Montant", "État"]} empty="Aucune opération enregistrée sur ce poste." data={recentActivity.map(({ row, kind, reference, customer, amount }) => [<strong>{reference}</strong>, customer, kind, formatMoney(amount, row.currency), <Status value={row.status} />])} />
        </Panel>
      </>}

      {view === "customers" && <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Répertoire des abonnés" description="Rechercher un dossier, le consulter et enregistrer un nouvel abonné." />
        <div className="columns customer-layout">
          <Panel title="Abonnés enregistrés" action={<input className="table-search" aria-label="Rechercher un abonné" placeholder="Code, nom ou téléphone" value={customerFilter} onChange={(event) => setCustomerFilter(event.target.value)} />}><Table headers={["Code", "Nom / raison sociale", "Adresse", "Action"]} empty="Aucun abonné disponible sur ce poste." data={filteredCustomers.map((row) => [<strong>{text(row.customer_code)}</strong>, text(row.legal_name), text(row.address), <button className="text-button" onClick={() => { setSelectedCustomer(text(row.customer_id ?? row.id, "")); setSelectedCustomerData(row); }}>Ouvrir le dossier →</button>])} /></Panel>
          <Panel title="Nouvel abonné"><form className="form-grid" onSubmit={(event) => submit(event, "/local/v1/customers", Object.fromEntries(new FormData(event.currentTarget)) as Row, "Abonné enregistré localement.")}>
            <label>Code abonné<input name="customer_code" required /></label><label>Nom / raison sociale<input name="legal_name" required /></label>
            <label className="span-two">Adresse<input name="address" /></label><label className="span-two">Téléphone<input name="phone" /></label>
            <p className="help span-two">Le dossier local enregistre le code abonné, le nom, l’adresse et le téléphone.</p>
            <div className="form-actions span-two"><Button primary type="submit">Enregistrer l’abonné</Button></div>
          </form></Panel>
        </div>
        {selectedCustomer && <>
          <Panel title={`Fiche abonné · ${text(selectedCustomerData?.customer_code)}`}>
            {selectedCustomerData ? <form className="form-grid" onSubmit={(event) => void updateCustomer(event)}>
              <label>Nom / raison sociale<input name="legal_name" defaultValue={text(selectedCustomerData.legal_name, "")} required /></label>
              <label>Révision locale<input value={text(selectedCustomerData.revision)} readOnly /></label>
              <label className="span-two">Adresse<input name="address" defaultValue={text(selectedCustomerData.address, "")} /></label>
              <label>Téléphone<input name="phone" defaultValue={text(selectedCustomerData.phone, "")} /></label>
              <div className="form-actions span-two"><Button primary type="submit">Enregistrer les modifications</Button></div>
            </form> : <div className="empty-state">Chargement du dossier abonné…</div>}
          </Panel>
          <CustomerStatement customerId={selectedCustomer} statement={statement} />
        </>}
      </>}

      {view === "invoices" && <>
        <div className="page-heading"><div><p className="eyebrow">CORE / FACTURATION</p><h1>Factures eau + électricité</h1><p className="muted">Les deux services figurent au document, y compris l’eau à 0 m³. Le calcul est effectué par Rust.</p></div><span className="tag">USD</span></div>
        <Panel title="Nouvelle facture">
          <form className="form-grid" onSubmit={previewInvoice} onChange={() => { setPreview(null); setInvoiceDraft(null); }}>
            <label className="span-two">Abonné<select name="customer_id" required defaultValue=""><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
            <label>Période<input name="billing_period" type="month" required /></label><label>Date limite<input name="due_date" type="date" /></label>
            <label>Électricité (kWh)<input name="electricity_quantity" type="number" min="0" step="0.01" defaultValue="0" required /></label>
            <label>Eau (m³)<input name="water_quantity" type="number" min="0" step="0.01" defaultValue="0" required onChange={(event) => { if (Number(event.target.value) !== 0) setZeroConfirmed(false); }} /></label>
            <label>Révision approuvée<input name="tariff_revision" required /></label>
            <label className="check-row"><input type="checkbox" checked={zeroConfirmed} onChange={(event) => setZeroConfirmed(event.target.checked)} /> Je confirme que le relevé d’eau est exactement de 0 m³.</label>
            <div className="form-actions span-two"><Button primary type="submit">Prévisualiser la facture</Button></div>
          </form>
        </Panel>
        {preview && <Preview data={preview} onConfirm={() => void confirmInvoice()} />}
        <Panel title="Historique des factures" action={<input className="table-search" aria-label="Rechercher une facture" placeholder="Référence, abonné ou période" value={invoiceFilter} onChange={(event) => setInvoiceFilter(event.target.value)} />}>
          <Table headers={["Référence", "Abonné", "Période", "État", "Action"]} empty="Aucune facture locale." data={filteredInvoices.map((row) => [<strong>{text(row.invoice_number ?? row.invoice_id)}</strong>, text(row.customer_name ?? row.customer_id), text(row.period ?? row.billing_period), <Status value={row.status} />, <button className="text-button" onClick={() => void openInvoice(row)}>Consulter</button>])} />
        </Panel>
        {selectedInvoice && <>
          <p className="muted">Période {text(selectedInvoice.billing_period)} · Échéance {text(selectedInvoice.due_date)} · <Status value={selectedInvoice.status} /></p>
          <Preview data={selectedInvoice} title={`Détail de la facture · ${text(selectedInvoice.invoice_number ?? selectedInvoice.invoice_id)}`} action={<Button onClick={() => setSelectedInvoice(null)}>Fermer</Button>} />
        </>}
      </>}

      {view === "payments" && <>
        <PageHeading eyebrow="FINANCE / CAISSE" title="Journal de caisse" description="Enregistrer un règlement et l’affecter à une créance du même abonné." />
        <div className="columns customer-layout">
          <Panel title="Nouvel encaissement"><PaymentForm customers={customers} onSubmit={submit} /></Panel>
          <Panel title="Encaissements enregistrés"><Table headers={["Référence", "Date", "Montant", "Mode", "État"]} empty="Aucun encaissement local." data={payments.map((row) => [<strong>{text(row.payment_id)}</strong>, text(row.paid_at ?? row.occurred_at), formatMoney(row.amount_received ?? row.received_amount, row.currency), text(row.method), <Status value={row.status} />])} /></Panel>
        </div>
      </>}

      {view === "solar" && <>
        <PageHeading eyebrow="FINANCE / SOLAIRE" title="Catalogue et ventes solaires" description="Les produits et prix affichés proviennent du dernier catalogue validé et disponible localement." />
        <Panel title="Catalogue approuvé">
          <div className="product-grid">{solarProducts.map((product) => <article className="product-card" key={text(product.id)}><p className="eyebrow">{text(product.sku)}</p><h3>{text(product.label)}</h3><p>{formatMoney(product.price_reference, product.currency)}</p><span className="product-meta">Valide depuis le {text(product.effective_from)} · {text(product.approval_ref)}</span></article>)}</div>
          {!solarProducts.length && <div className="empty-state">Aucun produit solaire approuvé et applicable n’est disponible sur ce poste.</div>}
        </Panel>
        <Panel title="Nouvelle vente solaire"><form className="form-grid" onSubmit={quoteSolar} onChange={() => { setSolarQuote(null); setSolarSaleDraft(null); }}>
          <label className="span-two">Abonné<select name="customer_id" required defaultValue=""><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          <label>Produit approuvé<select name="product_code" required defaultValue=""><option value="" disabled>Sélectionner…</option>{solarProducts.map((product) => <option key={text(product.id)} value={text(product.sku)}>{text(product.sku)} — {text(product.label)} · {formatMoney(product.price_reference, product.currency)}</option>)}</select></label>
          <label>Devise du catalogue<select name="currency" defaultValue={text(solarProducts[0]?.currency, "USD")} required>{[...new Set(solarProducts.map((product) => text(product.currency, "")))].filter(Boolean).map((currency) => <option key={currency} value={currency}>{currency}</option>)}</select></label>
          <label className="span-two">Révision des conditions approuvées<input name="terms_revision" required /></label>
          <p className="help span-two">Le prix est relu et vérifié par le moteur Rust avant l’émission. La vente crée une créance ; enregistrez tout règlement dans le journal de caisse.</p>
          <div className="form-actions span-two"><Button primary type="submit">Vérifier le devis</Button></div>
        </form></Panel>
        {solarQuote && <Panel title="Devis vérifié par Rust"><div className="quote-summary"><div><span>Produit</span><strong>{text(solarQuote.product_code)}</strong></div><div><span>Montant approuvé</span><strong>{formatMoney(solarQuote.principal, solarQuote.currency)}</strong></div><div><span>Conditions</span><strong>{text(solarQuote.terms_revision)}</strong></div></div><div className="form-actions"><Button primary onClick={() => void confirmSolarSale()}>Confirmer et créer la vente</Button></div></Panel>}
        <Panel title="Ventes locales"><Table headers={["Référence", "Abonné", "Produit", "Principal", "État"]} empty="Aucune vente solaire locale." data={sales.map((row) => [<strong>{text(row.sale_id)}</strong>, text(row.customer_id), text(row.product_code), formatMoney(row.principal, row.currency), <Status value={row.status} />])} /></Panel>
      </>}

      {view === "receivables" && <>
        <PageHeading eyebrow="FINANCE / SOLDES" title="Suivi des créances" description="Soldes et obligations renvoyés par l’agent local, sans recalcul dans l’interface." />
        <Panel title="Situation d’un abonné"><label className="customer-picker">Abonné<select value={selectedCustomer} onChange={(event) => setSelectedCustomer(event.target.value)}><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          {statement ? <Table headers={["Référence", "Nature", "Solde", "Situation"]} empty="Aucune créance associée." data={rows<Row>(statement.receivables).map((row) => [<strong>{text(row.receivable_id ?? row.id)}</strong>, text(row.source_type), formatMoney(row.balance, row.currency), <Status value={row.status} />])} /> : <div className="empty-state">Choisissez un abonné pour consulter son relevé et ses créances.</div>}
        </Panel>
      </>}

      {view === "sync" && <>
        <div className="page-heading"><div><p className="eyebrow">ADMIN / CONTINUITÉ</p><h1>Centre de synchronisation</h1><p className="muted">Une opération n’est confirmée qu’après l’accusé durable du serveur central.</p></div><Button primary onClick={() => void syncNow()}>Synchroniser maintenant</Button></div>
        <div className="metrics compact-metrics"><Metric label="À envoyer" value={text(sync?.pending, "—")} note="Enregistrées en SQLite" /><Metric label="Conflits" value={text(sync?.conflicts, "—")} note="À examiner" /><Metric label="Refus" value={text(sync?.rejected, "—")} note="Motif conservé" /><Metric label="Dernière réussite" value={text(sync?.last_successful_sync, "Aucune confirmée")} note="Accusé central" /></div>
        <Panel title="État du poste"><div className="summary-state"><Status value={text(sync?.state ?? sync?.status, "LOCAL_ONLY")} /><p>{pending ? `${pending} opération(s) attendent leur confirmation centrale.` : "Le statut reflète les informations confirmées par l’agent local."}</p><p>Autorisation hors ligne : {text(sync?.offline_grant, "état non communiqué")}</p></div></Panel>
      </>}

      {view === "admin" && <>
        <PageHeading eyebrow="ADMIN / RÉFÉRENTIELS" title="Administration" description="Les écrans administratifs suivent les permissions et les paramètres effectivement exposés par le service Rust." />
        <div className="admin-grid">
          <AdminCard title="Utilisateurs et rôles" description="Gestion des accès et sessions" state="API locale d’administration non exposée" />
          <AdminCard title="Appareils et autorisations offline" description="Enrôlement, grants et limites" state="À configurer par le serveur central" />
          <AdminCard title="Tarifs et taxes approuvés" description="Révisions datées et historisées" state="Le catalogue local est en lecture seule" />
          <AdminCard title="Journal d’audit" description="Écritures métier et corrections" state="Vue d’audit à raccorder au contrat central" />
        </div>
      </>}

      <footer>Application développée et sécurisée par LTEX · <button className="footer-link" onClick={() => void openMonarqueSite().catch((error: unknown) => setNotice({ message: humanError(error), error: true }))}>MonarQue Group</button></footer>
    </main>
  </div>;
}

function PageHeading({ eyebrow, title, description }: { eyebrow: string; title: string; description: string }) {
  return <div className="page-heading"><div><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p className="muted">{description}</p></div></div>;
}

function Metric({ label, value, note }: { label: string; value: ReactNode; note: string }) {
  return <div className="metric"><div className="metric-label">{label}</div><div className="metric-value">{value}</div><div className="metric-note">{note}</div></div>;
}

function Status({ value }: { value: unknown }) {
  const normalized = text(value, "État inconnu");
  const state = normalized.toLocaleLowerCase("en").includes("pending") || normalized.toLocaleLowerCase("fr").includes("attente") ? "pending"
    : normalized.toLocaleLowerCase("en").includes("conflict") || normalized.toLocaleLowerCase("fr").includes("conflit") || normalized.toLocaleLowerCase("en").includes("reject") ? "danger"
      : normalized.toLocaleLowerCase("en").includes("accept") || normalized.toLocaleLowerCase("en").includes("ready") ? "success" : "neutral";
  return <span className={`status-badge ${state}`}>{normalized}</span>;
}

function Preview({ data, onConfirm, title = "Prévisualisation calculée par Rust", action }: { data: Row; onConfirm?: () => void; title?: string; action?: ReactNode }) {
  const lines = rows<Row>(data.lines);
  return <Panel title={title} action={action}>{onConfirm && <p className="help">Vérifiez les deux services, les taxes et le total. Cette prévisualisation ne crée pas de facture.</p>}
    <Table headers={["Service", "Quantité", "Prix unitaire", "Sous-total", "Taxe", "Total"]} empty="Le moteur n’a retourné aucune ligne." data={lines.map((line) => [text(line.service_code), `${text(line.quantity)} ${text(line.unit)}`, formatMoney(line.unit_price, data.currency), formatMoney(line.subtotal, data.currency), formatMoney(line.tax, data.currency), formatMoney(line.total, data.currency)])} />
      <div className="totals"><span>Sous-total <strong>{formatMoney(data.subtotal, data.currency)}</strong></span><span>Taxes <strong>{formatMoney(data.tax, data.currency)}</strong></span><span>Total <strong>{formatMoney(data.total, data.currency)}</strong></span></div>
    {onConfirm && <div className="form-actions"><Button primary onClick={onConfirm}>Émettre cette facture</Button></div>}
  </Panel>;
}

function PaymentForm({ customers, onSubmit }: { customers: Row[]; onSubmit: (event: FormEvent<HTMLFormElement>, path: string, payload: Row, success: string) => Promise<void> }) {
  const [customerId, setCustomerId] = useState("");
  const [receivables, setReceivables] = useState<Row[]>([]);
  useEffect(() => {
    if (!customerId) { setReceivables([]); return; }
    let active = true;
    localRequest<Row>(`/local/v1/customers/${encodeURIComponent(customerId)}/statement`).then((result) => {
      if (active) setReceivables(rows<Row>(result.receivables).filter((item) => Number(item.balance) > 0));
    }).catch(() => { if (active) setReceivables([]); });
    return () => { active = false; };
  }, [customerId]);
  return <form className="form-grid" onSubmit={(event) => { const form = new FormData(event.currentTarget); void onSubmit(event, "/local/v1/payments", {
    customer_id: form.get("customer_id"), received_amount: form.get("received_amount"), currency: form.get("currency"), method: form.get("method"), occurred_at: new Date().toISOString(),
    allocations: [{ receivable_id: form.get("receivable_id"), amount: form.get("received_amount") }],
  }, "Règlement enregistré sur ce poste."); }}>
    <label className="span-two">Abonné<select name="customer_id" value={customerId} onChange={(event) => setCustomerId(event.target.value)} required><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
    <label>Montant reçu<input name="received_amount" type="number" min="0.01" step="0.01" required /></label><label>Devise<input name="currency" defaultValue="USD" required /></label>
    <label>Mode<select name="method"><option value="CASH">Espèces</option><option value="BANK">Banque</option><option value="MOBILE_MONEY">Mobile Money</option><option value="OTHER">Autre</option></select></label>
    <label>Créance à affecter<select name="receivable_id" required disabled={!customerId || !receivables.length}><option value="">{customerId ? "Sélectionner une créance" : "Sélectionnez d’abord un abonné"}</option>{receivables.map((item) => <option key={text(item.receivable_id ?? item.id)} value={text(item.receivable_id ?? item.id)}>{text(item.status)} · {formatMoney(item.balance, item.currency)}</option>)}</select></label>
    <div className="form-actions span-two"><Button primary type="submit">Enregistrer le règlement</Button></div>
  </form>;
}

function CustomerStatement({ customerId, statement }: { customerId: string; statement: Row | null }) {
  return <Panel title={`Situation du compte · ${customerId}`}>
    {statement ? <div className="columns"><div><p><strong>Factures</strong></p><Table headers={["Référence", "Période", "État"]} empty="Aucune facture dans le relevé." data={rows<Row>(statement.invoices).map((item) => [text(item.invoice_number ?? item.invoice_id), text(item.period ?? item.billing_period), text(item.status)])} /></div><div><p><strong>Créances</strong></p><Table headers={["Référence", "Solde", "Situation"]} empty="Aucune créance dans le relevé." data={rows<Row>(statement.receivables).map((item) => [text(item.id ?? item.receivable_id), formatMoney(item.balance, item.currency), text(item.status)])} /></div></div> : <div className="empty-state">Chargement du relevé abonné…</div>}
  </Panel>;
}

function AdminCard({ title, description, state }: { title: string; description: string; state: string }) {
  return <section className="panel admin-card"><p className="eyebrow">ADMINISTRATION</p><h2>{title}</h2><p className="muted">{description}</p><span className="status-badge pending">{state}</span></section>;
}
