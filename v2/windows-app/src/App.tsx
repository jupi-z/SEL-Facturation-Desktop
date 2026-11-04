import { useCallback, useEffect, useMemo, useState, type ButtonHTMLAttributes, type FormEvent, type ReactNode } from "react";
import { localRequest, localStatus, openMonarqueSite } from "./api";
import "./App.css";

type View = "dashboard" | "customers" | "readings" | "invoices" | "payments" | "solar" | "receivables" | "sync" | "admin";
type Row = Record<string, unknown>;
type Notice = { message: string; error?: boolean } | null;

const navigation: { id: View; label: string; icon: string }[] = [
  { id: "dashboard", label: "Vue d’ensemble", icon: "grid" },
  { id: "customers", label: "Abonnés", icon: "people" },
  { id: "readings", label: "Relevés", icon: "meter" },
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
  readings: { title: "Relevés de compteurs", context: "SEL / RELEVÉS" },
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

function formatUnit(unit: unknown): string {
  const unitCode = text(unit, "").toUpperCase();
  return unitCode === "KWH" ? "kWh" : unitCode === "M3" ? "m³" : text(unit, "");
}

function formatQuantity(value: unknown, unit: unknown): string {
  const raw = text(value, "").trim();
  const match = /^(-?)(\d+)(?:[.,](\d+))?$/.exec(raw);
  const unitLabel = formatUnit(unit);
  if (!match) return raw ? `${raw} ${unitLabel}`.trim() : "—";
  const [, sign, integer, rawFraction = ""] = match;
  const fraction = rawFraction.replace(/0+$/, "");
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, "\u202f");
  return `${sign}${grouped}${fraction ? `,${fraction}` : ""} ${unitLabel}`.trim();
}

function formatDateTime(value: unknown): string {
  const raw = text(value, "");
  const date = new Date(raw);
  if (!raw || Number.isNaN(date.getTime())) return raw || "—";
  return new Intl.DateTimeFormat("fr-CD", {
    dateStyle: "short",
    timeStyle: "short",
    timeZone: "Africa/Lubumbashi",
  }).format(date);
}

function localDateTimeValue(date = new Date()): string {
  const localDate = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return localDate.toISOString().slice(0, 16);
}

function domainLabel(value: unknown): string {
  const raw = text(value, "");
  const labels: Record<string, string> = {
    UTILITY_INVOICE: "Facture eau + électricité",
    SOLAR_SALE: "Vente de kit solaire",
    CASH: "Espèces",
    BANK: "Banque",
    MOBILE_MONEY: "Mobile Money",
    OTHER: "Autre",
    ELECTRICITY: "Électricité",
    WATER: "Eau",
    CONNECTED: "Raccordé",
    DISCONNECTED: "Non raccordé",
    UNKNOWN: "À vérifier",
  };
  return labels[raw.toUpperCase()] ?? raw;
}

function rows<T extends Row>(value: unknown): T[] {
  return Array.isArray(value) ? value as T[] : [];
}

function customerLabel(customerId: unknown, customers: Row[]): string {
  const id = text(customerId, "");
  const customer = customers.find((item) => text(item.customer_id ?? item.id, "") === id);
  return text(customer?.legal_name, text(customer?.customer_code, id));
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

function Table({ headers, data, empty, className = "" }: { headers: string[]; data: ReactNode[][]; empty: string; className?: string }) {
  if (!data.length) return <div className="empty-state">{empty}</div>;
  return <div className="table-scroll"><table className={className}><thead><tr>{headers.map((header) => <th key={header} scope="col">{header}</th>)}</tr></thead><tbody>{data.map((row, index) => <tr key={index}>{row.map((value, column) => <td key={column}>{value}</td>)}</tr>)}</tbody></table></div>;
}

function NavIcon({ name }: { name: string }) {
  const icons: Record<string, ReactNode> = {
    grid: <><rect x="3.5" y="3.5" width="5.5" height="5.5" rx="1" /><rect x="11" y="3.5" width="5.5" height="5.5" rx="1" /><rect x="3.5" y="11" width="5.5" height="5.5" rx="1" /><rect x="11" y="11" width="5.5" height="5.5" rx="1" /></>,
    people: <><circle cx="7" cy="7" r="2.5" /><path d="M2.8 16c.5-2.4 2-3.6 4.2-3.6s3.7 1.2 4.2 3.6" /><path d="M12.1 5a2.3 2.3 0 0 1 0 4.5M13 12.6c1.9.1 3.3 1.2 3.8 3.4" /></>,
    meter: <><rect x="3.5" y="3.5" width="13" height="13" rx="2" /><circle cx="10" cy="10" r="4.2" /><path d="m10 10 2.5-2" /><path d="M7 15.5h6" /></>,
    document: <><path d="M5 2.8h6l4 4V17a1.2 1.2 0 0 1-1.2 1.2H5A1.2 1.2 0 0 1 3.8 17V4A1.2 1.2 0 0 1 5 2.8Z" /><path d="M11 3v4h4M6.5 11h6M6.5 14h6" /></>,
    cash: <><rect x="2.8" y="5" width="14.4" height="10" rx="1.8" /><path d="M5 5V3.8h10V5M5 15v1.2h10V15" /><circle cx="10" cy="10" r="2.2" /><path d="M5.2 8v.1M14.8 12v.1" /></>,
    sun: <><circle cx="10" cy="10" r="3.2" /><path d="M10 2.5v2M10 15.5v2M17.5 10h-2M4.5 10h-2m12.8-5.3-1.4 1.4M6.1 13.9l-1.4 1.4m10.6 0-1.4-1.4M6.1 6.1 4.7 4.7" /></>,
    balance: <><path d="M10 3v13M5 6h10M4 6l-2 4h4L4 6Zm12 0-2 4h4l-2-4Z" /><path d="M6 17h8M7.5 14h5" /></>,
    sync: <><path d="M15.8 7.5A6.3 6.3 0 0 0 4.4 6L3 7.5" /><path d="M3 4.5v3h3M4.2 12.5A6.3 6.3 0 0 0 15.6 14L17 12.5" /><path d="M17 15.5v-3h-3" /></>,
    settings: <><path d="m8.6 2.8.4-1h2l.4 1a7.6 7.6 0 0 1 1.6.7l1-.4 1.4 1.4-.4 1a7.6 7.6 0 0 1 .7 1.6l1 .4v2l-1 .4a7.6 7.6 0 0 1-.7 1.6l.4 1-1.4 1.4-1-.4a7.6 7.6 0 0 1-1.6.7l-.4 1h-2l-.4-1a7.6 7.6 0 0 1-1.6-.7l-1 .4-1.4-1.4.4-1a7.6 7.6 0 0 1-.7-1.6l-1-.4v-2l1-.4A7.6 7.6 0 0 1 5 5.5l-.4-1L6 3.1l1 .4a7.6 7.6 0 0 1 1.6-.7Z" transform="translate(1.5 1.5) scale(.85)" /><circle cx="10" cy="10" r="2.5" /></>,
  };
  return <svg className="nav-icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">{icons[name] ?? icons.grid}</svg>;
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
  const [solarSaleOpen, setSolarSaleOpen] = useState(false);
  const [sync, setSync] = useState<Row | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [loading, setLoading] = useState(false);
  const [selectedCustomer, setSelectedCustomer] = useState("");
  const [selectedCustomerData, setSelectedCustomerData] = useState<Row | null>(null);
  const [customerEditOpen, setCustomerEditOpen] = useState(false);
  const [statement, setStatement] = useState<Row | null>(null);
  const [preview, setPreview] = useState<Row | null>(null);
  const [invoiceDraft, setInvoiceDraft] = useState<Row | null>(null);
  const [selectedInvoice, setSelectedInvoice] = useState<Row | null>(null);
  const [zeroConfirmed, setZeroConfirmed] = useState(false);
  const [invoiceFormKey, setInvoiceFormKey] = useState(0);
  const [invoiceFilter, setInvoiceFilter] = useState("");
  const [customerFilter, setCustomerFilter] = useState("");
  const [customerCreateOpen, setCustomerCreateOpen] = useState(false);

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

  async function submit(event: FormEvent<HTMLFormElement>, path: string, payload: Row, success: string): Promise<boolean> {
    event.preventDefault();
    const form = event.currentTarget;
    try {
      await localRequest(path, "POST", payload);
      setNotice({ message: success });
      form.reset();
      await refresh();
      return true;
    } catch (error) { setNotice({ message: humanError(error), error: true }); return false; }
  }

  async function createCustomer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    try {
      await localRequest("/local/v1/customers", "POST", Object.fromEntries(new FormData(form)) as Row);
      setNotice({ message: "Abonné enregistré localement." });
      form.reset();
      setCustomerCreateOpen(false);
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function previewInvoice(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setNotice(null);
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
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function confirmInvoice() {
    if (!invoiceDraft) return;
    try {
      await localRequest("/local/v1/billing/utility/issue", "POST", invoiceDraft);
      setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
      setInvoiceFormKey((key) => key + 1);
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
      setCustomerEditOpen(false);
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
      setSolarSaleOpen(false);
      setNotice({ message: "Vente enregistrée localement avec sa créance. Un paiement comptant se saisit dans le journal de caisse." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function navigateTo(nextView: View) {
    setNotice(null);
    setView(nextView);
  }

  const activePage = pageTitle[view];
  const pending = Number(sync?.pending ?? 0);
  const link = (id: View, label: string) => <button className="text-button" onClick={() => navigateTo(id)}>{label}</button>;
  const previewLines = rows<Row>(preview?.lines);
  const previewElectricity = previewLines.find((line) => text(line.service_code).toUpperCase() === "ELECTRICITY");
  const previewWater = previewLines.find((line) => text(line.service_code).toUpperCase() === "WATER");

  return <div className="app-shell">
    <aside className="sidebar">
      <button className="brand" aria-label="Accueil SEL" onClick={() => navigateTo("dashboard")}><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /></button>
      <nav aria-label="Navigation principale">
        {navigation.map((item) => <button key={item.id} className={`nav-item${view === item.id ? " active" : ""}`} aria-current={view === item.id ? "page" : undefined} onClick={() => navigateTo(item.id)}>
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
        <div className="page-heading"><div><p className="eyebrow">ACTIVITÉ DU POSTE</p><h1>Vue d’ensemble</h1><p className="muted">Opérations enregistrées localement et état de leur synchronisation.</p></div><Button primary onClick={() => navigateTo("invoices")}>Nouvelle facture</Button></div>
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

      {view === "customers" && (selectedCustomer ? <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Dossier abonné" description={`${text(selectedCustomerData?.customer_code)} · ${text(selectedCustomerData?.legal_name)}`} action={<div className="page-actions"><Button aria-label="Retour au répertoire" onClick={() => { setSelectedCustomer(""); setSelectedCustomerData(null); setStatement(null); setCustomerEditOpen(false); }}>Retour au répertoire</Button><Button primary onClick={() => setCustomerEditOpen((open) => !open)}>{customerEditOpen ? "Annuler la modification" : "Modifier la fiche"}</Button></div>} />
        <Panel title={`Fiche abonné · ${text(selectedCustomerData?.customer_code)}`}>
          {selectedCustomerData ? customerEditOpen ? <form className="form-grid" onSubmit={(event) => void updateCustomer(event)}>
            <label>Nom / raison sociale<input name="legal_name" defaultValue={text(selectedCustomerData.legal_name, "")} required /></label>
            <label>Révision locale<input value={text(selectedCustomerData.revision)} readOnly /></label>
            <label className="span-two">Adresse<input name="address" defaultValue={text(selectedCustomerData.address, "")} /></label>
            <label>Téléphone<input name="phone" defaultValue={text(selectedCustomerData.phone, "")} /></label>
            <div className="form-actions span-two"><Button type="button" onClick={() => setCustomerEditOpen(false)}>Annuler</Button><Button primary type="submit">Enregistrer les modifications</Button></div>
          </form> : <dl className="customer-details"><div><dt>Nom / raison sociale</dt><dd>{text(selectedCustomerData.legal_name)}</dd></div><div><dt>Code abonné</dt><dd>{text(selectedCustomerData.customer_code)}</dd></div><div><dt>Adresse</dt><dd>{text(selectedCustomerData.address)}</dd></div><div><dt>Téléphone</dt><dd>{text(selectedCustomerData.phone)}</dd></div><div><dt>Révision locale</dt><dd>{text(selectedCustomerData.revision)}</dd></div></dl> : <div className="empty-state">Chargement du dossier abonné…</div>}
        </Panel>
        <CustomerStatement customerName={text(selectedCustomerData?.legal_name, selectedCustomer)} customerCode={text(selectedCustomerData?.customer_code)} statement={statement} />
      </> : <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Répertoire des abonnés" description="Rechercher un dossier et consulter sa situation." action={<Button primary onClick={() => setCustomerCreateOpen((open) => !open)}>{customerCreateOpen ? "Fermer le formulaire" : "+ Ajouter un abonné"}</Button>} />
        <div className="metrics customer-metrics">
          <Metric label="Abonnés enregistrés" value={customers.length} note="Dossiers disponibles sur ce poste" />
          <Metric label="Factures locales" value={invoices.length} note="Documents présents sur ce poste" />
          <Metric label="Versements locaux" value={payments.length} note="Règlements enregistrés localement" />
          <Metric label="À synchroniser" value={pending} note={pending ? "En attente de confirmation centrale" : "État renvoyé par l’agent local"} />
        </div>
        {customerCreateOpen && <Panel title="Nouvel abonné"><form className="form-grid customer-create-form" onSubmit={(event) => void createCustomer(event)}>
            <label>Code abonné<input name="customer_code" required /></label><label>Nom / raison sociale<input name="legal_name" required /></label>
            <label>Adresse<input name="address" /></label><label>Téléphone<input name="phone" /></label>
            <p className="help span-two">Les informations pourront être complétées dans le dossier de l’abonné.</p>
            <div className="form-actions span-two"><Button primary type="submit">Enregistrer l’abonné</Button></div>
          </form></Panel>}
        <Panel title="Abonnés enregistrés" action={<input className="table-search" aria-label="Rechercher un abonné" placeholder="Code, nom ou téléphone" value={customerFilter} onChange={(event) => setCustomerFilter(event.target.value)} />}><Table headers={["Code", "Nom / raison sociale", "Adresse", "Action"]} empty="Aucun abonné disponible sur ce poste." data={filteredCustomers.map((row) => [<strong>{text(row.customer_code)}</strong>, text(row.legal_name), text(row.address), <button className="text-button" onClick={() => { setSelectedCustomer(text(row.customer_id ?? row.id, "")); setSelectedCustomerData(row); setCustomerEditOpen(false); }}>Ouvrir le dossier →</button>])} /></Panel>
      </>)}

      {view === "readings" && <MeterReadingsView customers={customers} />}

      {view === "invoices" && <>
        <div hidden={Boolean(preview)}>
          <div className="page-heading"><div><p className="eyebrow">CORE / FACTURATION</p><h1>Factures eau + électricité</h1><p className="muted">Les deux services figurent au document, y compris l’eau à 0 m³. Le calcul est effectué par Rust.</p></div><span className="tag">USD</span></div>
          <div className="invoice-workspace">
            <Panel title="Paramètres de facturation">
              <form id="utility-invoice-form" key={invoiceFormKey} className="form-grid invoice-form" onSubmit={previewInvoice} onChange={() => { setPreview(null); setInvoiceDraft(null); }}>
                <label>Abonné<select name="customer_id" required defaultValue=""><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
                <label>Période de facturation<input name="billing_period" type="month" required /></label>
                <label>Électricité consommée (kWh)<input name="electricity_quantity" type="number" min="0" step="0.01" defaultValue="0" required /></label>
                <label>Eau consommée (m³)<input name="water_quantity" type="number" min="0" step="0.01" defaultValue="0" required onChange={(event) => { if (Number(event.target.value) !== 0) setZeroConfirmed(false); }} /></label>
                <label>Révision tarifaire approuvée<input name="tariff_revision" required /></label>
                <label>Date limite de paiement<input name="due_date" type="date" /></label>
                <label className="check-row invoice-zero-confirm"><input type="checkbox" checked={zeroConfirmed} onChange={(event) => setZeroConfirmed(event.target.checked)} /> Je confirme que le relevé d’eau est exactement de 0 m³.</label>
              </form>
            </Panel>
            <aside className="panel invoice-summary" aria-label="Récapitulatif de la facture">
              <p className="eyebrow">RÉCAPITULATIF</p>
              <h2>Facture combinée</h2>
              <p className="muted">Électricité + eau · deux lignes obligatoires</p>
              <div className="invoice-summary-empty"><strong>À calculer</strong><span>Le montant s’affichera après vérification par le moteur Rust.</span></div>
              <span className="status-badge">En attente du calcul</span>
              <p className="help invoice-summary-help">Les calculs et l’autorisation restent gérés par le moteur Rust.</p>
              <Button primary type="submit" form="utility-invoice-form">Prévisualiser la facture</Button>
            </aside>
          </div>
          <Panel title="Historique des factures" action={<input className="table-search" aria-label="Rechercher une facture" placeholder="Référence, abonné ou période" value={invoiceFilter} onChange={(event) => setInvoiceFilter(event.target.value)} />}>
            <Table headers={["Référence", "Abonné", "Période", "État", "Action"]} empty="Aucune facture locale." data={filteredInvoices.map((row) => [<strong>{text(row.invoice_number ?? row.invoice_id)}</strong>, text(row.customer_name ?? row.customer_id), text(row.period ?? row.billing_period), <Status value={row.status} />, <button className="text-button" onClick={() => void openInvoice(row)}>Consulter</button>])} />
          </Panel>
          {selectedInvoice && <>
            <p className="muted">Période {text(selectedInvoice.billing_period)} · Échéance {text(selectedInvoice.due_date)} · <Status value={selectedInvoice.status} /></p>
            <Preview data={selectedInvoice} title={`Détail de la facture · ${text(selectedInvoice.invoice_number ?? selectedInvoice.invoice_id)}`} action={<Button onClick={() => setSelectedInvoice(null)}>Fermer</Button>} />
          </>}
        </div>
        {preview && <>
          <PageHeading eyebrow="CORE / FACTURATION" title="Prévisualisation de facture" description="Vérifier les lignes, les taxes et les références avant d’émettre." action={<div className="page-actions"><Button onClick={() => { setPreview(null); setInvoiceDraft(null); }}>Modifier la saisie</Button><Button primary onClick={() => void confirmInvoice()}>Émettre la facture</Button></div>} />
          <div className="invoice-preview-metrics metrics">
            <Metric label="Électricité" value={previewElectricity ? formatQuantity(previewElectricity.quantity, previewElectricity.unit) : "—"} note={previewElectricity ? `${formatMoney(previewElectricity.subtotal, preview.currency)} hors taxe` : "Ligne indisponible"} />
            <Metric label="Eau" value={previewWater ? formatQuantity(previewWater.quantity, previewWater.unit) : "—"} note={previewWater ? formatMoney(previewWater.total, preview.currency) : "Ligne indisponible"} />
            <Metric label="Taxes" value={formatMoney(preview.tax, preview.currency)} note="Calcul retourné par Rust" />
            <Metric label="Montant total" value={formatMoney(preview.total, preview.currency)} note="À confirmer avant émission" />
          </div>
          <Preview data={preview} title="Lignes de la facture" />
          <p className="help invoice-preview-status" role="status">Le calcul et les taxes viennent du moteur Rust. Cette prévisualisation n’a pas encore créé de facture.</p>
        </>}
      </>}

      {view === "payments" && <>
        <PageHeading eyebrow="FINANCE / CAISSE" title="Journal de caisse" description="Enregistrer un règlement et l’affecter à une créance du même abonné." />
        <PaymentForm customers={customers} sales={sales} onSubmit={submit} />
        <Panel title="Encaissements enregistrés"><Table className="payment-table" headers={["Référence", "Date et heure", "Montant", "Mode", "État"]} empty="Aucun encaissement local." data={payments.map((row) => [<strong title={text(row.payment_number ?? row.payment_id)}>{text(row.payment_number ?? row.payment_id)}</strong>, formatDateTime(row.paid_at ?? row.occurred_at), formatMoney(row.amount_received ?? row.received_amount, row.currency), domainLabel(row.method), <Status value={row.status} />])} /></Panel>
      </>}

      {view === "solar" && <>
        <PageHeading eyebrow="COMMERCE / FINANCE" title="Catalogue des kits solaires" description="Consultez les produits approuvés et les ventes enregistrées sur ce poste." action={<Button primary onClick={() => setSolarSaleOpen(true)}>Créer une vente</Button>} />
        {solarSaleOpen && <Panel title="Nouvelle vente solaire" action={<Button onClick={() => { setSolarSaleOpen(false); setSolarQuote(null); setSolarSaleDraft(null); }}>Fermer</Button>}><form className="form-grid" onSubmit={quoteSolar} onChange={() => { setSolarQuote(null); setSolarSaleDraft(null); }}>
          <label className="span-two">Abonné<select name="customer_id" required defaultValue=""><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          <label>Produit approuvé<select name="product_code" required defaultValue=""><option value="" disabled>Sélectionner…</option>{solarProducts.map((product) => <option key={text(product.id)} value={text(product.sku)}>{text(product.sku)} — {text(product.label)} · {formatMoney(product.price_reference, product.currency)}</option>)}</select></label>
          <label>Devise du catalogue<select name="currency" defaultValue={text(solarProducts[0]?.currency, "USD")} required>{[...new Set(solarProducts.map((product) => text(product.currency, "")))].filter(Boolean).map((currency) => <option key={currency} value={currency}>{currency}</option>)}</select></label>
          <label className="span-two">Révision des conditions approuvées<input name="terms_revision" required /></label>
          <p className="help span-two">Le prix est relu et vérifié par le moteur Rust avant l’émission. La vente crée une créance ; enregistrez tout règlement dans le journal de caisse.</p>
          <div className="form-actions span-two"><Button primary type="submit">Vérifier le devis</Button></div>
        </form></Panel>}
        {solarQuote && <Panel title="Devis vérifié par Rust"><div className="quote-summary"><div><span>Produit</span><strong>{text(solarQuote.product_code)}</strong></div><div><span>Montant approuvé</span><strong>{formatMoney(solarQuote.principal, solarQuote.currency)}</strong></div><div><span>Conditions</span><strong>{text(solarQuote.terms_revision)}</strong></div></div><div className="form-actions"><Button primary onClick={() => void confirmSolarSale()}>Confirmer et créer la vente</Button></div></Panel>}
        <div className="metrics solar-metrics">
          <Metric label="Produits disponibles" value={solarProducts.length} note="Catalogue local approuvé" />
          <Metric label="Ventes enregistrées" value={sales.length} note="Historique de ce poste" />
          <Metric label="Contrats à échéances" value={sales.filter((sale) => Boolean(sale.installment_plan_id)).length} note="Échéanciers associés" />
          <Metric label="À synchroniser" value={pending} note="Opérations en attente" />
        </div>
        <Panel title="Catalogue approuvé">
          <Table headers={["SKU", "Produit", "Catégorie", "Prix de référence", "État"]} empty="Aucun produit solaire approuvé et applicable n’est disponible sur ce poste." data={solarProducts.map((product) => [<strong>{text(product.sku)}</strong>, text(product.label), product.category ? domainLabel(product.category) : "—", formatMoney(product.price_reference, product.currency), <span className="status-badge success">Disponible</span>])} />
        </Panel>
        <Panel title="Historique des ventes"><Table headers={["Référence", "Abonné", "Produit", "Principal", "État"]} empty="Aucune vente solaire locale." data={sales.map((row) => [<strong>{text(row.sale_id)}</strong>, customerLabel(row.customer_id, customers), text(row.product_code), formatMoney(row.principal, row.currency), <Status value={row.status} />])} /></Panel>
        <SolarContractManager sales={sales} customers={customers} onCreated={refresh} />
      </>}

      {view === "receivables" && <>
        <PageHeading eyebrow="FINANCE / SOLDES" title="Suivi des créances" description="Soldes et obligations renvoyés par l’agent local, sans recalcul dans l’interface." />
        <Panel title="Situation d’un abonné"><label className="customer-picker">Abonné<select value={selectedCustomer} onChange={(event) => setSelectedCustomer(event.target.value)}><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          {statement ? <Table headers={["Référence", "Nature", "Solde", "Situation"]} empty="Aucune créance associée." data={rows<Row>(statement.receivables).map((row) => [<strong>{text(row.receivable_id ?? row.id)}</strong>, domainLabel(row.source_type), formatMoney(row.balance, row.currency), <Status value={row.status} />])} /> : <div className="empty-state">Choisissez un abonné pour consulter son relevé et ses créances.</div>}
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

function PageHeading({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: ReactNode }) {
  return <div className="page-heading"><div><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p className="muted">{description}</p></div>{action}</div>;
}

function Metric({ label, value, note }: { label: string; value: ReactNode; note: string }) {
  return <div className="metric"><div className="metric-label">{label}</div><div className="metric-value">{value}</div><div className="metric-note">{note}</div></div>;
}

function Status({ value }: { value: unknown }) {
  const normalized = text(value, "État inconnu");
  const labels: Record<string, string> = {
    LOCAL_PENDING: "En attente de synchronisation",
    CENTRAL_ACCEPTED: "Confirmé par le serveur",
    LOCAL_ACCEPTED: "Enregistré sur ce poste",
    LOCAL_READY: "Poste prêt",
    PENDING_NETWORK: "En attente du réseau",
    LOCAL_ONLY: "Fonctionnement local",
    UPCOMING: "À venir",
    OVERDUE: "En retard",
    PAID: "Payé",
    PARTIAL: "Partiellement payé",
    OPEN: "À régler",
    SETTLED: "Réglé",
    CREDITED: "Avoir enregistré",
    CONFLICT: "Conflit à examiner",
    REJECTED: "Refusé",
  };
  const state = normalized.toLocaleLowerCase("en").includes("pending") || normalized.toLocaleLowerCase("fr").includes("attente") ? "pending"
    : normalized.toLocaleLowerCase("en").includes("conflict") || normalized.toLocaleLowerCase("fr").includes("conflit") || normalized.toLocaleLowerCase("en").includes("reject") ? "danger"
      : normalized.toLocaleLowerCase("en").includes("accept") || normalized.toLocaleLowerCase("en").includes("ready") ? "success" : "neutral";
  return <span className={`status-badge ${state}`}>{labels[normalized] ?? normalized}</span>;
}

function Preview({ data, onConfirm, title = "Prévisualisation calculée par Rust", action }: { data: Row; onConfirm?: () => void; title?: string; action?: ReactNode }) {
  const lines = rows<Row>(data.lines);
  return <Panel title={title} action={action}>{onConfirm && <p className="help">Vérifiez les deux services, les taxes et le total. Cette prévisualisation ne crée pas de facture.</p>}
      <Table headers={["Service", "Quantité", "Prix unitaire", "Sous-total", "Taxe", "Total"]} empty="Le moteur n’a retourné aucune ligne." data={lines.map((line) => [domainLabel(line.service_code), formatQuantity(line.quantity, line.unit), formatMoney(line.unit_price, data.currency), formatMoney(line.subtotal, data.currency), formatMoney(line.tax, data.currency), formatMoney(line.total, data.currency)])} />
      <div className="totals"><span>Sous-total <strong>{formatMoney(data.subtotal, data.currency)}</strong></span><span>Taxes <strong>{formatMoney(data.tax, data.currency)}</strong></span><span>Total <strong>{formatMoney(data.total, data.currency)}</strong></span></div>
    {onConfirm && <div className="form-actions"><Button primary onClick={onConfirm}>Émettre cette facture</Button></div>}
  </Panel>;
}

function PaymentForm({ customers, sales, onSubmit }: { customers: Row[]; sales: Row[]; onSubmit: (event: FormEvent<HTMLFormElement>, path: string, payload: Row, success: string) => Promise<boolean> }) {
  const [customerId, setCustomerId] = useState("");
  const [receivableId, setReceivableId] = useState("");
  const [receivables, setReceivables] = useState<Row[]>([]);
  const [receivablesLoading, setReceivablesLoading] = useState(false);
  const [receivablesError, setReceivablesError] = useState("");
  const [scheduleRows, setScheduleRows] = useState<Row[]>([]);
  const [scheduleId, setScheduleId] = useState("");
  const [scheduleMessage, setScheduleMessage] = useState("");
  const [scheduleCurrency, setScheduleCurrency] = useState("");
  const [scheduleLoading, setScheduleLoading] = useState(false);
  const [scheduleError, setScheduleError] = useState("");
  const [currency, setCurrency] = useState("USD");
  const [receivedAmount, setReceivedAmount] = useState("");
  const [occurredAt, setOccurredAt] = useState(localDateTimeValue);
  useEffect(() => {
    if (!customerId) { setReceivables([]); setReceivableId(""); setReceivablesLoading(false); setReceivablesError(""); setCurrency("USD"); return; }
    let active = true;
    setReceivables([]); setReceivableId(""); setReceivablesLoading(true); setReceivablesError(""); setCurrency("USD");
    localRequest<Row>(`/local/v1/customers/${encodeURIComponent(customerId)}/statement`).then((result) => {
      if (active) { setReceivables(rows<Row>(result.receivables).filter((item) => Number(item.balance) > 0)); setReceivableId(""); }
    }).catch((error: unknown) => { if (active) { setReceivables([]); setReceivableId(""); setReceivablesError(humanError(error)); } })
      .finally(() => { if (active) setReceivablesLoading(false); });
    return () => { active = false; };
  }, [customerId]);

  useEffect(() => {
    const sale = sales.find((item) => text(item.receivable_id) === receivableId && item.installment_plan_id);
    if (!sale?.installment_plan_id) { setScheduleRows([]); setScheduleId(""); setScheduleCurrency(""); setScheduleMessage(""); setScheduleLoading(false); setScheduleError(""); return; }
    let active = true;
    setScheduleRows([]);
    setScheduleId("");
    setScheduleCurrency("");
    setScheduleMessage("");
    setScheduleError("");
    setScheduleLoading(true);
    localRequest<Row>(`/local/v1/installment-plans/${encodeURIComponent(text(sale.installment_plan_id))}`)
      .then((result) => { if (active) { const installments = rows<Row>(result.schedule); setScheduleRows(installments); setScheduleCurrency(text(result.currency, "")); setCurrency(text(result.currency, "USD")); if (!installments.some((item) => text(item.status).toUpperCase() !== "PAID")) { setScheduleError("Aucune échéance ouverte n’est disponible pour cette créance."); setScheduleMessage("Le règlement est bloqué tant qu’une échéance payable n’est pas disponible."); } } })
      .catch((error: unknown) => { if (active) { setScheduleRows([]); setScheduleCurrency(""); setScheduleError(humanError(error)); setScheduleMessage("Impossible de vérifier l’échéancier. Le règlement est bloqué."); } })
      .finally(() => { if (active) setScheduleLoading(false); });
    return () => { active = false; };
  }, [receivableId, sales]);

  const selectedSale = sales.find((item) => text(item.receivable_id) === receivableId && item.installment_plan_id);
  const selectedReceivable = receivables.find((item) => text(item.receivable_id ?? item.id) === receivableId);
  const scheduleRequired = Boolean(selectedSale?.installment_plan_id);
  const openScheduleRows = scheduleRows.filter((item) => text(item.status).toUpperCase() !== "PAID");
  const paymentBlocked = receivablesLoading || Boolean(receivablesError) || !receivableId || scheduleLoading || Boolean(scheduleError) || (scheduleRequired && (!openScheduleRows.length || !scheduleId));

  return <div className="payment-workspace"><Panel title="Informations du règlement"><form id="local-payment-form" className="form-grid" onSubmit={async (event) => { const form = new FormData(event.currentTarget); const saved = await onSubmit(event, "/local/v1/payments", {
    customer_id: form.get("customer_id"), received_amount: form.get("received_amount"), currency: form.get("currency"), method: form.get("method"), occurred_at: new Date(String(form.get("occurred_at"))).toISOString(),
    allocations: [{ receivable_id: form.get("receivable_id"), amount: form.get("received_amount"), schedule_id: form.get("schedule_id") || null }],
  }, "Règlement enregistré sur ce poste."); if (saved) { setCustomerId(""); setReceivableId(""); setScheduleId(""); setCurrency("USD"); setReceivedAmount(""); setOccurredAt(localDateTimeValue()); } }}>
    <label>Abonné<select name="customer_id" value={customerId} onChange={(event) => { setCustomerId(event.target.value); setReceivableId(""); setScheduleId(""); setCurrency("USD"); }} required><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
    <label>Créance à affecter<select name="receivable_id" value={receivableId} onChange={(event) => { setReceivableId(event.target.value); setScheduleId(""); const selected = receivables.find((item) => text(item.receivable_id ?? item.id) === event.target.value); setCurrency(text(selected?.currency, "USD")); }} required disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length}><option value="">{!customerId ? "Sélectionnez d’abord un abonné" : receivablesLoading ? "Chargement des créances…" : "Sélectionner une créance"}</option>{receivables.map((item) => <option key={text(item.receivable_id ?? item.id)} value={text(item.receivable_id ?? item.id)}>{domainLabel(item.source_type ?? item.status)} · {formatMoney(item.balance, item.currency)}</option>)}</select></label>
    <label>Montant reçu<input name="received_amount" type="number" min="0.01" step="0.01" value={receivedAmount} onChange={(event) => setReceivedAmount(event.target.value)} required /></label>
    <label>Mode<select name="method"><option value="CASH">Espèces</option><option value="BANK">Banque</option><option value="MOBILE_MONEY">Mobile Money</option><option value="OTHER">Autre</option></select></label>
    <label>Date du règlement<input name="occurred_at" type="datetime-local" value={occurredAt} onChange={(event) => setOccurredAt(event.target.value)} required /></label>
    <label>Devise<input name="currency" value={currency} readOnly required /></label>
    <label>Affectation<input value={receivableId ? "Créance sélectionnée" : "À définir avec la créance"} readOnly /></label>
    <label>Référence interne<input value="Générée par Rust à l’enregistrement" readOnly /></label>
    {receivablesError && <p className="help error-text span-two" role="alert">Impossible de charger les créances : {receivablesError}</p>}
    {scheduleRequired && <label className="span-two">Échéance solaire<select name="schedule_id" value={scheduleId} onChange={(event) => setScheduleId(event.target.value)} required disabled={scheduleLoading || Boolean(scheduleError)}><option value="">{scheduleLoading ? "Chargement de l’échéancier…" : "Sélectionner une échéance…"}</option>{openScheduleRows.map((item) => <option key={text(item.id)} value={text(item.id)}>N° {text(item.sequence)} · {text(item.due_date)} · restant {formatMoney(item.remaining, scheduleCurrency)}</option>)}</select></label>}
    {scheduleMessage && <p className="help span-two" role="status">{scheduleMessage}</p>}
  </form></Panel><Panel title="Récapitulatif"><div className="payment-summary"><p className="eyebrow">PRÉPARATION DU RÈGLEMENT</p><h2>Règlement</h2><p className="muted">{selectedReceivable ? `${domainLabel(selectedReceivable.source_type ?? selectedReceivable.status)} · solde retourné par Rust` : "Choisissez un abonné puis une créance à affecter."}</p><div className="payment-summary-values"><div><span>Abonné</span><strong>{customerId ? customerLabel(customerId, customers) : "—"}</strong></div><div><span>Créance sélectionnée</span><strong>{selectedReceivable ? text(selectedReceivable.receivable_number ?? selectedReceivable.receivable_id ?? selectedReceivable.id) : "—"}</strong></div><div><span>Solde actuel</span><strong>{selectedReceivable ? formatMoney(selectedReceivable.balance, selectedReceivable.currency) : "—"}</strong></div></div><p className="eyebrow">MONTANT À CONFIRMER</p><strong className="payment-amount">{receivedAmount ? formatMoney(receivedAmount, currency) : "—"}</strong><span className={`status-badge${selectedReceivable ? " pending" : ""}`}>{selectedReceivable ? "En attente d’enregistrement" : "Aucune créance sélectionnée"}</span><p className="help">Le solde restant est recalculé et validé par Rust à l’enregistrement.</p><Button primary type="submit" form="local-payment-form" disabled={paymentBlocked}>Enregistrer le règlement</Button></div></Panel></div>;
}

function MeterReadingsView({ customers }: { customers: Row[] }) {
  const [customerId, setCustomerId] = useState("");
  const [accounts, setAccounts] = useState<Row[]>([]);
  const [accountId, setAccountId] = useState("");
  const [meters, setMeters] = useState<Row[]>([]);
  const [meterId, setMeterId] = useState("");
  const [readings, setReadings] = useState<Row[]>([]);
  const [readingPeriod, setReadingPeriod] = useState("");
  const [previousIndex, setPreviousIndex] = useState("");
  const [correctingReading, setCorrectingReading] = useState<Row | null>(null);
  const [message, setMessage] = useState("");
  const [messageError, setMessageError] = useState(false);
  const [saving, setSaving] = useState(false);
  const [preview, setPreview] = useState<Row | null>(null);
  const [previewInput, setPreviewInput] = useState<Row | null>(null);

  useEffect(() => {
    if (!customerId) { setAccounts([]); setAccountId(""); setMeters([]); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); return; }
    let active = true;
    localRequest<unknown>(`/local/v1/customers/${encodeURIComponent(customerId)}/service-accounts`).then((result) => {
      if (active) { setAccounts(rows(result)); setAccountId(""); setMeters([]); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); }
    }).catch((error: unknown) => { if (active) { setMessage(humanError(error)); setMessageError(true); } });
    return () => { active = false; };
  }, [customerId]);

  useEffect(() => {
    if (!accountId) { setMeters([]); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); return; }
    let active = true;
    localRequest<unknown>(`/local/v1/service-accounts/${encodeURIComponent(accountId)}/meters`).then((result) => {
      if (active) { setMeters(rows(result).filter((meter) => meter.active !== false)); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); }
    }).catch((error: unknown) => { if (active) { setMessage(humanError(error)); setMessageError(true); } });
    return () => { active = false; };
  }, [accountId]);

  useEffect(() => {
    if (!meterId) { setReadings([]); setReadingPeriod(""); setPreviousIndex(""); return; }
    let active = true;
    localRequest<unknown>(`/local/v1/meters/${encodeURIComponent(meterId)}/readings`).then((result) => {
      if (active) { const nextReadings = rows(result); setReadings(nextReadings); setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : ""); }
    }).catch((error: unknown) => { if (active) { setMessage(humanError(error)); setMessageError(true); } });
    return () => { active = false; };
  }, [meterId]);

  async function previewReading(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!meterId) return;
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      const form = new FormData(event.currentTarget);
      const input = { period: form.get("period"), previous: form.get("previous"), current: form.get("current"), source: "MANUAL" };
      const meterBase = `/local/v1/meters/${encodeURIComponent(meterId)}/readings`;
      const result = await localRequest<Row>(`${meterBase}/preview`, "POST", input);
      setPreview(result);
      setPreviewInput(input);
    } catch (error) { setPreview(null); setPreviewInput(null); setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function saveReading() {
    if (!meterId || !previewInput) return;
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await localRequest(`/local/v1/meters/${encodeURIComponent(meterId)}/readings`, "POST", previewInput);
      const result = await localRequest<unknown>(`/local/v1/meters/${encodeURIComponent(meterId)}/readings`);
      const nextReadings = rows(result);
      setReadings(nextReadings);
      setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : "");
      setReadingPeriod("");
      setPreview(null); setPreviewInput(null);
      setMessage("Relevé enregistré sur ce poste."); setMessageError(false);
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function correctReading(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!correctingReading) return;
    const form = new FormData(event.currentTarget);
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await localRequest(`/local/v1/meter-readings/${encodeURIComponent(text(correctingReading.id))}/corrections`, "POST", {
        current: form.get("current"), source: "MANUAL", reason: form.get("reason"),
      });
      const result = await localRequest<unknown>(`/local/v1/meters/${encodeURIComponent(meterId)}/readings`);
      const nextReadings = rows(result);
      setReadings(nextReadings); setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : ""); setCorrectingReading(null);
      setMessage("Correction ajoutée à l’historique. Le relevé initial est conservé."); setMessageError(false);
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  const latestReading = readings.length ? readings[readings.length - 1] : null;

  const selectedMeter = meters.find((meter) => text(meter.id) === meterId);
  const isWaterMeter = text(selectedMeter?.unit).toUpperCase() === "M3";

  return <>
    <PageHeading eyebrow="CORE / RELEVÉS" title={isWaterMeter ? "Relevé d’eau au m³" : "Relevés de compteurs"} description={isWaterMeter ? "Calculer la consommation d’eau à partir des index du compteur." : "Consultez l’historique et enregistrez les index. La consommation mesurée est calculée et validée par Rust."} />
    <div className="reading-workspace">
      <Panel title={isWaterMeter ? "Saisie du relevé · Eau" : "Paramètres de facturation"}>
        <div className="form-grid reading-filters">
          <label>Abonné<select value={customerId} onChange={(event) => { setCustomerId(event.target.value); setPreview(null); setPreviewInput(null); }}><option value="">Sélectionner…</option>{customers.map((customer) => <option key={text(customer.customer_id ?? customer.id)} value={text(customer.customer_id ?? customer.id)}>{text(customer.customer_code)} — {text(customer.legal_name)}</option>)}</select></label>
          <label>Service<select value={accountId} onChange={(event) => { setAccountId(event.target.value); setPreview(null); setPreviewInput(null); }} disabled={!customerId}><option value="">{customerId ? "Sélectionner…" : "Sélectionnez d’abord un abonné"}</option>{accounts.map((account) => <option key={text(account.id)} value={text(account.id)}>{domainLabel(account.service_code)} · {domainLabel(account.connection_status)}</option>)}</select></label>
          <label className="span-two">Compteur<select value={meterId} onChange={(event) => { setMeterId(event.target.value); setPreview(null); setPreviewInput(null); }} disabled={!accountId}><option value="">{accountId ? "Sélectionner…" : "Sélectionnez d’abord un service"}</option>{meters.map((meter) => <option key={text(meter.id)} value={text(meter.id)}>{text(meter.serial)} · {formatUnit(meter.unit)}</option>)}</select></label>
        </div>
        {meterId && <form className="form-grid reading-entry" onSubmit={(event) => void previewReading(event)} onChange={() => { setPreview(null); setPreviewInput(null); }}>
          <label>Période<input name="period" type="month" value={readingPeriod} onChange={(event) => { const selectedPeriod = event.target.value; setReadingPeriod(selectedPeriod); const preceding = readings.filter((reading) => text(reading.period) < selectedPeriod); setPreviousIndex(preceding.length ? text(preceding[preceding.length - 1].current) : ""); }} required /></label>
          <label>Index précédent<input name="previous" type="number" min="0" step="0.001" value={previousIndex} onChange={(event) => setPreviousIndex(event.target.value)} required /></label>
          <label>Nouvel index<input name="current" type="number" min="0" step="0.001" required /></label>
          <label>Source du relevé<input value="Saisie sur ce poste" readOnly /></label>
          <p className="help span-two">La consommation est calculée et validée par le moteur Rust avant l’enregistrement.</p>
          <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Vérification…" : "Prévisualiser les index"}</Button></div>
        </form>}
        {message && <p role={messageError ? "alert" : "status"} className={`reading-message${messageError ? " error" : ""}`}>{message}</p>}
        {!customerId && <div className="empty-state">Choisissez un abonné pour afficher ses services et ses compteurs.</div>}
        {customerId && !accounts.length && <div className="empty-state">Aucun compte de service n’est enregistré pour cet abonné sur ce poste.</div>}
        {accountId && !meters.length && <div className="empty-state">Aucun compteur actif n’est associé à ce service.</div>}
      </Panel>
      <Panel title="Récapitulatif">
        <div className="reading-summary">
          <p className="eyebrow">PRÉPARATION DU RELEVÉ</p>
          <h2>{preview ? "Index vérifiés" : "À vérifier"}</h2>
          <p className="muted">{preview ? `${formatUnit(selectedMeter?.unit)} · ${text(preview.period)}` : "Le moteur calcule la différence entre les index avant l’enregistrement."}</p>
          <dl><div><dt>Compteur</dt><dd>{selectedMeter ? text(selectedMeter.serial) : "—"}</dd></div><div><dt>Index précédent</dt><dd>{preview ? formatQuantity(preview.previous, selectedMeter?.unit) : "—"}</dd></div><div><dt>Index actuel</dt><dd>{preview ? formatQuantity(preview.current, selectedMeter?.unit) : "—"}</dd></div></dl>
          <p className="eyebrow">CONSOMMATION MESURÉE</p>
          <strong className="reading-quantity">{preview ? formatQuantity(preview.measured_quantity, selectedMeter?.unit) : "—"}</strong>
          <span className={`status-badge${preview ? " pending" : ""}`}>{preview ? "Brouillon local · non enregistré" : "En attente de vérification"}</span>
          <p className="help">Les index sont revalidés par Rust au moment de l’enregistrement.</p>
          {preview && <Button primary onClick={() => void saveReading()} disabled={saving}>{saving ? "Enregistrement…" : "Valider les index"}</Button>}
        </div>
      </Panel>
    </div>
    {meterId && <Panel title="Historique des relevés"><Table headers={["Période", "Index précédent", "Nouvel index", "Consommation", "Source", "Correction", "Action"]} empty="Aucun relevé enregistré pour ce compteur." data={readings.map((reading) => [text(reading.period), formatQuantity(reading.previous, selectedMeter?.unit), formatQuantity(reading.current, selectedMeter?.unit), formatQuantity(reading.measured_quantity, selectedMeter?.unit), text(reading.source), reading.corrected_from_id ? text(reading.correction_reason) : "—", reading.id === latestReading?.id ? <button className="text-button" type="button" onClick={() => setCorrectingReading(reading)}>Corriger le dernier relevé</button> : "—"])} /></Panel>}
    {correctingReading && <Panel title={`Correction du relevé · ${text(correctingReading.period)}`}><form className="form-grid" onSubmit={(event) => void correctReading(event)}>
      <label>Nouvel index corrigé<input name="current" type="number" min="0" step="0.001" defaultValue={text(correctingReading.current, "")} required /></label>
      <label className="span-two">Motif de correction<input name="reason" maxLength={500} required /></label>
      <p className="help span-two">La correction crée une nouvelle entrée d’audit et ne supprime pas le relevé initial.</p>
      <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer la correction"}</Button><Button type="button" onClick={() => setCorrectingReading(null)}>Annuler</Button></div>
    </form></Panel>}
  </>;
}

function SolarContractManager({ sales, customers, onCreated }: { sales: Row[]; customers: Row[]; onCreated: () => Promise<void> }) {
  const [saleId, setSaleId] = useState("");
  const [schedule, setSchedule] = useState([{ due_date: "", amount: "" }]);
  const [plan, setPlan] = useState<Row | null>(null);
  const [message, setMessage] = useState("");
  const [saving, setSaving] = useState(false);
  const availableSales = sales.filter((sale) => sale.receivable_id && !sale.installment_plan_id);

  async function viewPlan(planId: string) {
    try { setPlan(await localRequest<Row>(`/local/v1/installment-plans/${encodeURIComponent(planId)}`)); }
    catch (error) { setMessage(humanError(error)); }
  }

  async function saveContract(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const sale = availableSales.find((item) => text(item.sale_id) === saleId);
    const form = new FormData(event.currentTarget);
    if (!sale || !sale.receivable_id) return;
    setSaving(true); setMessage("");
    try {
      const result = await localRequest<Row>("/local/v1/solar/contracts", "POST", {
        sale_id: sale.sale_id,
        receivable_id: sale.receivable_id,
        schedule: schedule.map((entry, index) => ({ sequence: index + 1, due_date: entry.due_date, amount: entry.amount })),
        currency: sale.currency,
        approved_terms_revision: form.get("approved_terms_revision"),
      });
      const planId = text(result.installment_plan_id, "");
      if (planId) setPlan(await localRequest<Row>(`/local/v1/installment-plans/${encodeURIComponent(planId)}`));
      setMessage("Contrat et échéancier enregistrés sur ce poste.");
      setSaleId(""); setSchedule([{ due_date: "", amount: "" }]);
      await onCreated();
    } catch (error) { setMessage(humanError(error)); }
    finally { setSaving(false); }
  }

  const planRows = rows<Row>(plan?.schedule);
  return <>
    <Panel title="Contrats et échéanciers solaires">
      {availableSales.length ? <form className="form-grid" onSubmit={(event) => void saveContract(event)}>
        <label className="span-two">Vente à financer<select value={saleId} onChange={(event) => setSaleId(event.target.value)} required><option value="">Sélectionner une vente…</option>{availableSales.map((sale) => <option key={text(sale.sale_id)} value={text(sale.sale_id)}>{customerLabel(sale.customer_id, customers)} · {text(sale.sale_id)} · {formatMoney(sale.principal, sale.currency)}</option>)}</select></label>
        {schedule.map((entry, index) => <div className="schedule-entry span-two" key={index}>
          <label>Échéance {index + 1} · Date<input type="date" required value={entry.due_date} onChange={(event) => setSchedule((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, due_date: event.target.value } : item))} /></label>
          <label>Montant dû<input type="number" min="0.01" step="0.01" required value={entry.amount} onChange={(event) => setSchedule((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, amount: event.target.value } : item))} /></label>
          {schedule.length > 1 && <button className="text-button remove-installment" type="button" onClick={() => setSchedule((current) => current.filter((_, itemIndex) => itemIndex !== index))}>Retirer</button>}
        </div>)}
        <div className="schedule-actions span-two"><button className="text-button" type="button" onClick={() => setSchedule((current) => [...current, { due_date: "", amount: "" }])}>+ Ajouter une échéance</button></div>
        <label className="span-two">Référence des conditions approuvées<input name="approved_terms_revision" required /></label>
        <p className="help span-two">Le moteur vérifie les échéances et leur total par rapport à la créance. Aucun intérêt ni frais n’est ajouté.</p>
        <div className="form-actions span-two"><Button primary type="submit" disabled={saving || !saleId}>{saving ? "Enregistrement…" : "Créer le contrat"}</Button></div>
      </form> : <div className="empty-state">Aucune vente sans échéancier n’est disponible pour créer un contrat.</div>}
      {message && <p role="status" className="reading-message">{message}</p>}
      {sales.some((sale) => sale.installment_plan_id) && <div className="table-scroll"><table><thead><tr><th>Abonné</th><th>Vente</th><th>Montant</th><th>Situation</th><th>Échéancier</th></tr></thead><tbody>{sales.filter((sale) => sale.installment_plan_id).map((sale) => <tr key={text(sale.sale_id)}><td>{customerLabel(sale.customer_id, customers)}</td><td><strong>{text(sale.sale_id)}</strong></td><td>{formatMoney(sale.principal, sale.currency)}</td><td><Status value={sale.status} /></td><td><button className="text-button" onClick={() => void viewPlan(text(sale.installment_plan_id))}>Consulter</button></td></tr>)}</tbody></table></div>}
    </Panel>
    {plan && <Panel title="Échéancier consulté"><div className="metrics compact-metrics"><Metric label="Principal" value={formatMoney(plan.principal, plan.currency)} note="Créance du contrat" /><Metric label="Payé" value={formatMoney(plan.paid, plan.currency)} note="Règlements affectés" /><Metric label="Restant" value={formatMoney(plan.remaining, plan.currency)} note="Solde de la créance" /><Metric label="En retard" value={formatMoney(plan.overdue_amount, plan.currency)} note="Échéances échues" /></div><Table headers={["N°", "Échéance", "Montant dû", "Payé", "Restant", "État"]} empty="Aucune échéance." data={planRows.map((item) => [text(item.sequence), text(item.due_date), formatMoney(item.amount_due, plan.currency), formatMoney(item.paid, plan.currency), formatMoney(item.remaining, plan.currency), <Status value={item.status} />])} /></Panel>}
  </>;
}

function CustomerStatement({ customerName, customerCode, statement }: { customerName: string; customerCode: string; statement: Row | null }) {
  return <Panel title={`Situation du compte · ${customerName}`}>
    {statement ? <><div className="dossier-metrics metrics compact-metrics"><Metric label="Compte client" value={customerCode} note="Référence abonné" /><Metric label="Factures" value={rows(statement.invoices).length} note="Documents conservés" /><Metric label="Versements" value={rows(statement.payments).length} note="Encaissements enregistrés" /><Metric label="Créances" value={rows(statement.receivables).length} note="Obligations suivies" /></div><div className="columns"><div><p><strong>Factures</strong></p><Table headers={["Référence", "Période", "État"]} empty="Aucune facture dans le relevé." data={rows<Row>(statement.invoices).map((item) => [text(item.invoice_number ?? item.invoice_id), text(item.period ?? item.billing_period), <Status value={item.status} />])} /></div><div><p><strong>Créances</strong></p><Table headers={["Référence", "Solde", "Situation"]} empty="Aucune créance dans le relevé." data={rows<Row>(statement.receivables).map((item) => [text(item.id ?? item.receivable_id), formatMoney(item.balance, item.currency), <Status value={item.status} />])} /></div></div></> : <div className="empty-state">Chargement du relevé abonné…</div>}
  </Panel>;
}

function AdminCard({ title, description, state }: { title: string; description: string; state: string }) {
  return <section className="panel admin-card"><p className="eyebrow">ADMINISTRATION</p><h2>{title}</h2><p className="muted">{description}</p><span className="status-badge pending">{state}</span></section>;
}
