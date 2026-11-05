import { useCallback, useEffect, useMemo, useRef, useState, type ButtonHTMLAttributes, type FormEvent, type ReactNode } from "react";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { centralAdminRequest, centralLogin, centralLogout, centralSessionStatus, centralRequest, centralStatus, openMonarqueSite, type CentralSessionInfo } from "./api";
import { buildClientPdf, renderClientPdfPreview } from "./documents";
import "./App.css";

type View = "dashboard" | "customers" | "readings" | "invoices" | "payments" | "solar" | "receivables" | "server" | "admin";
type Row = Record<string, unknown>;
type Notice = { message: string; error?: boolean } | null;
type PreviewDocument = { title: string; filename: string; blob: Blob };

const navigation: { id: View; label: string; icon: string }[] = [
  { id: "dashboard", label: "Vue d’ensemble", icon: "grid" },
  { id: "customers", label: "Abonnés", icon: "people" },
  { id: "readings", label: "Relevés", icon: "meter" },
  { id: "invoices", label: "Factures", icon: "document" },
  { id: "payments", label: "Encaissements", icon: "cash" },
  { id: "solar", label: "Kits solaires", icon: "sun" },
  { id: "receivables", label: "Créances", icon: "balance" },
  { id: "server", label: "Serveur", icon: "server" },
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
  server: { title: "État du serveur", context: "SEL / SERVICE CENTRAL" },
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

function dateTimeInputValue(date = new Date()): string {
  const wallClockDate = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return wallClockDate.toISOString().slice(0, 16);
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
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return "L’opération n’a pas abouti. Vérifiez la connexion au service SEL et réessayez.";
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
    server: <><rect x="3" y="3" width="14" height="5" rx="1" /><rect x="3" y="12" width="14" height="5" rx="1" /><path d="M6 5.5h.1M6 14.5h.1M9 5.5h5M9 14.5h5" /></>,
    settings: <><path d="m8.6 2.8.4-1h2l.4 1a7.6 7.6 0 0 1 1.6.7l1-.4 1.4 1.4-.4 1a7.6 7.6 0 0 1 .7 1.6l1 .4v2l-1 .4a7.6 7.6 0 0 1-.7 1.6l.4 1-1.4 1.4-1-.4a7.6 7.6 0 0 1-1.6.7l-.4 1h-2l-.4-1a7.6 7.6 0 0 1-1.6-.7l-1 .4-1.4-1.4.4-1a7.6 7.6 0 0 1-.7-1.6l-1-.4v-2l1-.4A7.6 7.6 0 0 1 5 5.5l-.4-1L6 3.1l1 .4a7.6 7.6 0 0 1 1.6-.7Z" transform="translate(1.5 1.5) scale(.85)" /><circle cx="10" cy="10" r="2.5" /></>,
  };
  return <svg className="nav-icon" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">{icons[name] ?? icons.grid}</svg>;
}

export default function App() {
  const [session, setSession] = useState<CentralSessionInfo | null>(null);
  const [authChecking, setAuthChecking] = useState(true);
  const [authMessage, setAuthMessage] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  const [serverReady, setServerReady] = useState<boolean | null>(null);
  const [serverReachable, setServerReachable] = useState<boolean | null>(null);
  const [view, setView] = useState<View>("dashboard");
  const [customers, setCustomers] = useState<Row[]>([]);
  const [invoices, setInvoices] = useState<Row[]>([]);
  const [payments, setPayments] = useState<Row[]>([]);
  const [sales, setSales] = useState<Row[]>([]);
  const [solarProducts, setSolarProducts] = useState<Row[]>([]);
  const [solarQuote, setSolarQuote] = useState<Row | null>(null);
  const [solarSaleDraft, setSolarSaleDraft] = useState<Row | null>(null);
  const [solarSaleOpen, setSolarSaleOpen] = useState(false);
  const [serverStatus, setServerStatus] = useState<Row | null>(null);
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
  const [documentPreview, setDocumentPreview] = useState<PreviewDocument | null>(null);
  const [documentBusy, setDocumentBusy] = useState(false);

  useEffect(() => {
    let active = true;
    Promise.allSettled([centralSessionStatus(), centralStatus<Row>()]).then(([sessionResult, healthResult]) => {
      if (!active) return;
      if (sessionResult.status === "fulfilled") {
        setSession(sessionResult.value);
        if (!sessionResult.value.authenticated && sessionResult.value.server_reachable === false) setAuthMessage("Le serveur SEL est inaccessible. Vérifiez votre connexion Internet puis réessayez.");
      } else setAuthMessage(humanError(sessionResult.reason));
      if (healthResult.status === "fulfilled") {
        const ready = healthResult.value.status === "CENTRAL_CONNECTED" && healthResult.value.configured === true;
        setServerReachable(true);
        setServerReady(ready);
        if (!ready) setAuthMessage("Le serveur SEL est joignable. Un administrateur doit terminer la configuration avant la facturation.");
      } else {
        setServerReachable(false);
        setServerReady(false);
        if (sessionResult.status !== "fulfilled" || !sessionResult.value.authenticated) setAuthMessage("Le serveur SEL est inaccessible. Une connexion Internet est nécessaire pour ouvrir l’application.");
      }
    }).finally(() => { if (active) setAuthChecking(false); });
    const expired = () => { setSession({ authenticated: false }); setAuthMessage("Votre session a expiré. Connectez-vous à nouveau."); };
    const unavailable = () => {
      setServerReachable(false);
      setServerReady(false);
      setCustomers([]); setInvoices([]); setPayments([]); setSales([]); setSolarProducts([]);
      setStatement(null); setSelectedCustomerData(null); setSelectedInvoice(null); setPreview(null);
      setNotice({ message: "Le serveur SEL est indisponible ou non configuré. Les données métier sont masquées jusqu’au rétablissement du service.", error: true });
    };
    window.addEventListener("sel-session-expired", expired);
    window.addEventListener("sel-server-unavailable", unavailable);
    return () => { active = false; window.removeEventListener("sel-session-expired", expired); window.removeEventListener("sel-server-unavailable", unavailable); };
  }, []);

  const refresh = useCallback(async () => {
    if (!session?.authenticated) return;
    setLoading(true);
    try {
      const [customerResult, invoiceResult, paymentResult, salesResult, productsResult, statusResult] = await Promise.allSettled([
        centralRequest<unknown>("/v1/customers"),
        centralRequest<unknown>("/v1/invoices?limit=100"),
        centralRequest<unknown>("/v1/payments?limit=100"),
        centralRequest<unknown>("/v1/solar/sales?limit=100"),
        centralRequest<unknown>("/v1/catalog/products"),
        centralStatus<Row>(),
      ]);
      setCustomers(customerResult.status === "fulfilled" ? rows(customerResult.value) : []);
      setInvoices(invoiceResult.status === "fulfilled" ? rows(invoiceResult.value) : []);
      setPayments(paymentResult.status === "fulfilled" ? rows(paymentResult.value) : []);
      setSales(salesResult.status === "fulfilled" ? rows(salesResult.value) : []);
      setSolarProducts(productsResult.status === "fulfilled" ? rows(productsResult.value) : []);
      if (statusResult.status === "fulfilled") {
        setServerReachable(true);
        setServerStatus(statusResult.value);
        setServerReady(statusResult.value.status === "CENTRAL_CONNECTED" && statusResult.value.configured === true);
      } else {
        setServerReachable(false);
        setServerStatus(null);
        setServerReady(false);
        window.dispatchEvent(new Event("sel-server-unavailable"));
      }
      const failure = [customerResult, invoiceResult, paymentResult, salesResult, productsResult, statusResult].find((result) => result.status === "rejected");
      setNotice(failure?.status === "rejected"
        ? { message: humanError(failure.reason), error: true }
        : statusResult.status === "fulfilled" && statusResult.value.configured !== true
          ? { message: "Le serveur SEL répond, mais sa configuration de production n’est pas prête. Les opérations restent bloquées.", error: true }
          : null);
    } finally {
      setLoading(false);
    }
  }, [session?.authenticated]);

  useEffect(() => { if (session?.authenticated) void refresh(); }, [session?.authenticated, refresh]);

  async function signIn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    setAuthBusy(true); setAuthMessage("");
    try {
      const result = await centralLogin(String(values.get("identity_ref") ?? "").trim(), String(values.get("password") ?? ""));
      setSession({ authenticated: true, ...result });
      setAuthMessage("");
      form.reset();
    } catch (reason) { setAuthMessage(humanError(reason)); }
    finally { setAuthBusy(false); }
  }

  async function signOut() {
    try { await centralLogout(); }
    catch (reason) { setAuthMessage(humanError(reason)); }
    finally { setSession({ authenticated: false }); setCustomers([]); setInvoices([]); setPayments([]); setSales([]); setSolarProducts([]); }
  }

  async function retryServer() {
    try {
      const status = await centralStatus<Row>();
      const ready = status.configured === true && status.status === "CENTRAL_CONNECTED";
      setServerReachable(true);
      setServerReady(ready);
      if (!ready) setNotice({ message: "Le serveur est joignable, mais ses référentiels doivent être initialisés par un administrateur.", error: true });
      else {
        setNotice(null);
        await refresh();
      }
    } catch (error) {
      setServerReachable(false);
      setServerReady(false);
      setNotice({ message: humanError(error), error: true });
    }
  }

  useEffect(() => {
    if (!selectedCustomer) { setStatement(null); return; }
    let active = true;
    centralRequest<Row>(`/v1/customers/${encodeURIComponent(selectedCustomer)}/statement`)
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
      await centralRequest(path, "POST", payload);
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
      await centralRequest("/v1/customers", "POST", Object.fromEntries(new FormData(form)) as Row);
      setNotice({ message: "Abonné enregistré sur le serveur SEL." });
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
      const result = await centralRequest<Row>("/v1/billing/utility/preview", "POST", payload);
      setInvoiceDraft(payload);
      setPreview(result);
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function confirmInvoice() {
    if (!invoiceDraft) return;
    try {
      await centralRequest("/v1/billing/utility/issue", "POST", invoiceDraft);
      setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
      setInvoiceFormKey((key) => key + 1);
      setNotice({ message: "Facture enregistrée par le serveur SEL." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function openInvoice(invoice: Row) {
    const id = text(invoice.invoice_id ?? invoice.id, "");
    if (!id) return;
    try {
      const details = await centralRequest<Row>(`/v1/invoices/${encodeURIComponent(id)}`);
      setSelectedInvoice(details);
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function openDocument(kind: "invoice" | "payment" | "solar", id: unknown) {
    const key = text(id, "");
    if (!key) return;
    setDocumentBusy(true);
    try {
      const route = kind === "invoice"
        ? `/v1/invoices/${encodeURIComponent(key)}/document-data`
        : kind === "payment"
          ? `/v1/payments/${encodeURIComponent(key)}/document-data`
          : `/v1/solar/sales/${encodeURIComponent(key)}/document-data`;
      const data = await centralRequest<Row>(route);
      const generated = await buildClientPdf(kind, data);
      setDocumentPreview({ title: generated.title, filename: generated.filename, blob: generated.blob });
      setNotice({ message: `${generated.title} préparé sur ce poste à partir des données centrales.` });
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
    finally { setDocumentBusy(false); }
  }

  async function saveDocument() {
    if (!documentPreview) return;
    try {
      const selectedPath = await saveFileDialog({
        title: "Enregistrer le document SEL",
        defaultPath: documentPreview.filename,
        filters: [{ name: "Document PDF", extensions: ["pdf"] }],
      });
      if (!selectedPath) return;
      await writeFile(selectedPath, new Uint8Array(await documentPreview.blob.arrayBuffer()));
      setNotice({ message: `Document enregistré : ${selectedPath}` });
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function closeDocumentPreview() {
    setDocumentPreview(null);
  }

  async function updateCustomer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedCustomerData) return;
    const form = new FormData(event.currentTarget);
    const payload = {
      legal_name: String(form.get("legal_name") ?? "").trim(),
      customer_category: String(form.get("customer_category") ?? "").trim(),
      address: String(form.get("address") ?? "").trim() || null,
      phone: String(form.get("phone") ?? "").trim() || null,
    };
    try {
      const updated = await centralRequest<Row>(`/v1/customers/${encodeURIComponent(selectedCustomer)}`, "PATCH", payload, Number(selectedCustomerData.revision));
      setSelectedCustomerData(updated);
      setCustomerEditOpen(false);
      setNotice({ message: "Dossier abonné modifié sur le serveur central." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function refreshServerStatus() {
    try {
      await refresh();
      setNotice({ message: "Données actualisées depuis le serveur SEL." });
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
      const result = await centralRequest<Row>("/v1/solar/quotes", "POST", {
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
      await centralRequest("/v1/solar/sales", "POST", solarSaleDraft);
      setSolarQuote(null);
      setSolarSaleDraft(null);
      setSolarSaleOpen(false);
      setNotice({ message: "Vente enregistrée sur le serveur SEL avec sa créance. Un paiement comptant se saisit dans le journal de caisse." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function navigateTo(nextView: View) {
    setNotice(null);
    setView(nextView);
  }

  const activePage = pageTitle[view];
  const link = (id: View, label: string) => <button className="text-button" onClick={() => navigateTo(id)}>{label}</button>;
  const previewLines = rows<Row>(preview?.lines);
  const previewElectricity = previewLines.find((line) => text(line.service_code).toUpperCase() === "ELECTRICITY");
  const previewWater = previewLines.find((line) => text(line.service_code).toUpperCase() === "WATER");

  if (authChecking) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Une application connectée au service central pour gérer les abonnés, les factures et les paiements.</p></div><small>Données métier gérées par le serveur SEL</small></div><div className="auth-panel"><div className="auth-card" role="status"><p className="eyebrow">VÉRIFICATION SÉCURISÉE</p><h2>Connexion au serveur SEL</h2><p className="muted">Vérification de la session et de la disponibilité du service central…</p></div></div></section>;

  if (!session?.authenticated) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Une application connectée au service central pour gérer les abonnés, les factures et les paiements.</p></div><small>Données métier gérées par le serveur SEL</small></div><div className="auth-panel"><form className="auth-card" onSubmit={(event) => void signIn(event)}><p className="eyebrow">ACCÈS SÉCURISÉ</p><h2>Connexion à SEL Facturation</h2><p className="muted">Connectez-vous avec votre compte SEL pour accéder à l’application.</p>{authMessage && <div className="notice error" role="alert">{authMessage}</div>}<label>Identifiant SEL<input name="identity_ref" autoComplete="username" required autoFocus /></label><label>Mot de passe<input name="password" type="password" autoComplete="current-password" required /></label><Button primary type="submit" disabled={authBusy || serverReachable === false}>{authBusy ? "Connexion…" : "Se connecter"}</Button>{serverReachable === false && <Button type="button" onClick={() => void retryServer()}>Réessayer la connexion</Button>}<p className="auth-help">Une connexion Internet est requise pour ouvrir et utiliser SEL Facturation.</p></form></div></section>;

  if (serverReachable === false) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Les opérations sont traitées et conservées par le serveur central SEL.</p></div><small>Connexion au serveur requise</small></div><div className="auth-panel"><div className="auth-card"><p className="eyebrow">SERVICE CENTRAL</p><h2>Connexion au serveur interrompue</h2><p className="muted">Pour protéger l’exactitude des factures et des paiements, l’application est verrouillée jusqu’au rétablissement du service en ligne.</p>{notice?.message && <div className="notice error" role="alert">{notice.message}</div>}<div className="page-actions"><Button primary onClick={() => void retryServer()}>Réessayer</Button><Button onClick={() => void signOut()}>Se déconnecter</Button></div></div></div></section>;

  if (session.require_password_change) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Votre compte SEL,<br />sécurisé dès le départ.</h1><p>Modifiez votre mot de passe temporaire pour continuer.</p></div><small>Application connectée au serveur central</small></div><div className="auth-panel"><div className="auth-card"><p className="eyebrow">PREMIÈRE CONNEXION</p><h2>Changer le mot de passe</h2><PasswordChangeForm onComplete={() => { setSession({ authenticated: false }); setAuthMessage("Mot de passe modifié. Connectez-vous avec votre nouveau mot de passe."); }} /></div></div></section>;

  if (serverReady === false) return <section className="central-setup-screen"><header><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">CONFIGURATION CENTRALE</p><h1>Initialisation des référentiels</h1><p>Le serveur est connecté. Les opérations de facturation restent bloquées jusqu’à l’approbation des paramètres SEL.</p></div><Button onClick={() => void signOut()}>Se déconnecter</Button></header>{session.role === "ADMIN" ? <AdminConsole session={session} onSignedOut={() => void signOut()} /> : <Panel title="Configuration administrateur requise"><p>Le service central est joignable, mais aucun référentiel commercial approuvé n’est actif. Demandez à un administrateur SEL de terminer la configuration.</p><Button onClick={() => void signOut()}>Se déconnecter</Button></Panel>}</section>;

  return <div className="app-shell">
    <aside className="sidebar">
      <button className="brand" aria-label="Accueil SEL" onClick={() => navigateTo("dashboard")}><img src="/assets/sel-logo-dark.svg" alt="S.E.L. SASU" /></button>
      <nav aria-label="Navigation principale">
        {navigation.map((item) => <button key={item.id} className={`nav-item${view === item.id ? " active" : ""}`} aria-current={view === item.id ? "page" : undefined} onClick={() => navigateTo(item.id)}>
          <NavIcon name={item.icon} /><span>{item.label}</span>
        </button>)}
      </nav>
      <div className="sidebar-foot"><span className="eyebrow">SERVICE CENTRAL</span><p>Connexion requise pour toute opération</p></div>
    </aside>

    <main className="content">
      <header className="topbar"><p className="eyebrow">{activePage.context}</p>
        <div className="connection-status"><span className="status-dot" aria-hidden="true" />{serverStatus?.status === "CENTRAL_CONNECTED" ? "Serveur SEL · connecté" : "Serveur SEL · état inconnu"}<Button onClick={() => void signOut()}>Se déconnecter</Button></div>
      </header>
      {notice && <div className={`notice${notice.error ? " error" : ""}`} role={notice.error ? "alert" : "status"}>{notice.message}<button aria-label="Fermer le message" onClick={() => setNotice(null)}>×</button></div>}
      {loading && <div className="loading-line" role="status">Chargement des données centrales…</div>}

      {view === "dashboard" && <>
        <div className="page-heading"><div><p className="eyebrow">ACTIVITÉ SEL</p><h1>Vue d’ensemble</h1><p className="muted">Dossiers et opérations consultés directement sur le serveur central.</p></div><Button primary onClick={() => navigateTo("invoices")}>Nouvelle facture</Button></div>
        <div className="metrics">
          <Metric label="Abonnés" value={customers.length} note="Dossiers chargés du serveur SEL" />
          <Metric label="Factures" value={invoices.length} note="Documents enregistrés au serveur" />
          <Metric label="Encaissements" value={payments.length} note="Règlements renvoyés par le serveur" />
          <Metric label="Connexion" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Active" : "—"} note="Source unique : serveur SEL" />
        </div>
        <Panel title="Dossiers / activité récente" action={link("server", "État du serveur →")}>
          <Table headers={["Référence", "Abonné", "Opération", "Montant", "État"]} empty="Aucune activité récente retournée par le serveur." data={recentActivity.map(({ row, kind, reference, customer, amount }) => [<strong>{reference}</strong>, customer, kind, formatMoney(amount, row.currency), <Status value={row.status} />])} />
        </Panel>
      </>}

      {view === "customers" && (selectedCustomer ? <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Dossier abonné" description={`${text(selectedCustomerData?.customer_code)} · ${text(selectedCustomerData?.legal_name)}`} action={<div className="page-actions"><Button aria-label="Retour au répertoire" onClick={() => { setSelectedCustomer(""); setSelectedCustomerData(null); setStatement(null); setCustomerEditOpen(false); }}>Retour au répertoire</Button><Button primary onClick={() => setCustomerEditOpen((open) => !open)}>{customerEditOpen ? "Annuler la modification" : "Modifier la fiche"}</Button></div>} />
        <Panel title={`Fiche abonné · ${text(selectedCustomerData?.customer_code)}`}>
          {selectedCustomerData ? customerEditOpen ? <form className="form-grid" onSubmit={(event) => void updateCustomer(event)}>
            <label>Nom / raison sociale<input name="legal_name" defaultValue={text(selectedCustomerData.legal_name, "")} required /></label>
            <label>Catégorie tarifaire<select name="customer_category" defaultValue={text(selectedCustomerData.customer_category, "")} required><option value="">Sélectionner…</option><option value="RESIDENTIAL">Résidentiel</option><option value="COMMERCIAL">Commercial</option></select></label>
            <label>Révision du dossier<input value={text(selectedCustomerData.revision)} readOnly /></label>
            <label className="span-two">Adresse<input name="address" defaultValue={text(selectedCustomerData.address, "")} /></label>
            <label>Téléphone<input name="phone" defaultValue={text(selectedCustomerData.phone, "")} /></label>
            <div className="form-actions span-two"><Button type="button" onClick={() => setCustomerEditOpen(false)}>Annuler</Button><Button primary type="submit">Enregistrer les modifications</Button></div>
          </form> : <dl className="customer-details"><div><dt>Nom / raison sociale</dt><dd>{text(selectedCustomerData.legal_name)}</dd></div><div><dt>Code abonné</dt><dd>{text(selectedCustomerData.customer_code)}</dd></div><div><dt>Catégorie tarifaire</dt><dd>{text(selectedCustomerData.customer_category, "À compléter")}</dd></div><div><dt>Adresse</dt><dd>{text(selectedCustomerData.address)}</dd></div><div><dt>Téléphone</dt><dd>{text(selectedCustomerData.phone)}</dd></div><div><dt>Révision du dossier</dt><dd>{text(selectedCustomerData.revision)}</dd></div></dl> : <div className="empty-state">Chargement du dossier abonné…</div>}
        </Panel>
        <ServiceAccountsManager customerId={selectedCustomer} />
        <CustomerStatement customerName={text(selectedCustomerData?.legal_name, selectedCustomer)} customerCode={text(selectedCustomerData?.customer_code)} statement={statement} />
      </> : <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Répertoire des abonnés" description="Rechercher un dossier et consulter sa situation." action={<Button primary onClick={() => setCustomerCreateOpen((open) => !open)}>{customerCreateOpen ? "Fermer le formulaire" : "+ Ajouter un abonné"}</Button>} />
        <div className="metrics customer-metrics">
          <Metric label="Abonnés enregistrés" value={customers.length} note="Dossiers gérés par le serveur SEL" />
          <Metric label="Factures" value={invoices.length} note="Documents enregistrés au serveur" />
          <Metric label="Versements" value={payments.length} note="Règlements enregistrés au serveur" />
          <Metric label="Connexion" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Active" : "—"} note="Source unique : serveur SEL" />
        </div>
        {customerCreateOpen && <Panel title="Nouvel abonné"><form className="form-grid customer-create-form" onSubmit={(event) => void createCustomer(event)}>
            <label>Code abonné<input name="customer_code" required /></label><label>Nom / raison sociale<input name="legal_name" required /></label>
            <label>Catégorie tarifaire<select name="customer_category" required defaultValue=""><option value="">Sélectionner…</option><option value="RESIDENTIAL">Résidentiel</option><option value="COMMERCIAL">Commercial</option></select></label>
            <label>Adresse<input name="address" /></label><label>Téléphone<input name="phone" /></label>
            <p className="help span-two">Les informations pourront être complétées dans le dossier de l’abonné.</p>
            <div className="form-actions span-two"><Button primary type="submit">Enregistrer l’abonné</Button></div>
          </form></Panel>}
        <Panel title="Abonnés enregistrés" action={<input className="table-search" aria-label="Rechercher un abonné" placeholder="Code, nom ou téléphone" value={customerFilter} onChange={(event) => setCustomerFilter(event.target.value)} />}><Table headers={["Code", "Nom / raison sociale", "Adresse", "Action"]} empty="Aucun abonné retourné par le serveur." data={filteredCustomers.map((row) => [<strong>{text(row.customer_code)}</strong>, text(row.legal_name), text(row.address), <button className="text-button" onClick={() => { setSelectedCustomer(text(row.customer_id ?? row.id, "")); setSelectedCustomerData(row); setCustomerEditOpen(false); }}>Ouvrir le dossier →</button>])} /></Panel>
      </>)}

      {view === "readings" && <MeterReadingsView customers={customers} onOpenInvoices={() => navigateTo("invoices")} onOpenCustomer={(customerId) => {
        const customer = customers.find((item) => text(item.customer_id ?? item.id) === customerId);
        if (!customer) return;
        setSelectedCustomer(customerId);
        setSelectedCustomerData(customer);
        setCustomerEditOpen(false);
        navigateTo("customers");
      }} />}

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
            <Table headers={["Référence", "Abonné", "Période", "État", "Actions"]} empty="Aucune facture retournée par le serveur." data={filteredInvoices.map((row) => [<strong>{text(row.invoice_number ?? row.invoice_id)}</strong>, text(row.customer_name ?? row.customer_id), text(row.period ?? row.billing_period), <Status value={row.status} />, <div className="page-actions"><button className="text-button" onClick={() => void openInvoice(row)}>Détail</button><Button disabled={documentBusy} onClick={() => void openDocument("invoice", row.invoice_id ?? row.id)}>Document PDF</Button></div>])} />
          </Panel>
          {selectedInvoice && <>
            <p className="muted">Période {text(selectedInvoice.billing_period)} · Échéance {text(selectedInvoice.due_date)} · <Status value={selectedInvoice.status} /></p>
            <Preview data={selectedInvoice} title={`Détail de la facture · ${text(selectedInvoice.invoice_number ?? selectedInvoice.invoice_id)}`} action={<Button onClick={() => setSelectedInvoice(null)}>Fermer</Button>} />
          </>}
        </div>
        {preview && <>
          <PageHeading eyebrow="CORE / FACTURATION" title="Prévisualisation de facture" description="Vérifier les lignes, les taxes et les références avant d’émettre." action={<div className="page-actions"><Button onClick={() => { setPreview(null); setInvoiceDraft(null); }}>Modifier la saisie</Button><Button onClick={() => { setPreview(null); setInvoiceDraft(null); }}>Retour à l’historique des factures</Button><Button primary onClick={() => void confirmInvoice()}>Émettre la facture</Button></div>} />
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
        <Panel title="Encaissements enregistrés"><Table className="payment-table" headers={["Référence", "Date et heure", "Montant", "Mode", "État", "Document"]} empty="Aucun encaissement retourné par le serveur." data={payments.map((row) => [<strong title={text(row.payment_number ?? row.payment_id)}>{text(row.payment_number ?? row.payment_id)}</strong>, formatDateTime(row.paid_at ?? row.occurred_at), formatMoney(row.amount_received ?? row.received_amount, row.currency), domainLabel(row.method), <Status value={row.status} />, <Button disabled={documentBusy} onClick={() => void openDocument("payment", row.payment_id ?? row.id)}>Reçu PDF</Button>])} /></Panel>
      </>}

      {view === "solar" && <>
        <PageHeading eyebrow="COMMERCE / FINANCE" title="Catalogue des kits solaires" description="Consultez les produits approuvés et les ventes enregistrées par le serveur." action={<Button primary onClick={() => setSolarSaleOpen(true)}>Créer une vente</Button>} />
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
          <Metric label="Produits disponibles" value={solarProducts.length} note="Catalogue central approuvé" />
          <Metric label="Ventes enregistrées" value={sales.length} note="Historique chargé du serveur" />
          <Metric label="Contrats à échéances" value={sales.filter((sale) => Boolean(sale.installment_plan_id)).length} note="Échéanciers associés" />
          <Metric label="Connexion" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Active" : "—"} note="Opérations centralisées" />
        </div>
        <Panel title="Catalogue approuvé">
          <Table headers={["SKU", "Produit", "Catégorie", "Prix de référence", "État"]} empty="Aucun produit solaire approuvé et applicable n’est disponible sur le serveur." data={solarProducts.map((product) => [<strong>{text(product.sku)}</strong>, text(product.label), product.category ? domainLabel(product.category) : "—", formatMoney(product.price_reference, product.currency), <span className="status-badge success">Disponible</span>])} />
        </Panel>
        <Panel title="Historique des ventes"><Table headers={["Référence", "Abonné", "Produit", "Principal", "État", "Document"]} empty="Aucune vente solaire retournée par le serveur." data={sales.map((row) => [<strong>{text(row.sale_id)}</strong>, customerLabel(row.customer_id, customers), text(row.product_code), formatMoney(row.principal, row.currency), <Status value={row.status} />, <Button disabled={documentBusy} onClick={() => void openDocument("solar", row.sale_id)}>Confirmation PDF</Button>])} /></Panel>
        <SolarContractManager sales={sales} customers={customers} onCreated={refresh} />
      </>}

      {view === "receivables" && <>
        <PageHeading eyebrow="FINANCE / SOLDES" title="Suivi des créances" description="Soldes et obligations renvoyés par le serveur, sans recalcul dans l’interface." />
        <Panel title="Situation d’un abonné"><label className="customer-picker">Abonné<select value={selectedCustomer} onChange={(event) => setSelectedCustomer(event.target.value)}><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          {statement ? <Table headers={["Référence", "Nature", "Solde", "Situation"]} empty="Aucune créance associée." data={rows<Row>(statement.receivables).map((row) => [<strong>{text(row.receivable_id ?? row.id)}</strong>, domainLabel(row.source_type), formatMoney(row.balance, row.currency), <Status value={row.status} />])} /> : <div className="empty-state">Choisissez un abonné pour consulter son relevé et ses créances.</div>}
        </Panel>
      </>}

      {view === "server" && <>
        <PageHeading eyebrow="SEL / SERVICE CENTRAL" title="Connexion au serveur" description="Les abonnés, factures et paiements sont consultés et enregistrés directement sur le serveur SEL." action={<Button primary onClick={() => void refreshServerStatus()}>Actualiser l’état</Button>} />
        <Panel title="État de la connexion"><div className="summary-state"><Status value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Connecté au serveur SEL" : serverStatus?.status === "CENTRAL_UNCONFIGURED" ? "Configuration serveur incomplète" : "Serveur indisponible"} /><p>{serverStatus?.status === "CENTRAL_CONNECTED" ? "Les opérations sont envoyées directement au serveur central." : serverStatus?.status === "CENTRAL_UNCONFIGURED" ? "Le service répond, mais la configuration de production doit être terminée avant toute opération." : "Aucune opération métier n’est possible sans connexion au serveur SEL."}</p></div></Panel>
      </>}

      {view === "admin" && <>
        <PageHeading eyebrow="ADMIN / RÉFÉRENTIELS" title="Administration" description="Les écrans administratifs suivent les permissions et les paramètres effectivement exposés par le service Rust." />
        <AdminConsole session={session} onSignedOut={() => void signOut()} />
      </>}

      {documentPreview && <div className="document-preview-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) closeDocumentPreview(); }}>
        <section className="document-preview-dialog" role="dialog" aria-modal="true" aria-label={documentPreview.title}>
          <header className="document-preview-toolbar"><div><p className="eyebrow">DOCUMENT · A4</p><h2>{documentPreview.title}</h2><p className="muted">Aperçu préparé à partir des données renvoyées par le serveur SEL.</p></div><div className="page-actions"><Button onClick={() => window.print()}>Imprimer</Button><Button primary onClick={() => void saveDocument()}>Enregistrer le PDF</Button><Button aria-label="Fermer l’aperçu" onClick={closeDocumentPreview}>Fermer</Button></div></header>
      <ClientPdfPreview blob={documentPreview.blob} />
        </section>
      </div>}
      <footer>Application développée et sécurisée par LTEX · <button className="footer-link" onClick={() => void openMonarqueSite().catch((error: unknown) => setNotice({ message: humanError(error), error: true }))}>MonarQue Group</button></footer>
    </main>
  </div>;
}

function ClientPdfPreview({ blob }: { blob: Blob }) {
  const container = useRef<HTMLDivElement>(null);
  const [pageCount, setPageCount] = useState(0);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    let dispose = () => {};
    const controller = new AbortController();
    setPageCount(0);
    setError("");
    const element = container.current;
    if (!element) return;
    void renderClientPdfPreview(blob, element, setPageCount, controller.signal)
      .then((cleanup) => { if (active) dispose = cleanup; else cleanup(); })
      .catch((reason: unknown) => { if (active) setError(humanError(reason)); });
    return () => { active = false; controller.abort(); dispose(); };
  }, [blob]);

  return <div className="document-pdf-stage">
    <p className="document-pdf-status" role="status">{error || (pageCount ? `${pageCount} page${pageCount > 1 ? "s" : ""} · aperçu préparé sur ce poste` : "Préparation de l’aperçu PDF…")}</p>
    <div ref={container} className="document-pdf-pages" aria-label="Pages du document PDF" />
  </div>;
}

function PageHeading({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: ReactNode }) {
  return <div className="page-heading"><div><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p className="muted">{description}</p></div>{action}</div>;
}

function PasswordChangeForm({ onComplete }: { onComplete: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    const currentPassword = String(values.get("current_password") ?? "");
    const password = String(values.get("new_password") ?? "");
    if (password !== String(values.get("repeat_password") ?? "")) { setError("Les deux nouveaux mots de passe ne correspondent pas."); return; }
    setBusy(true); setError("");
    try {
      await centralAdminRequest("/v1/auth/change-password", "POST", { current_password: currentPassword, new_password: password });
      onComplete();
    } catch (reason) { setError(humanError(reason)); }
    finally { setBusy(false); }
  }
  return <form className="auth-change-form" onSubmit={(event) => void submit(event)}>
    {error && <div className="notice error" role="alert">{error}</div>}
    <label>Mot de passe temporaire<input type="password" name="current_password" autoComplete="current-password" minLength={8} required /></label>
    <label>Nouveau mot de passe<input type="password" name="new_password" autoComplete="new-password" minLength={8} required /></label>
    <label>Confirmer le nouveau mot de passe<input type="password" name="repeat_password" autoComplete="new-password" minLength={8} required /></label>
    <Button primary type="submit" disabled={busy}>{busy ? "Enregistrement…" : "Changer le mot de passe"}</Button>
  </form>;
}

function Metric({ label, value, note }: { label: string; value: ReactNode; note: string }) {
  return <div className="metric"><div className="metric-label">{label}</div><div className="metric-value">{value}</div><div className="metric-note">{note}</div></div>;
}

function Status({ value }: { value: unknown }) {
  const normalized = text(value, "État inconnu");
  const labels: Record<string, string> = {
    CENTRAL_ACCEPTED: "Confirmé par le serveur",
    UPCOMING: "À venir",
    OVERDUE: "En retard",
    PAID: "Payé",
    PARTIAL: "Partiellement payé",
    OPEN: "À régler",
    SETTLED: "Réglé",
    CREDITED: "Avoir enregistré",
    CONFLICT: "Conflit à examiner",
    REJECTED: "Refusé",
    ACTIVE: "Actif",
    DISABLED: "Désactivé",
    APPROVED: "Approuvé",
    DRAFT: "Brouillon",
    REVOKED: "Révoqué",
    ISSUED: "Émise",
  };
  const state = normalized.toLocaleLowerCase("en").includes("pending") || normalized.toLocaleLowerCase("fr").includes("attente") ? "pending"
    : normalized.toLocaleLowerCase("en").includes("conflict") || normalized.toLocaleLowerCase("fr").includes("conflit") || normalized.toLocaleLowerCase("en").includes("reject") ? "danger"
      : ["ACTIVE", "APPROVED", "ISSUED"].includes(normalized.toUpperCase()) || normalized.toLocaleLowerCase("en").includes("accept") || normalized.toLocaleLowerCase("en").includes("ready") ? "success"
        : ["DISABLED", "REVOKED"].includes(normalized.toUpperCase()) ? "danger" : "neutral";
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
  const [allocationRows, setAllocationRows] = useState([{ key: "allocation-0", receivableId: "", amount: "", scheduleId: "" }]);
  const [receivables, setReceivables] = useState<Row[]>([]);
  const [receivablesLoading, setReceivablesLoading] = useState(false);
  const [receivablesError, setReceivablesError] = useState("");
  const [planSchedules, setPlanSchedules] = useState<Record<string, { schedule: Row[]; currency: string; loading: boolean; error: string }>>({});
  const [receivedAmount, setReceivedAmount] = useState("");
  const [occurredAt, setOccurredAt] = useState(dateTimeInputValue);

  useEffect(() => {
    if (!customerId) { setReceivables([]); setReceivablesLoading(false); setReceivablesError(""); return; }
    let active = true;
    setReceivables([]); setReceivablesLoading(true); setReceivablesError("");
    centralRequest<Row>(`/v1/customers/${encodeURIComponent(customerId)}/statement`).then((result) => {
      if (active) setReceivables(rows<Row>(result.receivables).filter((item) => Number(item.balance) > 0));
    }).catch((error: unknown) => { if (active) { setReceivables([]); setReceivablesError(humanError(error)); } })
      .finally(() => { if (active) setReceivablesLoading(false); });
    return () => { active = false; };
  }, [customerId]);

  const installmentPlanByReceivable = new Map(sales
    .filter((sale) => sale.installment_plan_id)
    .map((sale) => [text(sale.receivable_id), text(sale.installment_plan_id)]));
  const schedulePlanIds = [...new Set(allocationRows
    .map((allocation) => installmentPlanByReceivable.get(allocation.receivableId) ?? "")
    .filter(Boolean))].sort();
  const schedulePlanKey = schedulePlanIds.join("|");
  useEffect(() => {
    if (!schedulePlanKey) { setPlanSchedules({}); return; }
    let active = true;
    const planIds = schedulePlanKey.split("|");
    setPlanSchedules((current) => {
      const next: typeof current = {};
      for (const planId of planIds) next[planId] = { schedule: current[planId]?.schedule ?? [], currency: current[planId]?.currency ?? "", loading: true, error: "" };
      return next;
    });
    void Promise.all(planIds.map(async (planId) => {
      try {
        const result = await centralRequest<Row>(`/v1/installment-plans/${encodeURIComponent(planId)}`);
        return [planId, { schedule: rows<Row>(result.schedule), currency: text(result.currency), loading: false, error: "" }] as const;
      } catch (error) {
        return [planId, { schedule: [], currency: "", loading: false, error: humanError(error) }] as const;
      }
    })).then((entries) => {
      if (active) setPlanSchedules((current) => ({ ...current, ...Object.fromEntries(entries) }));
    });
    return () => { active = false; };
  }, [schedulePlanKey]);

  const selectedReceivables = allocationRows.map((allocation) => receivables.find((item) => text(item.receivable_id ?? item.id) === allocation.receivableId));
  const currency = text(selectedReceivables.find(Boolean)?.currency, "USD");
  const duplicateAllocation = allocationRows.some((allocation, index) => allocation.receivableId && allocationRows.some((other, otherIndex) => otherIndex !== index && other.receivableId === allocation.receivableId));
  const currencyMismatch = selectedReceivables.some((item) => item && text(item.currency, "USD") !== currency);
  const allocationsReady = allocationRows.length > 0 && allocationRows.every((allocation) => {
    if (!allocation.receivableId || !allocation.amount) return false;
    const planId = installmentPlanByReceivable.get(allocation.receivableId);
    if (!planId) return true;
    const state = planSchedules[planId];
    const openSchedule = state?.schedule.filter((item) => text(item.status).toUpperCase() !== "PAID") ?? [];
    return Boolean(state && !state.loading && !state.error && openSchedule.length && allocation.scheduleId);
  });
  const paymentBlocked = receivablesLoading || Boolean(receivablesError) || !allocationsReady || duplicateAllocation || currencyMismatch;
  const primaryReceivable = selectedReceivables.find(Boolean);

  function updateAllocation(key: string, field: "receivableId" | "amount" | "scheduleId", value: string) {
    setAllocationRows((current) => current.map((allocation) => allocation.key === key
      ? { ...allocation, [field]: value, ...(field === "receivableId" ? { scheduleId: "" } : {}) }
      : allocation));
  }

  return <div className="payment-workspace"><Panel title="Informations du règlement"><form id="central-payment-form" className="form-grid" onSubmit={async (event) => {
    const form = new FormData(event.currentTarget);
    const saved = await onSubmit(event, "/v1/payments", {
      customer_id: form.get("customer_id"), received_amount: form.get("received_amount"), currency, method: form.get("method"), occurred_at: new Date(String(form.get("occurred_at"))).toISOString(),
      allocations: allocationRows.map((allocation) => ({ receivable_id: allocation.receivableId, amount: allocation.amount, schedule_id: allocation.scheduleId || null })),
    }, "Règlement enregistré par le serveur SEL.");
    if (saved) { setCustomerId(""); setAllocationRows([{ key: `allocation-${Date.now()}`, receivableId: "", amount: "", scheduleId: "" }]); setReceivedAmount(""); setOccurredAt(dateTimeInputValue()); }
  }}>
    <label className="span-two">Abonné<select name="customer_id" value={customerId} onChange={(event) => { setCustomerId(event.target.value); setAllocationRows([{ key: `allocation-${Date.now()}`, receivableId: "", amount: "", scheduleId: "" }]); }} required><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
    <label>Montant reçu<input name="received_amount" type="number" min="0.01" step="0.01" value={receivedAmount} onChange={(event) => setReceivedAmount(event.target.value)} required /></label>
    <label>Mode<select name="method"><option value="CASH">Espèces</option><option value="BANK">Banque</option><option value="MOBILE_MONEY">Mobile Money</option><option value="OTHER">Autre</option></select></label>
    <label>Date du règlement<input name="occurred_at" type="datetime-local" value={occurredAt} onChange={(event) => setOccurredAt(event.target.value)} required /></label>
    <label>Devise<input value={currency} readOnly required /></label>
    <div className="allocation-list span-two"><div className="allocation-list-heading"><strong>Répartition sur les créances</strong><Button type="button" disabled={allocationRows.length >= 10 || receivables.length <= allocationRows.length} onClick={() => setAllocationRows((current) => [...current, { key: `allocation-${Date.now()}-${current.length}`, receivableId: "", amount: "", scheduleId: "" }])}>+ Ajouter une créance</Button></div>
      {allocationRows.map((allocation, index) => {
        const planId = installmentPlanByReceivable.get(allocation.receivableId);
        const planState = planId ? planSchedules[planId] : undefined;
        const openSchedule = planState?.schedule.filter((item) => text(item.status).toUpperCase() !== "PAID") ?? [];
        return <fieldset className="allocation-card" key={allocation.key}><legend>Affectation {index + 1}</legend>
          <label>Créance<select value={allocation.receivableId} onChange={(event) => updateAllocation(allocation.key, "receivableId", event.target.value)} required disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length}><option value="">{!customerId ? "Sélectionnez d’abord un abonné" : receivablesLoading ? "Chargement des créances…" : "Sélectionner une créance"}</option>{receivables.map((item) => { const id = text(item.receivable_id ?? item.id); const usedElsewhere = allocationRows.some((other) => other.key !== allocation.key && other.receivableId === id); const wrongCurrency = text(item.currency, "USD") !== currency; return <option key={id} value={id} disabled={usedElsewhere || wrongCurrency}>{domainLabel(item.source_type ?? item.status)} · {formatMoney(item.balance, item.currency)}{usedElsewhere ? " · déjà sélectionnée" : wrongCurrency ? " · devise différente" : ""}</option>; })}</select></label>
          <label>Montant affecté<input type="number" min="0.01" step="0.01" value={allocation.amount} onChange={(event) => updateAllocation(allocation.key, "amount", event.target.value)} required /></label>
          {planId && <label className="span-two">Échéance solaire<select value={allocation.scheduleId} onChange={(event) => updateAllocation(allocation.key, "scheduleId", event.target.value)} required disabled={!planState || planState.loading || Boolean(planState.error)}><option value="">{planState?.loading ? "Chargement de l’échéancier…" : "Sélectionner une échéance…"}</option>{openSchedule.map((item) => <option key={text(item.id)} value={text(item.id)}>N° {text(item.sequence)} · {text(item.due_date)} · restant {formatMoney(item.remaining, planState?.currency)}</option>)}</select></label>}
          {planState?.error && <p className="help error-text span-two" role="alert">Impossible de vérifier l’échéancier : {planState.error} Le règlement est bloqué.</p>}
          {!planState?.loading && planId && !planState?.error && !openSchedule.length && <p className="help span-two" role="status">Aucune échéance ouverte n’est disponible pour cette créance. Le règlement est bloqué.</p>}
          {allocationRows.length > 1 && <div className="allocation-remove span-two"><Button type="button" onClick={() => setAllocationRows((current) => current.filter((item) => item.key !== allocation.key))}>Retirer cette créance</Button></div>}
        </fieldset>;
      })}
      {receivablesError && <p className="help error-text" role="alert">Impossible de charger les créances : {receivablesError}</p>}
      {duplicateAllocation && <p className="help error-text" role="alert">Choisissez une seule ligne par créance ; le moteur Rust vérifiera chaque solde et le total affecté.</p>}
      {currencyMismatch && <p className="help error-text" role="alert">Toutes les créances d’un même règlement doivent utiliser la même devise.</p>}
      <p className="help">Les montants, échéances et soldes sont vérifiés atomiquement par le moteur Rust. Chaque ligne reste affectée à une créance distincte.</p>
    </div>
    <label>Référence interne<input value="Générée par Rust à l’enregistrement" readOnly /></label>
  </form></Panel><Panel title="Récapitulatif"><div className="payment-summary"><p className="eyebrow">PRÉPARATION DU RÈGLEMENT</p><h2>Règlement</h2><p className="muted">{primaryReceivable ? `${domainLabel(primaryReceivable.source_type ?? primaryReceivable.status)} · soldes retournés par Rust` : "Choisissez un abonné puis les créances à affecter."}</p><div className="payment-summary-values"><div><span>Abonné</span><strong>{customerId ? customerLabel(customerId, customers) : "—"}</strong></div><div><span>Créances sélectionnées</span><strong>{allocationRows.filter((allocation) => allocation.receivableId).length}</strong></div><div><span>Devise</span><strong>{currency}</strong></div></div><p className="eyebrow">MONTANT À CONFIRMER</p><strong className="payment-amount">{receivedAmount ? formatMoney(receivedAmount, currency) : "—"}</strong><span className={`status-badge${primaryReceivable ? " pending" : ""}`}>{primaryReceivable ? "En attente d’enregistrement" : "Aucune créance sélectionnée"}</span><p className="help">Rust contrôle le montant reçu, l’ensemble des affectations, les devises et chaque solde avant d’enregistrer le règlement.</p><Button primary type="submit" form="central-payment-form" disabled={paymentBlocked}>Enregistrer le règlement</Button></div></Panel></div>;
}

function ServiceAccountsManager({ customerId }: { customerId: string }) {
  const [accounts, setAccounts] = useState<Row[]>([]);
  const [metersByAccount, setMetersByAccount] = useState<Record<string, Row[]>>({});
  const [accountFormOpen, setAccountFormOpen] = useState(false);
  const [meterFormAccount, setMeterFormAccount] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [messageError, setMessageError] = useState(false);

  const loadAccounts = useCallback(async () => {
    setLoading(true);
    try {
      const result = rows(await centralRequest<unknown>(`/v1/customers/${encodeURIComponent(customerId)}/service-accounts`));
      const meterEntries = await Promise.all(result.map(async (account) => {
        const accountId = text(account.id);
        const meters = rows(await centralRequest<unknown>(`/v1/service-accounts/${encodeURIComponent(accountId)}/meters`));
        return [accountId, meters] as const;
      }));
      setAccounts(result);
      setMetersByAccount(Object.fromEntries(meterEntries));
    } catch (error) {
      setMessage(humanError(error));
      setMessageError(true);
    } finally { setLoading(false); }
  }, [customerId]);

  useEffect(() => { void loadAccounts(); }, [loadAccounts]);

  async function createServiceAccount(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const serviceCode = String(form.get("service_code") ?? "");
    const payload = {
      service_code: serviceCode,
      connection_status: String(form.get("connection_status") ?? "UNKNOWN"),
      contract_ref: String(form.get("contract_ref") ?? "").trim() || null,
      activated_at: String(form.get("activated_at") ?? "") || null,
    };
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await centralRequest(`/v1/customers/${encodeURIComponent(customerId)}/service-accounts`, "POST", payload);
      setAccountFormOpen(false);
      setMessage(`${domainLabel(serviceCode)} rattaché à l’abonné. Aucun compteur n’a été créé.`);
      await loadAccounts();
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function createMeter(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const account = accounts.find((item) => text(item.id) === meterFormAccount);
    if (!account) return;
    const form = new FormData(event.currentTarget);
    const unit = text(account.service_code).toUpperCase() === "WATER" ? "M3" : "KWH";
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await centralRequest(`/v1/service-accounts/${encodeURIComponent(meterFormAccount)}/meters`, "POST", {
        serial: String(form.get("serial") ?? "").trim(), unit,
      });
      setMeterFormAccount("");
      setMessage("Compteur enregistré. Les relevés pourront être saisis pour ce service.");
      await loadAccounts();
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  const availableServices = ["ELECTRICITY", "WATER"].filter((code) => !accounts.some((account) => text(account.service_code).toUpperCase() === code));

  return <Panel title="Services et compteurs" action={<Button primary onClick={() => setAccountFormOpen((open) => !open)} disabled={!availableServices.length || saving}>{accountFormOpen ? "Fermer" : "Rattacher un service"}</Button>}>
    <p className="help service-account-intro">Rattacher Eau ou Électricité ne nécessite pas de compteur. Ajoutez un compteur uniquement lorsqu’il existe réellement; sans compteur, la consommation reste saisissable directement sur une facture.</p>
    {loading ? <div className="empty-state">Chargement des services depuis le serveur…</div> : accounts.length ? <div className="service-account-list">{accounts.map((account) => {
      const accountId = text(account.id);
      const accountMeters = metersByAccount[accountId] ?? [];
      const unit = text(account.service_code).toUpperCase() === "WATER" ? "M3" : "KWH";
      const contractRef = text(account.contract_ref, "").trim();
      return <article className="service-account-card" key={accountId}>
        <div className="service-account-head"><div><strong>{domainLabel(account.service_code)}</strong><span className="muted">{domainLabel(account.connection_status)}</span></div><Button onClick={() => setMeterFormAccount((current) => current === accountId ? "" : accountId)} disabled={saving}>{meterFormAccount === accountId ? "Annuler" : "Ajouter un compteur"}</Button></div>
        {contractRef && <p className="help">Contrat : {contractRef}</p>}
        {accountMeters.length ? <ul className="service-meter-list">{accountMeters.map((meter) => <li key={text(meter.id)}><strong>{text(meter.serial)}</strong><span>{formatUnit(meter.unit)}{meter.active === false ? " · Inactif" : " · Actif"}</span></li>)}</ul> : <p className="help">Aucun compteur enregistré. Le service reste utilisable pour la facturation manuelle.</p>}
        {meterFormAccount === accountId && <form className="form-grid service-meter-form" onSubmit={(event) => void createMeter(event)}>
          <label>Numéro réel du compteur<input name="serial" maxLength={128} required placeholder="À saisir uniquement si le compteur existe" /></label>
          <label>Unité<input value={unit === "M3" ? "m³" : "kWh"} readOnly /></label>
          <p className="help span-two">Ce champ est obligatoire uniquement pour enregistrer un compteur réel. Aucun numéro fictif ne sera créé.</p>
          <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer le compteur"}</Button></div>
        </form>}
      </article>;
    })}</div> : <div className="empty-state">Aucun service n’est encore rattaché. Vous pouvez ajouter l’eau et l’électricité sans créer de compteur.</div>}
    {accountFormOpen && availableServices.length > 0 && <form className="form-grid service-account-form" onSubmit={(event) => void createServiceAccount(event)}>
      <label>Service<select name="service_code" required defaultValue=""><option value="" disabled>Sélectionner…</option>{availableServices.map((code) => <option key={code} value={code}>{domainLabel(code)}</option>)}</select></label>
      <label>État du raccordement<select name="connection_status" defaultValue="UNKNOWN"><option value="UNKNOWN">Inconnu</option><option value="CONNECTED">Raccordé</option><option value="NOT_CONNECTED">Non raccordé</option></select></label>
      <label>Référence du contrat <span className="muted">(facultatif)</span><input name="contract_ref" maxLength={128} /></label>
      <label>Date d’activation <span className="muted">(facultative)</span><input name="activated_at" type="date" /></label>
      <p className="help span-two">Aucun numéro de compteur n’est demandé à cette étape. L’état « Inconnu » évite d’affirmer un raccordement non vérifié.</p>
      <div className="form-actions span-two"><Button type="button" onClick={() => setAccountFormOpen(false)}>Annuler</Button><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Rattacher le service"}</Button></div>
    </form>}
    {accountFormOpen && !availableServices.length && <p className="help">L’eau et l’électricité sont déjà rattachées à cet abonné.</p>}
    {message && <p role={messageError ? "alert" : "status"} className={`reading-message${messageError ? " error" : ""}`}>{message}</p>}
  </Panel>;
}

function MeterReadingsView({ customers, onOpenInvoices, onOpenCustomer }: { customers: Row[]; onOpenInvoices: () => void; onOpenCustomer: (customerId: string) => void }) {
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
    centralRequest<unknown>(`/v1/customers/${encodeURIComponent(customerId)}/service-accounts`).then((result) => {
      if (active) { setAccounts(rows(result)); setAccountId(""); setMeters([]); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); }
    }).catch((error: unknown) => { if (active) { setMessage(humanError(error)); setMessageError(true); } });
    return () => { active = false; };
  }, [customerId]);

  useEffect(() => {
    if (!accountId) { setMeters([]); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); return; }
    let active = true;
    centralRequest<unknown>(`/v1/service-accounts/${encodeURIComponent(accountId)}/meters`).then((result) => {
      if (active) { setMeters(rows(result).filter((meter) => meter.active !== false)); setMeterId(""); setReadings([]); setReadingPeriod(""); setPreviousIndex(""); }
    }).catch((error: unknown) => { if (active) { setMessage(humanError(error)); setMessageError(true); } });
    return () => { active = false; };
  }, [accountId]);

  useEffect(() => {
    if (!meterId) { setReadings([]); setReadingPeriod(""); setPreviousIndex(""); return; }
    let active = true;
    centralRequest<unknown>(`/v1/meters/${encodeURIComponent(meterId)}/readings`).then((result) => {
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
      const meterBase = `/v1/meters/${encodeURIComponent(meterId)}/readings`;
      const result = await centralRequest<Row>(`${meterBase}/preview`, "POST", input);
      setPreview(result);
      setPreviewInput(input);
    } catch (error) { setPreview(null); setPreviewInput(null); setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function saveReading() {
    if (!meterId || !previewInput) return;
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await centralRequest(`/v1/meters/${encodeURIComponent(meterId)}/readings`, "POST", previewInput);
      const result = await centralRequest<unknown>(`/v1/meters/${encodeURIComponent(meterId)}/readings`);
      const nextReadings = rows(result);
      setReadings(nextReadings);
      setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : "");
      setReadingPeriod("");
      setPreview(null); setPreviewInput(null);
      setMessage("Relevé enregistré par le serveur SEL."); setMessageError(false);
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function correctReading(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!correctingReading) return;
    const form = new FormData(event.currentTarget);
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await centralRequest(`/v1/meter-readings/${encodeURIComponent(text(correctingReading.id))}/corrections`, "POST", {
        current: form.get("current"), source: "MANUAL", reason: form.get("reason"),
      });
      const result = await centralRequest<unknown>(`/v1/meters/${encodeURIComponent(meterId)}/readings`);
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
          <label>Source du relevé<input value="Saisie de l’agent" readOnly /></label>
          <p className="help span-two">La consommation est calculée et validée par le moteur Rust avant l’enregistrement.</p>
          <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Vérification…" : "Prévisualiser les index"}</Button></div>
        </form>}
        {message && <p role={messageError ? "alert" : "status"} className={`reading-message${messageError ? " error" : ""}`}>{message}</p>}
        {!customerId && <div className="empty-state">Choisissez un abonné pour afficher ses services et ses compteurs.</div>}
        {customerId && !accounts.length && <div className="empty-state service-empty-state"><p>Aucun service n’est encore rattaché à cet abonné. Vous pouvez le faire depuis son dossier, sans numéro de compteur.</p><Button onClick={() => onOpenCustomer(customerId)}>Ouvrir le dossier abonné</Button></div>}
        {accountId && !meters.length && <div className="empty-state service-empty-state"><p>Aucun compteur actif n’est associé à ce service. La facture peut tout de même être créée avec une consommation saisie manuellement.</p><Button primary onClick={onOpenInvoices}>Saisir une facture manuellement</Button></div>}
      </Panel>
      <Panel title="Récapitulatif">
        <div className="reading-summary">
          <p className="eyebrow">PRÉPARATION DU RELEVÉ</p>
          <h2>{preview ? "Index vérifiés" : "À vérifier"}</h2>
          <p className="muted">{preview ? `${formatUnit(selectedMeter?.unit)} · ${text(preview.period)}` : "Le moteur calcule la différence entre les index avant l’enregistrement."}</p>
          <dl><div><dt>Compteur</dt><dd>{selectedMeter ? text(selectedMeter.serial) : "—"}</dd></div><div><dt>Index précédent</dt><dd>{preview ? formatQuantity(preview.previous, selectedMeter?.unit) : "—"}</dd></div><div><dt>Index actuel</dt><dd>{preview ? formatQuantity(preview.current, selectedMeter?.unit) : "—"}</dd></div></dl>
          <p className="eyebrow">CONSOMMATION MESURÉE</p>
          <strong className="reading-quantity">{preview ? formatQuantity(preview.measured_quantity, selectedMeter?.unit) : "—"}</strong>
          <span className={`status-badge${preview ? " pending" : ""}`}>{preview ? "Prévisualisation · non enregistrée" : "En attente de vérification"}</span>
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
    try { setPlan(await centralRequest<Row>(`/v1/installment-plans/${encodeURIComponent(planId)}`)); }
    catch (error) { setMessage(humanError(error)); }
  }

  async function saveContract(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const sale = availableSales.find((item) => text(item.sale_id) === saleId);
    const form = new FormData(event.currentTarget);
    if (!sale || !sale.receivable_id) return;
    setSaving(true); setMessage("");
    try {
      const result = await centralRequest<Row>("/v1/solar/contracts", "POST", {
        sale_id: sale.sale_id,
        receivable_id: sale.receivable_id,
        schedule: schedule.map((entry, index) => ({ sequence: index + 1, due_date: entry.due_date, amount: entry.amount })),
        currency: sale.currency,
        approved_terms_revision: form.get("approved_terms_revision"),
      });
      const planId = text(result.installment_plan_id, "");
      if (planId) setPlan(await centralRequest<Row>(`/v1/installment-plans/${encodeURIComponent(planId)}`));
      setMessage("Contrat et échéancier enregistrés par le serveur SEL.");
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

type AdminCatalog = "tariffs" | "products" | "tax-profiles" | "fx-rates";
type AdminCatalogField = { name: string; label: string; type?: string; required?: boolean; step?: string };

const adminCatalogDefinitions: Record<AdminCatalog, { title: string; fields: AdminCatalogField[]; columns: string[] }> = {
  tariffs: { title: "Tarifs", fields: [
    { name: "service_code", label: "Service" }, { name: "customer_category", label: "Catégorie" },
    { name: "unit", label: "Unité" }, { name: "unit_price", label: "Prix unitaire", type: "number", step: "any" },
    { name: "currency", label: "Devise" }, { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["service_code", "customer_category", "unit_price", "currency", "effective_from", "status"] },
  products: { title: "Catalogue solaire", fields: [
    { name: "sku", label: "Référence produit" }, { name: "label", label: "Désignation" },
    { name: "price_reference", label: "Prix de référence", type: "number", step: "any" },
    { name: "currency", label: "Devise" }, { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["sku", "label", "price_reference", "currency", "effective_from", "status"] },
  "tax-profiles": { title: "Profils fiscaux", fields: [
    { name: "service_code", label: "Service" }, { name: "jurisdiction", label: "Juridiction" },
    { name: "base", label: "Assiette" }, { name: "rate", label: "Taux", type: "number", step: "any" },
    { name: "source", label: "Référence approuvée par SEL" }, { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["service_code", "jurisdiction", "base", "rate", "effective_from", "status"] },
  "fx-rates": { title: "Taux de change", fields: [
    { name: "from_currency", label: "Devise source" }, { name: "to_currency", label: "Devise cible" },
    { name: "value", label: "Taux", type: "number", step: "any" }, { name: "source", label: "Référence approuvée par SEL" },
    { name: "effective_from", label: "Début d’effet", type: "date" }, { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["from_currency", "to_currency", "value", "effective_from", "status"] },
};

function AdminCatalogPanel({ kind, items, busy, onCreate, onApprove, onRevoke }: {
  kind: AdminCatalog; items: Row[]; busy: boolean;
  onCreate: (kind: AdminCatalog, values: Record<string, string>) => void;
  onApprove: (kind: AdminCatalog, item: Row, approvalRef: string, supersedesId: string) => void;
  onRevoke: (kind: AdminCatalog, item: Row, changeRef: string) => void;
}) {
  const definition = adminCatalogDefinitions[kind];
  const idField = kind === "tariffs" ? "tariff_id" : kind === "products" ? "product_id" : "id";
  function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = Object.fromEntries([...new FormData(form).entries()].map(([key, value]) => [key, String(value).trim()]));
    onCreate(kind, values);
    form.reset();
  }
  return <Panel title={definition.title}>
    <form className="form-grid admin-catalog-form" onSubmit={create}>
      {definition.fields.map((field) => <label key={field.name}>{field.label}<input name={field.name} type={field.type ?? "text"} step={field.step} required={field.required !== false} /></label>)}
      <div className="form-actions span-two"><Button primary type="submit" disabled={busy}>Créer un brouillon</Button></div>
      <p className="help span-two">Le brouillon n’est pas applicable aux factures. Il faut une référence d’approbation saisie par SEL pour l’activer.</p>
    </form>
    <Table headers={[...definition.columns.map((field) => field.replace(/_/g, " ")), "Approbation"]} empty="Aucune entrée retournée par le serveur central." data={items.map((item) => {
      const id = text(item.id ?? item[idField], "");
      const status = text(item.status).toUpperCase();
      const approval = status === "DRAFT" ? <form className="admin-approve-form" onSubmit={(event) => {
        event.preventDefault();
        const values = new FormData(event.currentTarget);
        onApprove(kind, item, String(values.get("approval_ref") ?? "").trim(), String(values.get("supersedes_id") ?? "").trim());
      }}><input aria-label="Référence d’approbation SEL" name="approval_ref" placeholder="Référence SEL" required /><input aria-label="Version remplacée (facultatif)" name="supersedes_id" placeholder="UUID remplacé (facultatif)" /><Button primary type="submit" disabled={busy || !id}>Approuver</Button></form> : status === "APPROVED" ? <form className="admin-approve-form" onSubmit={(event) => {
        event.preventDefault();
        const changeRef = String(new FormData(event.currentTarget).get("change_ref") ?? "").trim();
        onRevoke(kind, item, changeRef);
      }}><input aria-label="Référence de révocation SEL" name="change_ref" placeholder="Référence SEL" required /><Button type="submit" disabled={busy || !id}>Révoquer</Button></form> : <Status value={status || "État inconnu"} />;
      return [...definition.columns.map((field) => field === "status" ? <Status value={item[field]} /> : text(item[field])), approval];
    })} />
  </Panel>;
}

function AdminConsole({ session, onSignedOut }: { session: CentralSessionInfo; onSignedOut: () => void }) {
  const [users, setUsers] = useState<Row[]>([]);
  const [tariffs, setTariffs] = useState<Row[]>([]);
  const [products, setProducts] = useState<Row[]>([]);
  const [taxProfiles, setTaxProfiles] = useState<Row[]>([]);
  const [fxRates, setFxRates] = useState<Row[]>([]);
  const [auditEvents, setAuditEvents] = useState<Row[]>([]);
  const [roleDrafts, setRoleDrafts] = useState<Record<string, string>>({});
  const [passwordTarget, setPasswordTarget] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(false);
  const [catalogBusy, setCatalogBusy] = useState(false);
  const authenticated = useRef(false);

  function clearAdminSession() {
    authenticated.current = false;
    setUsers([]); setTariffs([]); setProducts([]); setTaxProfiles([]); setFxRates([]); setAuditEvents([]);
  }

  const loadData = useCallback(async () => {
    setLoading(true);
    const paths = [
      "/v1/admin/users",
      "/v1/admin/catalog/tariffs", "/v1/admin/catalog/products",
      "/v1/admin/catalog/tax-profiles", "/v1/admin/catalog/fx-rates", "/v1/admin/audit-events",
    ];
    const results = await Promise.allSettled(paths.map((path) => centralAdminRequest<unknown>(path)));
    if (!authenticated.current) return;
    const setters = [setUsers, setTariffs, setProducts, setTaxProfiles, setFxRates, setAuditEvents] as const;
    results.forEach((result, index) => { if (result.status === "fulfilled") setters[index](rows<Row>(result.value)); });
    const failed = results.filter((result) => result.status === "rejected");
    if (failed.length) {
      const current = await centralSessionStatus().catch(() => ({ authenticated: false }));
      if (!current.authenticated) {
        clearAdminSession();
        setMessage("La session centrale a expiré. Reconnectez-vous pour continuer."); setError(true); setLoading(false);
        return;
      }
    }
    setMessage(failed.length ? `${6 - failed.length} rubrique(s) chargée(s). ${failed.length} lecture(s) refusée(s) ou indisponible(s).` : "Données d’administration actualisées depuis le serveur central.");
    setError(failed.length > 0);
    setLoading(false);
  }, []);

  useEffect(() => {
    authenticated.current = session.authenticated;
    if (session.authenticated && session.role === "ADMIN") void loadData();
    else clearAdminSession();
  }, [session, loadData]);

  async function createUser(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    try {
      await centralAdminRequest("/v1/admin/users", "POST", {
        identity_ref: String(values.get("identity_ref") ?? "").trim(),
        role: String(values.get("role") ?? "BILLING_AGENT"),
        password: String(values.get("password") ?? ""),
        require_password_change: values.get("require_password_change") === "on",
      });
      form.reset(); setMessage("Utilisateur créé par le serveur central."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
  }

  async function toggleUser(user: Row) {
    const id = text(user.id ?? user.user_id, "");
    const status = text(user.status).toUpperCase() === "ACTIVE" ? "DISABLED" : "ACTIVE";
    if (!id) return;
    try {
      await centralAdminRequest(`/v1/admin/users/${encodeURIComponent(id)}`, "PATCH", { status });
      setMessage(status === "ACTIVE" ? "Compte réactivé." : "Compte désactivé. Les sessions associées sont révoquées côté serveur.");
      setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
  }

  async function updateUserRole(user: Row) {
    const id = text(user.id ?? user.user_id, "");
    const role = roleDrafts[id];
    if (!id || !role || role === text(user.role)) return;
    try {
      await centralAdminRequest(`/v1/admin/users/${encodeURIComponent(id)}`, "PATCH", { role });
      setMessage("Rôle modifié par le serveur central."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
  }

  async function resetUserPassword(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!passwordTarget) return;
    const form = event.currentTarget;
    const password = String(new FormData(form).get("password") ?? "");
    try {
      await centralAdminRequest(`/v1/users/${encodeURIComponent(passwordTarget)}/password`, "POST", { password });
      form.reset(); setPasswordTarget(""); setMessage("Mot de passe mis à jour. Les sessions existantes sont révoquées côté serveur."); setError(false); await loadData();
    } catch (reason) { form.reset(); setMessage(humanError(reason)); setError(true); }
  }

  async function createCatalogDraft(kind: AdminCatalog, values: Record<string, string>) {
    setCatalogBusy(true);
    try {
      const body = { ...values, effective_to: values.effective_to || null };
      await centralAdminRequest(`/v1/admin/catalog/${kind}`, "POST", body);
      setMessage(`Brouillon ${adminCatalogDefinitions[kind].title.toLocaleLowerCase("fr")} transmis au serveur. Il n’est pas encore applicable.`); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setCatalogBusy(false); }
  }

  async function approveCatalogDraft(kind: AdminCatalog, item: Row, approvalRef: string, supersedesId: string) {
    const id = text(item.id ?? item[kind === "tariffs" ? "tariff_id" : "product_id"], "");
    if (!id || !approvalRef) return;
    setCatalogBusy(true);
    try {
      const supersedesKey = kind === "tariffs" ? "supersedes_tariff_id" : kind === "products" ? "supersedes_product_id" : "supersedes_id";
      const body = { approval_ref: approvalRef, [supersedesKey]: supersedesId || null };
      await centralAdminRequest(`/v1/admin/catalog/${kind}/${encodeURIComponent(id)}/approve`, "POST", body);
      setMessage("Référentiel approuvé par le serveur central avec la référence fournie."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setCatalogBusy(false); }
  }

  async function revokeCatalogItem(kind: AdminCatalog, item: Row, changeRef: string) {
    const id = text(item.id ?? item[kind === "tariffs" ? "tariff_id" : "product_id"], "");
    if (!id || !changeRef) return;
    setCatalogBusy(true);
    try {
      await centralAdminRequest(`/v1/admin/catalog/${kind}/${encodeURIComponent(id)}/revoke`, "POST", { change_ref: changeRef });
      setMessage("Révocation enregistrée par le serveur central avec la référence fournie."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setCatalogBusy(false); }
  }

  if (session.role !== "ADMIN") return <Panel title="Accès administrateur requis">
    <p className="muted">La session {text(session.role)} est authentifiée, mais ne possède pas le rôle ADMIN nécessaire à ces fonctions.</p>
    <div className="form-actions"><Button onClick={onSignedOut}>Fermer la session</Button></div>
  </Panel>;

  return <>
    {message && <div className={`notice${error ? " error" : ""}`} role={error ? "alert" : "status"}>{message}</div>}
    <Panel title="Session administrateur" action={<div className="page-actions"><Button onClick={() => void loadData()} disabled={loading}>{loading ? "Actualisation…" : "Actualiser"}</Button><Button onClick={onSignedOut}>Se déconnecter</Button></div>}>
      <dl className="customer-details admin-session"><div><dt>Utilisateur</dt><dd>{text(session.user_id)}</dd></div><div><dt>Rôle</dt><dd>{text(session.role)}</dd></div><div><dt>Organisation</dt><dd>{text(session.organization_id)}</dd></div></dl>
    </Panel>
    <Panel title="Utilisateurs et rôles">
      <form className="form-grid admin-create-user" onSubmit={(event) => void createUser(event)}>
        <label>Identifiant utilisateur<input name="identity_ref" autoComplete="off" required /></label>
        <label>Rôle<select name="role" defaultValue="BILLING_AGENT"><option value="BILLING_AGENT">Agent facturation</option><option value="CASHIER">Caissier</option><option value="SUPERVISOR">Superviseur</option><option value="AUDITOR_READONLY">Auditeur lecture seule</option><option value="ADMIN">Administrateur</option></select></label>
        <label>Nouveau mot de passe<input name="password" type="password" minLength={8} autoComplete="new-password" required /></label>
        <label><span>Connexion initiale</span><span><input name="require_password_change" type="checkbox" defaultChecked /> Exiger le changement du mot de passe à la première connexion</span></label>
        <div className="form-actions"><Button primary type="submit">Créer l’utilisateur</Button></div>
      </form>
      <Table headers={["Identifiant", "Rôle", "État", "Actions"]} empty="Aucun utilisateur retourné par le serveur." data={users.map((user) => {
        const id = text(user.id ?? user.user_id, "");
        return [text(user.identity_ref), <div className="admin-role-control"><select aria-label={`Rôle de ${text(user.identity_ref)}`} value={roleDrafts[id] ?? text(user.role)} onChange={(event) => setRoleDrafts((current) => ({ ...current, [id]: event.target.value }))}><option value="ADMIN">Administrateur</option><option value="BILLING_AGENT">Agent facturation</option><option value="CASHIER">Caissier</option><option value="SUPERVISOR">Superviseur</option><option value="AUDITOR_READONLY">Auditeur lecture seule</option></select><Button disabled={!roleDrafts[id] || roleDrafts[id] === text(user.role)} onClick={() => void updateUserRole(user)}>Enregistrer</Button></div>, <Status value={user.status} />, <div className="admin-user-actions"><Button onClick={() => setPasswordTarget(id)}>Réinitialiser le mot de passe</Button><Button onClick={() => void toggleUser(user)}>{text(user.status).toUpperCase() === "ACTIVE" ? "Désactiver" : "Réactiver"}</Button></div>];
      })} />
      {passwordTarget && <form className="form-grid admin-password-reset" onSubmit={(event) => void resetUserPassword(event)}><p className="help span-two">Les sessions existantes de ce compte seront révoquées après validation côté serveur.</p><label>Nouveau mot de passe<input type="password" name="password" minLength={8} autoComplete="new-password" required /></label><div className="page-actions"><Button type="button" onClick={() => setPasswordTarget("")}>Annuler</Button><Button primary type="submit">Confirmer le changement</Button></div></form>}
    </Panel>
    <div className="admin-grid admin-catalog-grid">
      <AdminCatalogPanel kind="tariffs" items={tariffs} busy={catalogBusy} onCreate={createCatalogDraft} onApprove={approveCatalogDraft} onRevoke={revokeCatalogItem} />
      <AdminCatalogPanel kind="products" items={products} busy={catalogBusy} onCreate={createCatalogDraft} onApprove={approveCatalogDraft} onRevoke={revokeCatalogItem} />
      <AdminCatalogPanel kind="tax-profiles" items={taxProfiles} busy={catalogBusy} onCreate={createCatalogDraft} onApprove={approveCatalogDraft} onRevoke={revokeCatalogItem} />
      <AdminCatalogPanel kind="fx-rates" items={fxRates} busy={catalogBusy} onCreate={createCatalogDraft} onApprove={approveCatalogDraft} onRevoke={revokeCatalogItem} />
    </div>
    <Panel title="Journal d’audit récent"><p className="help">Lecture seule — 100 événements les plus récents associés à cette organisation. Les métadonnées sensibles ne sont pas affichées.</p><Table headers={["Action", "Agent", "Élément concerné", "Date"]} empty="Aucun événement d’audit disponible pour cette organisation." data={auditEvents.map((event) => [<strong>{text(event.event_type)}</strong>, text(event.actor_identity ?? event.actor_id ?? event.device_id), text(event.subject_id), formatDateTime(event.occurred_at)])} /></Panel>
  </>;
}
