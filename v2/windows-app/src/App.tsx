import { forwardRef, useCallback, useEffect, useMemo, useRef, useState, type ButtonHTMLAttributes, type FormEvent, type ReactNode } from "react";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";
import { centralAdminRequest, centralLogin, centralLogout, centralSessionStatus, centralRequest, centralRequestPage, centralStatus, openMonarqueSite, type CentralSessionInfo } from "./api";
import { buildClientPdf, renderClientPdfPreview } from "./documents";
import "./App.css";

export type View = "dashboard" | "customers" | "readings" | "invoices" | "payments" | "solar" | "receivables" | "server" | "admin";
const PRIMARY_CURRENCY = "USD";
const CURRENCY_OPTIONS: { value: string; label: string }[] = [
  { value: "USD", label: "USD · principale" },
  { value: "CDF", label: "CDF" },
];
type Domain = "home" | "customers" | "billing" | "payments" | "solar" | "admin";
type Row = Record<string, unknown>;
type Notice = { message: string; error?: boolean } | null;
type PreviewDocument = { title: string; filename: string; blob: Blob };
type FinancialAction = { kind: "void-invoice" | "reverse-payment" | "credit-receivable"; id: string; amount?: string; reference: string };
type ConfirmationRequest = { title: string; description: string; confirmLabel: string; tone?: "neutral" | "warning" };

function announceDraftSaved() { window.dispatchEvent(new Event("sel-draft-saved")); }

async function loadAllCustomers(): Promise<Row[]> {
  const customers: Row[] = [];
  const cursors = new Set<string>();
  let cursor = "";
  while (true) {
    const query = new URLSearchParams({ limit: "100" });
    if (cursor) query.set("cursor", cursor);
    const page = await centralRequestPage<unknown>(`/v1/customers?${query.toString()}`);
    customers.push(...rows<Row>(page.body));
    const next = page.next_cursor?.trim() ?? "";
    if (!next) return customers;
    if (cursors.has(next) || next === cursor) throw new Error("Le serveur a renvoyé un curseur client répétitif. Actualisez ou contactez l’administrateur SEL.");
    cursors.add(next);
    cursor = next;
  }
}

const navigation: { id: Domain; target: View; label: string; icon: string }[] = [
  { id: "home", target: "dashboard", label: "Accueil", icon: "grid" },
  { id: "customers", target: "customers", label: "Abonnés", icon: "people" },
  { id: "billing", target: "invoices", label: "Facturation", icon: "document" },
  { id: "payments", target: "payments", label: "Paiements", icon: "cash" },
  { id: "solar", target: "solar", label: "Solaire", icon: "sun" },
  { id: "admin", target: "admin", label: "Administration", icon: "settings" },
];

const domainByView: Record<View, Domain> = {
  dashboard: "home", customers: "customers", readings: "billing", invoices: "billing",
  payments: "payments", receivables: "payments", solar: "solar", server: "admin", admin: "admin",
};

const domainViews: Partial<Record<Domain, { id: View; label: string }[]>> = {
  billing: [{ id: "invoices", label: "Factures" }, { id: "readings", label: "Relevés" }],
  payments: [{ id: "payments", label: "Journal de caisse" }, { id: "receivables", label: "Créances" }],
  admin: [{ id: "admin", label: "Administration" }, { id: "server", label: "État du serveur" }],
};

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
    INVOICE: "Facture eau + électricité",
    SOLAR_SALE: "Vente de kit solaire",
    SALE: "Vente de kit solaire",
    SOLAR_KIT: "Kit solaire",
    CASH: "Espèces",
    BANK: "Banque",
    MOBILE_MONEY: "Mobile Money",
    OTHER: "Autre",
    ELECTRICITY: "Électricité",
    WATER: "Eau",
    COMMUNICATION: "Ligne de communication",
    CONNECTED: "Raccordé",
    DISCONNECTED: "Non raccordé",
    UNKNOWN: "À vérifier",
  };
  return labels[raw.toUpperCase()] ?? raw;
}

function rows<T extends Row>(value: unknown): T[] {
  return Array.isArray(value) ? value as T[] : [];
}

function record(value: unknown): Row {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Row : {};
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

const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean }>(function Button({ children, primary = false, className = "", ...props }, ref) {
  return <button ref={ref} className={`button${primary ? " button-primary" : ""}${className ? ` ${className}` : ""}`} {...props}>{children}</button>;
});

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
  const [solarWorkspace, setSolarWorkspace] = useState<"sales" | "contracts">("sales");
  const [solarProductCode, setSolarProductCode] = useState("");
  const [solarPaymentMode, setSolarPaymentMode] = useState<"CASH" | "INSTALLMENT">("CASH");
  const [solarContractSalePrefill, setSolarContractSalePrefill] = useState("");
  const [paymentWorkspace, setPaymentWorkspace] = useState<"journal" | "create">("journal");
  const [invoiceWorkspace, setInvoiceWorkspace] = useState<"history" | "create">("history");
  const [viewHistory, setViewHistory] = useState<View[]>([]);
  const [draftDirty, setDraftDirty] = useState(false);
  const [financialAction, setFinancialAction] = useState<FinancialAction | null>(null);
  const [financialActionAmount, setFinancialActionAmount] = useState("");
  const [financialActionReason, setFinancialActionReason] = useState("");
  const [financialActionBusy, setFinancialActionBusy] = useState(false);
  const [confirmationRequest, setConfirmationRequest] = useState<ConfirmationRequest | null>(null);
  const confirmationResolver = useRef<((accepted: boolean) => void) | null>(null);
  const confirmationCancelRef = useRef<HTMLButtonElement | null>(null);
  const confirmationPreviousFocus = useRef<HTMLElement | null>(null);
  const [customers, setCustomers] = useState<Row[]>([]);
  const [invoices, setInvoices] = useState<Row[]>([]);
  const [payments, setPayments] = useState<Row[]>([]);
  const [sales, setSales] = useState<Row[]>([]);
  const [solarProducts, setSolarProducts] = useState<Row[]>([]);
  const [tariffCatalog, setTariffCatalog] = useState<Row[]>([]);
  const [solarQuote, setSolarQuote] = useState<Row | null>(null);
  const [solarSaleDraft, setSolarSaleDraft] = useState<Row | null>(null);
  const [solarSaleOpen, setSolarSaleOpen] = useState(false);
  const [serverStatus, setServerStatus] = useState<Row | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [loading, setLoading] = useState(false);
  const [selectedCustomer, setSelectedCustomer] = useState("");
  const [paymentPrefill, setPaymentPrefill] = useState<{ receivableId: string; scheduleId: string; amount?: string } | null>(null);
  const [selectedCustomerData, setSelectedCustomerData] = useState<Row | null>(null);
  const [customerEditOpen, setCustomerEditOpen] = useState(false);
  const [customerEditBusy, setCustomerEditBusy] = useState(false);
  const [customerCreateBusy, setCustomerCreateBusy] = useState(false);
  const [statementState, setStatementState] = useState<{ customerId: string; data: Row | null; loading: boolean; error: string }>({ customerId: "", data: null, loading: false, error: "" });
  const [receivablesState, setReceivablesState] = useState<{ data: Row | null; loading: boolean; error: string }>({ data: null, loading: false, error: "" });
  const [selectedReceivableId, setSelectedReceivableId] = useState("");
  const [receivableDetail, setReceivableDetail] = useState<Row | null>(null);
  const [receivableDetailId, setReceivableDetailId] = useState("");
  const [receivableDetailBusy, setReceivableDetailBusy] = useState(false);
  const [preview, setPreview] = useState<Row | null>(null);
  const [invoiceDraft, setInvoiceDraft] = useState<Row | null>(null);
  const [invoiceIssueBusy, setInvoiceIssueBusy] = useState(false);
  const invoiceIssueBusyRef = useRef(false);
  const [selectedInvoice, setSelectedInvoice] = useState<Row | null>(null);
  const [zeroConfirmed, setZeroConfirmed] = useState(false);
  const [invoiceFormKey, setInvoiceFormKey] = useState(0);
  const [invoiceCustomerId, setInvoiceCustomerId] = useState("");
  const [invoiceBillingPeriod, setInvoiceBillingPeriod] = useState("");
  const [invoiceWaterQuantity, setInvoiceWaterQuantity] = useState("0");
  const [invoiceFilter, setInvoiceFilter] = useState("");
  const [customerFilter, setCustomerFilter] = useState("");
  const [customerCreateOpen, setCustomerCreateOpen] = useState(false);
  const [documentPreview, setDocumentPreview] = useState<PreviewDocument | null>(null);
  const [documentBusy, setDocumentBusy] = useState(false);

  function requestConfirmation(request: ConfirmationRequest): Promise<boolean> {
    if (confirmationResolver.current) return Promise.resolve(false);
    confirmationPreviousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setConfirmationRequest(request);
    return new Promise((resolve) => { confirmationResolver.current = resolve; });
  }

  function finishConfirmation(accepted: boolean) {
    const resolve = confirmationResolver.current;
    confirmationResolver.current = null;
    setConfirmationRequest(null);
    resolve?.(accepted);
  }

  useEffect(() => {
    if (confirmationRequest) {
      confirmationCancelRef.current?.focus();
      return;
    }
    const previous = confirmationPreviousFocus.current;
    confirmationPreviousFocus.current = null;
    if (previous?.isConnected) previous.focus();
  }, [confirmationRequest]);

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
      setTariffCatalog([]);
      setStatementState({ customerId: "", data: null, loading: false, error: "" }); setSelectedCustomerData(null); setSelectedInvoice(null); setPreview(null);
      setNotice({ message: "Le serveur SEL est indisponible ou non configuré. Les données métier sont masquées jusqu’au rétablissement du service.", error: true });
    };
    window.addEventListener("sel-session-expired", expired);
    window.addEventListener("sel-server-unavailable", unavailable);
    return () => { active = false; window.removeEventListener("sel-session-expired", expired); window.removeEventListener("sel-server-unavailable", unavailable); };
  }, []);

  useEffect(() => {
    const saved = () => setDraftDirty(false);
    window.addEventListener("sel-draft-saved", saved);
    return () => window.removeEventListener("sel-draft-saved", saved);
  }, []);

  const refresh = useCallback(async () => {
    if (!session?.authenticated) return;
    setLoading(true);
    try {
      const [customerResult, invoiceResult, paymentResult, salesResult, productsResult, tariffsResult, statusResult] = await Promise.allSettled([
        loadAllCustomers(),
        centralRequest<unknown>("/v1/invoices?limit=100"),
        centralRequest<unknown>("/v1/payments?limit=100"),
        centralRequest<unknown>("/v1/solar/sales?limit=100"),
        centralRequest<unknown>("/v1/catalog/products"),
        centralRequest<unknown>("/v1/catalog/tariffs"),
        centralStatus<Row>(),
      ]);
      setCustomers(customerResult.status === "fulfilled" ? rows(customerResult.value) : []);
      setInvoices(invoiceResult.status === "fulfilled" ? rows(invoiceResult.value) : []);
      setPayments(paymentResult.status === "fulfilled" ? rows(paymentResult.value) : []);
      setSales(salesResult.status === "fulfilled" ? rows(salesResult.value) : []);
      setSolarProducts(productsResult.status === "fulfilled" ? rows(productsResult.value) : []);
      setTariffCatalog(tariffsResult.status === "fulfilled" ? rows(tariffsResult.value) : []);
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
      const failure = [customerResult, invoiceResult, paymentResult, salesResult, productsResult, tariffsResult, statusResult].find((result) => result.status === "rejected");
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
    finally { setSession({ authenticated: false }); setCustomers([]); setInvoices([]); setPayments([]); setSales([]); setSolarProducts([]); setTariffCatalog([]); }
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
    if (!selectedCustomer) { setStatementState({ customerId: "", data: null, loading: false, error: "" }); return; }
    let active = true;
    setStatementState({ customerId: selectedCustomer, data: null, loading: true, error: "" });
    centralRequest<Row>(`/v1/customers/${encodeURIComponent(selectedCustomer)}/statement`)
      .then((data) => { if (active) setStatementState({ customerId: selectedCustomer, data, loading: false, error: "" }); })
      .catch((error: unknown) => {
        if (!active) return;
        const message = humanError(error);
        setStatementState({ customerId: selectedCustomer, data: null, loading: false, error: message });
        setNotice({ message, error: true });
      });
    return () => { active = false; };
  }, [selectedCustomer]);

  useEffect(() => {
    if (view !== "receivables") return;
    let active = true;
    setReceivablesState({ data: null, loading: true, error: "" });
    setSelectedReceivableId("");
    setReceivableDetail(null);
    setReceivableDetailId("");
    centralRequest<Row>("/v1/receivables")
      .then((data) => { if (active) setReceivablesState({ data, loading: false, error: "" }); })
      .catch((error: unknown) => {
        if (!active) return;
        setReceivablesState({ data: null, loading: false, error: humanError(error) });
      });
    return () => { active = false; };
  }, [view]);

  const currentStatement = statementState.customerId === selectedCustomer ? statementState : null;

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
  const invoiceCustomer = customers.find((customer) => text(customer.customer_id ?? customer.id, "") === invoiceCustomerId);
  const invoiceCategory = text(invoiceCustomer?.customer_category, "").trim().toUpperCase();
  const invoiceEffectiveDate = invoiceBillingPeriod ? `${invoiceBillingPeriod}-01` : "";
  const applicableTariffs = useMemo(() => {
    if (!invoiceCategory || !invoiceEffectiveDate) return { revisions: [] as string[], electricity: [] as Row[], water: [] as Row[] };
    const matching = tariffCatalog.filter((row) => {
      const from = text(row.effective_from, "");
      const to = text(row.effective_to, "");
      return text(row.customer_category, "").trim().toUpperCase() === invoiceCategory
        && text(row.currency, "").trim().toUpperCase() === PRIMARY_CURRENCY
        && text(row.approval_ref, "").trim() !== ""
        && from <= invoiceEffectiveDate
        && (!to || to >= invoiceEffectiveDate);
    });
    const electricity = matching.filter((row) => text(row.service_code, "").toUpperCase() === "ELECTRICITY" && text(row.unit, "").toUpperCase() === "KWH");
    const water = matching.filter((row) => text(row.service_code, "").toUpperCase() === "WATER" && text(row.unit, "").toUpperCase() === "M3");
    const waterRequired = Number(invoiceWaterQuantity) > 0;
    const revisions = [...new Set(electricity
      .map((row) => text(row.approval_ref, "").trim())
      .filter((revision) => !waterRequired || water.some((row) => text(row.approval_ref, "").trim() === revision)))];
    return { revisions, electricity, water };
  }, [tariffCatalog, invoiceCategory, invoiceEffectiveDate, invoiceWaterQuantity]);
  const waterTariffRequired = Number(invoiceWaterQuantity) > 0;
  const invoiceTariffRevision = applicableTariffs.electricity.length === 1
    && (!waterTariffRequired || applicableTariffs.water.length === 1)
    && applicableTariffs.revisions.length === 1
    ? applicableTariffs.revisions[0]
    : "";
  async function submit(form: HTMLFormElement, path: string, payload: Row, success: string): Promise<boolean> {
    try {
      await centralRequest(path, "POST", payload);
      setDraftDirty(false);
      setNotice({ message: success });
      form.reset();
      await refresh();
      return true;
    } catch (error) { setNotice({ message: humanError(error), error: true }); return false; }
  }

  async function createCustomer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    const payload = {
      customer_code: String(values.get("customer_code") ?? "").trim(),
      legal_name: String(values.get("legal_name") ?? "").trim(),
      customer_category: String(values.get("customer_category") ?? "").trim(),
      address: String(values.get("address") ?? "").trim(),
      phone: String(values.get("phone") ?? "").trim(),
    };
    const categoryLabel = payload.customer_category === "RESIDENTIAL" ? "Résidentiel" : payload.customer_category === "COMMERCIAL" ? "Commercial" : payload.customer_category;
    setCustomerCreateBusy(true);
    try {
      const accepted = await requestConfirmation({
        title: "Vérifier le nouvel abonné",
        description: `Code : ${payload.customer_code}. Nom : ${payload.legal_name}. Catégorie tarifaire : ${categoryLabel}. Adresse : ${payload.address || "non renseignée"}. Téléphone : ${payload.phone || "non renseigné"}. État initial : Actif, selon la règle du serveur SEL.`,
        confirmLabel: "Créer l’abonné",
      });
      if (!accepted) return;
      await centralRequest("/v1/customers", "POST", payload);
      setDraftDirty(false);
      setNotice({ message: "Abonné enregistré sur le serveur SEL." });
      form.reset();
      setCustomerCreateOpen(false);
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
    finally { setCustomerCreateBusy(false); }
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
      currency: PRIMARY_CURRENCY,
      tariff_revision: invoiceTariffRevision,
      due_date: form.get("due_date") || null,
    };
    if (!invoiceTariffRevision) {
      const reason = !invoiceCategory
        ? "La catégorie tarifaire de cet abonné doit être renseignée avant la facturation."
        : applicableTariffs.electricity.length === 0
          ? `Aucun tarif d’électricité approuvé en USD/kWh ne couvre ${invoiceBillingPeriod} pour la catégorie ${invoiceCategory}. Un administrateur doit vérifier le catalogue et ses dates d’effet.`
          : Number(water) > 0 && applicableTariffs.water.length === 0
            ? `Aucun tarif d’eau approuvé en USD/m³ ne couvre ${invoiceBillingPeriod} pour la catégorie ${invoiceCategory}.`
            : applicableTariffs.revisions.length === 0
              ? `Les tarifs d’électricité et d’eau applicables à ${invoiceBillingPeriod} n’ont pas la même référence d’approbation SEL.`
              : "Plusieurs tarifs d’électricité se chevauchent pour cette catégorie et cette période. Un administrateur doit corriger le catalogue.";
      setNotice({ message: reason, error: true });
      return;
    }
    try {
      const result = await centralRequest<Row>("/v1/billing/utility/preview", "POST", payload);
      setInvoiceDraft(payload);
      setPreview(result);
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  async function confirmInvoice() {
    if (!invoiceDraft || invoiceIssueBusyRef.current) return;
    invoiceIssueBusyRef.current = true;
    setInvoiceIssueBusy(true);
    try {
      await centralRequest("/v1/billing/utility/issue", "POST", invoiceDraft);
      setDraftDirty(false);
      setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
      setInvoiceWorkspace("history");
      setInvoiceFormKey((key) => key + 1);
      setInvoiceCustomerId(""); setInvoiceBillingPeriod(""); setInvoiceWaterQuantity("0");
      setNotice({ message: "Facture enregistrée par le serveur SEL." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
    finally { invoiceIssueBusyRef.current = false; setInvoiceIssueBusy(false); }
  }

  async function openInvoice(invoice: Row) {
    const id = text(invoice.invoice_id ?? invoice.id, "");
    if (!id) return;
    try {
      const details = await centralRequest<Row>(`/v1/invoices/${encodeURIComponent(id)}`);
      setSelectedInvoice(details);
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function beginFinancialAction(action: FinancialAction) {
    setFinancialAction(action);
    setFinancialActionAmount(action.amount ?? "");
    setFinancialActionReason("");
  }

  async function commitFinancialAction(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!financialAction || !financialActionReason.trim()) return;
    const { kind, id } = financialAction;
    const path = kind === "void-invoice"
      ? `/v1/invoices/${encodeURIComponent(id)}/void`
      : kind === "reverse-payment"
        ? `/v1/payments/${encodeURIComponent(id)}/reverse`
        : `/v1/receivables/${encodeURIComponent(id)}/credit`;
    const payload = kind === "void-invoice"
      ? { reason: financialActionReason.trim() }
      : { amount: financialActionAmount, reason: financialActionReason.trim() };
    setFinancialActionBusy(true);
    try {
      await centralRequest(path, "POST", payload);
      setNotice({ message: kind === "void-invoice" ? "Annulation ajoutée à l’historique financier." : kind === "reverse-payment" ? "Contre-écriture ajoutée à l’historique financier." : "Avoir ajouté à l’historique de la créance." });
      setFinancialAction(null);
      setFinancialActionAmount(""); setFinancialActionReason("");
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
    finally { setFinancialActionBusy(false); }
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
    if (!selectedCustomerData || customerEditBusy) return;
    const form = new FormData(event.currentTarget);
    const payload = {
      legal_name: String(form.get("legal_name") ?? "").trim(),
      customer_category: String(form.get("customer_category") ?? "").trim(),
      address: String(form.get("address") ?? "").trim() || null,
      phone: String(form.get("phone") ?? "").trim() || null,
    };
    const categoryLabel = payload.customer_category === "RESIDENTIAL" ? "Résidentiel" : payload.customer_category === "COMMERCIAL" ? "Commercial" : payload.customer_category;
    setCustomerEditBusy(true);
    try {
      const accepted = await requestConfirmation({
        title: "Vérifier les modifications",
        description: `Abonné : ${text(selectedCustomerData.customer_code)}. Nouveau nom : ${payload.legal_name}. Catégorie tarifaire : ${categoryLabel}. Adresse : ${payload.address || "non renseignée"}. Téléphone : ${payload.phone || "non renseigné"}.`,
        confirmLabel: "Enregistrer les modifications",
      });
      if (!accepted) return;
      const updated = await centralRequest<Row>(`/v1/customers/${encodeURIComponent(selectedCustomer)}`, "PATCH", payload, Number(selectedCustomerData.revision));
      setSelectedCustomerData(updated);
      setDraftDirty(false);
      setCustomerEditOpen(false);
      setNotice({ message: "Dossier abonné modifié sur le serveur central." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
    finally { setCustomerEditBusy(false); }
  }

  async function toggleCustomerArchive() {
    if (!selectedCustomer || !selectedCustomerData) return;
    const currentlyArchived = text(selectedCustomerData.status).toUpperCase() === "ARCHIVED";
    const nextStatus = currentlyArchived ? "ACTIVE" : "ARCHIVED";
    const accepted = await requestConfirmation({
      title: currentlyArchived ? "Réactiver cet abonné ?" : "Archiver cet abonné ?",
      description: currentlyArchived
        ? "Son historique et ses factures seront conservés."
        : "Il restera consultable et ses factures historiques seront conservées.",
      confirmLabel: currentlyArchived ? "Réactiver l’abonné" : "Archiver l’abonné",
      tone: "warning",
    });
    if (!accepted) return;
    try {
      const updated = await centralRequest<Row>(`/v1/customers/${encodeURIComponent(selectedCustomer)}`, "PATCH", { status: nextStatus }, Number(selectedCustomerData.revision));
      setSelectedCustomerData(updated);
      setNotice({ message: currentlyArchived ? "Abonné réactivé." : "Abonné archivé. Son historique reste consultable." });
      await refresh();
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function confirmMeterReplacement(oldSerial: string, newSerial: string, reason: string) {
    return requestConfirmation({
      title: "Remplacer ce compteur ?",
      description: `Le compteur ${oldSerial} sera désactivé, sans supprimer ses relevés ni les factures associées. Le nouveau compteur ${newSerial} sera créé pour le même service. Motif : ${reason}`,
      confirmLabel: "Confirmer le remplacement",
      tone: "warning",
    });
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
    const selectedProduct = solarProducts.find((product) => text(product.sku) === solarProductCode);
    if (!selectedProduct) {
      setNotice({ message: "Sélectionnez un kit approuvé dans le catalogue SEL.", error: true });
      return;
    }
    const draft = {
      customer_id: form.get("customer_id"),
      product_code: solarProductCode,
      currency: text(selectedProduct.currency, PRIMARY_CURRENCY),
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
      const sale = await centralRequest<Row>("/v1/solar/sales", "POST", solarSaleDraft);
      const customerId = text(solarSaleDraft.customer_id, "");
      const receivableId = text(sale.receivable_id, "");
      const saleId = text(sale.sale_id, "");
      const paymentMode = solarPaymentMode;
      setDraftDirty(false);
      setSolarQuote(null);
      setSolarSaleDraft(null);
      setSolarProductCode("");
      setSolarSaleOpen(false);
      await refresh();
      if (paymentMode === "CASH" && customerId && receivableId) {
        if (await navigateTo("payments", customerId)) {
          setPaymentPrefill({ receivableId, scheduleId: "", amount: text(sale.principal ?? solarSaleDraft.principal, "") });
          setNotice({ message: "Vente enregistrée. Sa créance est présélectionnée : saisissez le montant reçu et confirmez le règlement comptant." });
        }
      } else if (paymentMode === "INSTALLMENT" && saleId) {
        setSolarContractSalePrefill(saleId);
        setSolarWorkspace("contracts");
        setNotice({ message: "Vente enregistrée. Vérifiez maintenant les conditions et l’échéancier du contrat." });
      } else {
        setNotice({ message: "Vente enregistrée sur le serveur SEL avec sa créance. Le règlement reste à enregistrer dans le journal des paiements." });
      }
    } catch (error) { setNotice({ message: humanError(error), error: true }); }
  }

  function discardPendingDraft() {
    setDraftDirty(false);
    setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
    setCustomerCreateOpen(false); setCustomerEditOpen(false);
    setSolarSaleOpen(false); setSolarQuote(null); setSolarSaleDraft(null); setSolarProductCode("");
  }

  async function returnToInvoiceHistory() {
    if (draftDirty && !await requestConfirmation({ title: "Abandonner la facture ?", description: "Cette facture n’est pas encore émise. Ses saisies seront effacées si vous revenez à l’historique.", confirmLabel: "Abandonner et continuer" })) return;
    setDraftDirty(false);
    setPreview(null); setInvoiceDraft(null); setZeroConfirmed(false);
    setInvoiceFormKey((key) => key + 1);
    setInvoiceCustomerId(""); setInvoiceBillingPeriod(""); setInvoiceWaterQuantity("0");
    setSelectedInvoice(null);
    setInvoiceWorkspace("history");
  }

  async function returnToPaymentJournal() {
    if (draftDirty && !await requestConfirmation({ title: "Abandonner le règlement ?", description: "Ce règlement n’est pas encore enregistré. Les saisies seront effacées si vous revenez au journal.", confirmLabel: "Abandonner et continuer" })) return;
    setDraftDirty(false);
    setPaymentWorkspace("journal");
  }

  async function navigateTo(nextView: View, customerId?: string) {
    if (nextView === view && !customerId) return false;
    if (draftDirty && !await requestConfirmation({ title: "Abandonner la saisie ?", description: "Les changements non enregistrés seront perdus si vous changez d’écran.", confirmLabel: "Abandonner et continuer" })) return false;
    if (draftDirty) discardPendingDraft();
    setViewHistory((history) => [...history, view]);
    setNotice(null);
    if (customerId) setSelectedCustomer(customerId);
    if (nextView === "invoices") {
      setInvoiceWorkspace(customerId ? "create" : "history");
      if (customerId) setInvoiceCustomerId(customerId);
    }
    if (nextView === "payments") setPaymentWorkspace(customerId ? "create" : "journal");
    setView(nextView);
    return true;
  }

  async function openInvoiceCreation(customerId?: string) {
    if (view !== "invoices") {
      const navigated = await navigateTo("invoices", customerId);
      if (!navigated) return;
    } else if (draftDirty && !await requestConfirmation({ title: "Abandonner la saisie ?", description: "Les changements non enregistrés seront perdus si vous démarrez une nouvelle facture.", confirmLabel: "Abandonner et continuer" })) return;
    if (draftDirty) discardPendingDraft();
    if (customerId) setInvoiceCustomerId(customerId);
    setInvoiceWorkspace("create");
  }

  async function openPaymentEntry(customerId?: string) {
    if (view !== "payments") {
      const navigated = await navigateTo("payments", customerId);
      if (!navigated) return;
    } else if (draftDirty && !await requestConfirmation({ title: "Abandonner le règlement ?", description: "Les champs du règlement non enregistré seront perdus si vous commencez un autre encaissement.", confirmLabel: "Abandonner et continuer" })) return;
    if (draftDirty) discardPendingDraft();
    setSelectedCustomer(customerId ?? "");
    setPaymentWorkspace("create");
  }

  async function navigateSolarWorkspace(next: "sales" | "contracts") {
    if (next === solarWorkspace) return;
    if (draftDirty && !await requestConfirmation({ title: "Abandonner la saisie solaire ?", description: "Les champs du formulaire solaire non envoyés seront perdus si vous changez d’onglet.", confirmLabel: "Abandonner et continuer" })) return;
    if (draftDirty) discardPendingDraft();
    setSolarSaleOpen(false);
    setSolarQuote(null);
    setSolarSaleDraft(null);
    setSolarWorkspace(next);
  }

  async function navigateBack() {
    if (!viewHistory.length) return;
    if (draftDirty && !await requestConfirmation({ title: "Abandonner la saisie ?", description: "Les changements non enregistrés seront perdus si vous revenez à l’écran précédent.", confirmLabel: "Abandonner et revenir" })) return;
    if (draftDirty) discardPendingDraft();
    setView(viewHistory[viewHistory.length - 1]);
    setViewHistory((history) => history.slice(0, -1));
    setNotice(null);
  }

  const activePage = pageTitle[view];
  const previewLines = rows<Row>(preview?.lines);

  if (authChecking) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-light.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Une application connectée au service central pour gérer les abonnés, les factures et les paiements.</p></div><small>Données métier gérées par le serveur SEL</small></div><div className="auth-panel"><div className="auth-card" role="status"><p className="eyebrow">VÉRIFICATION SÉCURISÉE</p><h2>Connexion au serveur SEL</h2><p className="muted">Vérification de la session et de la disponibilité du service central…</p></div></div></section>;

  if (!session?.authenticated) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-light.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Une application connectée au service central pour gérer les abonnés, les factures et les paiements.</p></div><small>Données métier gérées par le serveur SEL</small></div><div className="auth-panel"><form className="auth-card" onSubmit={(event) => void signIn(event)}><p className="eyebrow">ACCÈS SÉCURISÉ</p><h2>Connexion à SEL Facturation</h2><p className="muted">Connectez-vous avec votre compte SEL pour accéder à l’application.</p>{authMessage && <div className="notice error" role="alert">{authMessage}</div>}<label>Identifiant SEL<input name="identity_ref" autoComplete="username" required autoFocus /></label><label>Mot de passe<input name="password" type="password" autoComplete="current-password" required /></label><Button primary type="submit" disabled={authBusy || serverReachable === false}>{authBusy ? "Connexion…" : "Se connecter"}</Button>{serverReachable === false && <Button type="button" onClick={() => void retryServer()}>Réessayer la connexion</Button>}<p className="auth-help">Une connexion Internet est requise pour ouvrir et utiliser SEL Facturation.</p></form></div></section>;

  if (serverReachable === false) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-light.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Une facturation claire,<br />sur votre poste SEL.</h1><p>Les opérations sont traitées et conservées par le serveur central SEL.</p></div><small>Connexion au serveur requise</small></div><div className="auth-panel"><div className="auth-card"><p className="eyebrow">SERVICE CENTRAL</p><h2>Connexion au serveur interrompue</h2><p className="muted">Pour protéger l’exactitude des factures et des paiements, l’application est verrouillée jusqu’au rétablissement du service en ligne.</p>{notice?.message && <div className="notice error" role="alert">{notice.message}</div>}<div className="page-actions"><Button primary onClick={() => void retryServer()}>Réessayer</Button><Button onClick={() => void signOut()}>Se déconnecter</Button></div></div></div></section>;

  if (session.require_password_change) return <section className="auth-screen"><div className="auth-brand"><img src="/assets/sel-logo-light.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">SOCIÉTÉ D’ÉNERGIE DU LUALABA</p><h1>Votre compte SEL,<br />sécurisé dès le départ.</h1><p>Modifiez votre mot de passe temporaire pour continuer.</p></div><small>Application connectée au serveur central</small></div><div className="auth-panel"><div className="auth-card"><p className="eyebrow">PREMIÈRE CONNEXION</p><h2>Changer le mot de passe</h2><PasswordChangeForm onComplete={() => { setSession({ authenticated: false }); setAuthMessage("Mot de passe modifié. Connectez-vous avec votre nouveau mot de passe."); }} /></div></div></section>;

  if (serverReady === false) return <section className="central-setup-screen"><header><img src="/assets/sel-logo-light.svg" alt="S.E.L. SASU" /><div><p className="eyebrow">CONFIGURATION CENTRALE</p><h1>Initialisation des référentiels</h1><p>Le serveur est connecté. Les opérations de facturation restent bloquées jusqu’à l’approbation des paramètres SEL.</p></div><Button onClick={() => void signOut()}>Se déconnecter</Button></header>{session.role === "ADMIN" ? <AdminConsole session={session} onSignedOut={() => void signOut()} confirmAction={requestConfirmation} draftDirty={draftDirty} /> : <Panel title="Configuration administrateur requise"><p>Le service central est joignable, mais aucun référentiel commercial approuvé n’est actif. Demandez à un administrateur SEL de terminer la configuration.</p><Button onClick={() => void signOut()}>Se déconnecter</Button></Panel>}</section>;

  return <div className="app-shell">
    <aside className="sidebar">
      <button className="brand" aria-label="Accueil SEL" onClick={() => navigateTo("dashboard")}><img src="/assets/sel-logo-ux3.svg" alt="S.E.L. SASU" /></button>
      <nav aria-label="Navigation principale">
        {navigation.map((item) => <button key={item.id} className={`nav-item${domainByView[view] === item.id ? " active" : ""}`} aria-current={domainByView[view] === item.id ? "page" : undefined} onClick={() => navigateTo(item.target)}>
          <NavIcon name={item.icon} /><span>{item.label}</span>
        </button>)}
      </nav>
      <button className={`server-nav${view === "server" ? " active" : ""}`} aria-current={view === "server" ? "page" : undefined} onClick={() => navigateTo("server")}><span className="status-dot" aria-hidden="true" />État du serveur · diagnostic</button>
      <div className="sidebar-foot"><span className="eyebrow">SERVICE CENTRAL</span><p>Connexion requise pour toute opération</p></div>
    </aside>

    <main className="content" onChange={(event) => { if ((event.target as HTMLElement).closest("form[data-preserve-draft]")) setDraftDirty(true); }}>
      <header className="topbar"><div className="topbar-context">{viewHistory.length > 0 && <Button className="back-button" onClick={navigateBack} aria-label="Revenir à l’écran précédent">← Retour</Button>}<p className="eyebrow">{activePage.context}</p></div>
        <div className="connection-status"><span className="status-dot" aria-hidden="true" />{serverStatus?.status === "CENTRAL_CONNECTED" ? "Serveur SEL · connecté" : "Serveur SEL · état inconnu"}<Button onClick={() => void signOut()}>Se déconnecter</Button></div>
      </header>
      {view !== "admin" && domainViews[domainByView[view]] && <nav className="workspace-tabs" aria-label={`Pages ${navigation.find((item) => item.id === domainByView[view])?.label ?? ""}`}>
        {domainViews[domainByView[view]]?.map((item) => <button key={item.id} className={`workspace-tab${view === item.id ? " active" : ""}`} aria-current={view === item.id ? "page" : undefined} onClick={() => navigateTo(item.id)}>{item.label}</button>)}
      </nav>}
      {view === "solar" && <nav className="workspace-tabs" aria-label="Pages solaire">
        <button className={`workspace-tab${solarWorkspace === "sales" ? " active" : ""}`} aria-current={solarWorkspace === "sales" ? "page" : undefined} onClick={() => void navigateSolarWorkspace("sales")}>Catalogue et ventes</button>
        <button className={`workspace-tab${solarWorkspace === "contracts" ? " active" : ""}`} aria-current={solarWorkspace === "contracts" ? "page" : undefined} onClick={() => void navigateSolarWorkspace("contracts")}>Contrats et échéanciers</button>
      </nav>}
      {notice && <div className={`notice${notice.error ? " error" : ""}`} role={notice.error ? "alert" : "status"}>{notice.message}<button aria-label="Fermer le message" onClick={() => setNotice(null)}>×</button></div>}
      {loading && <div className="loading-line" role="status">Chargement des données centrales…</div>}

      {view === "dashboard" && <>
        <div className="page-heading"><div><p className="eyebrow">SEL / ACCUEIL / OPÉRATIONS</p><h1>Bienvenue dans SEL Facturation</h1><p className="muted">Accédez directement aux opérations, sans parcourir plusieurs tableaux.</p></div><Button primary onClick={() => void openInvoiceCreation()}>Nouvelle facture</Button></div>
        <div className="metrics">
          <Metric label="Abonnés" value={customers.length} note="Dossiers chargés du serveur SEL" />
          <Metric label="Factures" value={invoices.length} note="Documents enregistrés au serveur" />
          <Metric label="Encaissements" value={payments.length} note="Règlements renvoyés par le serveur" />
          <Metric label="Serveur" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Connecté" : "—"} note="Valeur du serveur SEL" />
        </div>
        <div className="home-workspace">
          <Panel title="Que souhaitez-vous faire ?">
            <p className="muted">Sélectionnez une action pour retrouver directement le bon parcours.</p>
            <div className="quick-actions">
              <button onClick={() => navigateTo("customers")}><strong>Gérer les abonnés <span aria-hidden="true">›</span></strong><small>Fiches, services et compteurs</small></button>
              <button onClick={() => void openInvoiceCreation()}><strong>Émettre une facture <span aria-hidden="true">›</span></strong><small>Eau et électricité sur le même document</small></button>
              <button onClick={() => void openPaymentEntry()}><strong>Encaisser un règlement <span aria-hidden="true">›</span></strong><small>Paiement, affectation et reçu</small></button>
              <button onClick={() => navigateTo("readings")}><strong>Saisir un relevé <span aria-hidden="true">›</span></strong><small>Index, contrôle et correction</small></button>
              <button onClick={() => navigateTo("solar")}><strong>Vendre un kit solaire <span aria-hidden="true">›</span></strong><small>Catalogue, vente et échéancier</small></button>
              {session.role === "ADMIN" && <button onClick={() => navigateTo("admin")}><strong>Paramétrer SEL <span aria-hidden="true">›</span></strong><small>Utilisateurs et référentiels approuvés</small></button>}
            </div>
          </Panel>
          <Panel title="Vos repères">
            <ul className="home-guidance"><li>Une action principale par étape.</li><li>Vous pouvez revenir sans publier un brouillon.</li><li>Les écritures validées se corrigent avec traçabilité.</li><li>Le serveur est requis pour toute opération métier.</li><li>Les données affichées viennent de l’API SEL.</li></ul>
          </Panel>
        </div>
      </>}

      {view === "customers" && (selectedCustomer ? <>
        <PageHeading eyebrow="CORE / CLIENTS" title="Dossier abonné" description={`${text(selectedCustomerData?.customer_code)} · ${text(selectedCustomerData?.legal_name)}`} action={<div className="page-actions"><Button aria-label="Retour au répertoire" onClick={async () => {
          if (draftDirty && !await requestConfirmation({ title: "Abandonner les modifications ?", description: "Les changements non enregistrés dans la fiche seront perdus.", confirmLabel: "Abandonner et revenir" })) return;
          if (draftDirty) discardPendingDraft();
          setSelectedCustomer(""); setSelectedCustomerData(null); setStatementState({ customerId: "", data: null, loading: false, error: "" }); setCustomerEditOpen(false);
        }}>Retour au répertoire</Button><Button onClick={() => navigateTo("readings", selectedCustomer)}>Relevés</Button><Button onClick={() => navigateTo("payments", selectedCustomer)}>Encaisser</Button><Button primary onClick={() => navigateTo("invoices", selectedCustomer)}>Nouvelle facture</Button><Button onClick={() => { if (customerEditOpen) setDraftDirty(false); setCustomerEditOpen((open) => !open); }}>{customerEditOpen ? "Annuler la modification" : "Modifier la fiche"}</Button><Button onClick={() => void toggleCustomerArchive()}>{text(selectedCustomerData?.status).toUpperCase() === "ARCHIVED" ? "Réactiver" : "Archiver"}</Button></div>} />
        <Panel title={`Fiche abonné · ${text(selectedCustomerData?.customer_code)}`}>
          {selectedCustomerData ? customerEditOpen ? <form data-preserve-draft className="form-grid" onSubmit={(event) => void updateCustomer(event)}>
            <label>Nom / raison sociale<input name="legal_name" defaultValue={text(selectedCustomerData.legal_name, "")} required disabled={customerEditBusy} /></label>
            <label>Catégorie tarifaire<select name="customer_category" defaultValue={text(selectedCustomerData.customer_category, "")} required disabled={customerEditBusy}><option value="">Sélectionner…</option><option value="RESIDENTIAL">Résidentiel</option><option value="COMMERCIAL">Commercial</option></select></label>
            <label>Révision du dossier<input value={text(selectedCustomerData.revision)} readOnly /></label>
            <label className="span-two">Adresse<input name="address" defaultValue={text(selectedCustomerData.address, "")} disabled={customerEditBusy} /></label>
            <label>Téléphone<input name="phone" defaultValue={text(selectedCustomerData.phone, "")} disabled={customerEditBusy} /></label>
            <div className="form-actions span-two"><Button type="button" disabled={customerEditBusy} onClick={() => { setCustomerEditOpen(false); setDraftDirty(false); }}>Annuler</Button><Button primary type="submit" disabled={customerEditBusy}>{customerEditBusy ? "Vérification…" : "Vérifier les modifications"}</Button></div>
          </form> : <dl className="customer-details"><div><dt>Nom / raison sociale</dt><dd>{text(selectedCustomerData.legal_name)}</dd></div><div><dt>Code abonné</dt><dd>{text(selectedCustomerData.customer_code)}</dd></div><div><dt>Catégorie tarifaire</dt><dd>{text(selectedCustomerData.customer_category, "À compléter")}</dd></div><div><dt>Adresse</dt><dd>{text(selectedCustomerData.address)}</dd></div><div><dt>Téléphone</dt><dd>{text(selectedCustomerData.phone)}</dd></div><div><dt>Révision du dossier</dt><dd>{text(selectedCustomerData.revision)}</dd></div></dl> : <div className="empty-state">Chargement du dossier abonné…</div>}
        </Panel>
        <ServiceAccountsManager customerId={selectedCustomer} canManageMeters={["ADMIN", "SUPERVISOR", "BILLING_AGENT"].includes(text(session?.role).toUpperCase())} onConfirmReplacement={confirmMeterReplacement} confirmAction={requestConfirmation} />
        <CustomerStatement customerName={text(selectedCustomerData?.legal_name, selectedCustomer)} customerCode={text(selectedCustomerData?.customer_code)} statement={currentStatement?.data ?? null} loading={currentStatement?.loading ?? Boolean(selectedCustomer)} error={currentStatement?.error ?? ""} />
      </> : <>
      <PageHeading eyebrow="CORE / CLIENTS" title="Répertoire des abonnés" description="Rechercher un dossier et consulter sa situation." action={<Button primary onClick={() => { if (customerCreateOpen) setDraftDirty(false); setCustomerCreateOpen((open) => !open); }}>{customerCreateOpen ? "Fermer le formulaire" : "+ Ajouter un abonné"}</Button>} />
        <div className="metrics customer-metrics">
          <Metric label="Abonnés enregistrés" value={customers.length} note="Dossiers gérés par le serveur SEL" />
          <Metric label="Factures" value={invoices.length} note="Documents enregistrés au serveur" />
          <Metric label="Versements" value={payments.length} note="Règlements enregistrés au serveur" />
          <Metric label="Connexion" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Active" : "—"} note="Source unique : serveur SEL" />
        </div>
        {customerCreateOpen && <Panel title="Créer un abonné"><form data-preserve-draft className="form-grid customer-create-form" onSubmit={(event) => void createCustomer(event)}>
            <label>Code abonné<input name="customer_code" required disabled={customerCreateBusy} /></label><label>Nom / raison sociale<input name="legal_name" required disabled={customerCreateBusy} /></label>
            <label>Catégorie tarifaire<select name="customer_category" required defaultValue="" disabled={customerCreateBusy}><option value="">Sélectionner…</option><option value="RESIDENTIAL">Résidentiel</option><option value="COMMERCIAL">Commercial</option></select></label>
            <label>Adresse<input name="address" disabled={customerCreateBusy} /></label><label>Téléphone<input name="phone" disabled={customerCreateBusy} /></label>
            <p className="help span-two">Les informations pourront être complétées dans le dossier de l’abonné.</p>
            <div className="form-actions span-two"><Button primary type="submit" disabled={customerCreateBusy}>{customerCreateBusy ? "Vérification…" : "Vérifier la création"}</Button></div>
          </form></Panel>}
        <Panel title="Abonnés enregistrés" action={<input className="table-search" aria-label="Rechercher un abonné" placeholder="Code, nom ou téléphone" value={customerFilter} onChange={(event) => setCustomerFilter(event.target.value)} />}><Table headers={["Code", "Nom / raison sociale", "Adresse", "État", "Action"]} empty="Aucun abonné retourné par le serveur." data={filteredCustomers.map((row) => [<strong>{text(row.customer_code)}</strong>, text(row.legal_name), text(row.address), <Status value={row.status} />, <button className="text-button" onClick={() => { setSelectedCustomer(text(row.customer_id ?? row.id, "")); setSelectedCustomerData(row); setCustomerEditOpen(false); }}>Ouvrir le dossier →</button>])} /></Panel>
      </>)}

      {view === "readings" && <MeterReadingsView customers={customers} initialCustomerId={selectedCustomer} canCorrect={["ADMIN", "SUPERVISOR"].includes(text(session?.role).toUpperCase())} onOpenInvoices={() => navigateTo("invoices", selectedCustomer)} onOpenCustomer={(customerId) => {
        const customer = customers.find((item) => text(item.customer_id ?? item.id) === customerId);
        if (!customer) return;
        setSelectedCustomer(customerId);
        setSelectedCustomerData(customer);
        setCustomerEditOpen(false);
        navigateTo("customers", customerId);
      }} />}

      {view === "invoices" && <>
        {!preview && invoiceWorkspace === "history" && <>
          <PageHeading eyebrow="SEL / FACTURATION / FACTURES / HISTORIQUE" title="Historique des factures" description="Rechercher, ouvrir et remettre des factures aux clients." action={<Button primary onClick={() => void openInvoiceCreation()}>Nouvelle facture</Button>} />
          <div className="metrics invoice-history-metrics">
            <Metric label="Factures" value={invoices.length} note="Données du serveur SEL" />
            <Metric label="Périodes" value={new Set(invoices.map((row) => text(row.period ?? row.billing_period, "")).filter(Boolean)).size} note="Périodes facturées" />
            <Metric label="Documents" value={invoices.filter((row) => text(row.invoice_id ?? row.id, "")).length} note="Références disponibles au serveur" />
            <Metric label="Statut" value={invoices.filter((row) => text(row.status).toUpperCase() === "CENTRAL_ACCEPTED").length} note="Confirmées par le serveur SEL" />
          </div>
          <div className="invoice-history-layout">
            <Panel title="Factures enregistrées" action={<input className="table-search" aria-label="Rechercher une facture" placeholder="Référence, abonné ou période" value={invoiceFilter} onChange={(event) => setInvoiceFilter(event.target.value)} />}>
              <p className="help invoice-history-caption">Les informations et statuts affichés proviennent du serveur SEL.</p>
              <Table headers={["Facture", "Abonné", "Période", "État", "Actions"]} empty={loading ? "Chargement des factures du serveur…" : "Aucune facture retournée par le serveur."} data={filteredInvoices.map((row) => {
                const invoiceId = text(row.invoice_id ?? row.id, "");
                const invoiceStatus = text(row.status).toUpperCase();
                return [<strong>{text(row.invoice_number ?? row.invoice_id)}</strong>, text(row.customer_name ?? row.customer_id), text(row.period ?? row.billing_period), <Status value={row.status} />, <div className="page-actions"><button className="text-button" onClick={() => void openInvoice(row)}>Détail</button><Button disabled={documentBusy} onClick={() => void openDocument("invoice", invoiceId)}>Document PDF</Button>{["ADMIN", "SUPERVISOR"].includes(text(session?.role).toUpperCase()) && invoiceId && !["VOID", "CANCELLED"].includes(invoiceStatus) && <Button onClick={() => beginFinancialAction({ kind: "void-invoice", id: invoiceId, reference: text(row.invoice_number ?? invoiceId) })}>Annuler avec motif</Button>}</div>];
              })} />
            </Panel>
            <Panel title="Actions suivantes">
              <p className="help">Choisissez une étape ou ouvrez une facture existante.</p>
              <div className="invoice-next-actions">
                <Button onClick={() => void openInvoiceCreation()}>Émettre une facture combinée</Button>
                <Button disabled={!filteredInvoices.length} onClick={() => filteredInvoices[0] && void openInvoice(filteredInvoices[0])}>Détail d’une facture</Button>
              </div>
              <p className="help">La génération PDF est lancée depuis la ligne de facture et reste sur ce poste.</p>
            </Panel>
          </div>
          {selectedInvoice && <Preview data={selectedInvoice} title={`Détail de la facture · ${text(selectedInvoice.invoice_number ?? selectedInvoice.invoice_id)}`} action={<Button onClick={() => setSelectedInvoice(null)}>Fermer</Button>} />}
        </>}
        {invoiceWorkspace === "create" && <div hidden={Boolean(preview)}>
          <PageHeading eyebrow="SEL / FACTURATION / FACTURES / ÉMISSION" title="Émettre une facture combinée" description="Électricité et eau sur un seul document, même si l’eau vaut 0 m³." action={<Button onClick={() => void returnToInvoiceHistory()}>Historique des factures</Button>} />
          <div className="invoice-workspace">
            <Panel title="Paramètres de facturation">
              <form data-preserve-draft id="utility-invoice-form" key={invoiceFormKey} className="form-grid invoice-form" onSubmit={previewInvoice} onChange={() => { setPreview(null); setInvoiceDraft(null); }}>
                <label>Abonné<select name="customer_id" required value={invoiceCustomerId} onChange={(event) => setInvoiceCustomerId(event.target.value)}><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
                <label>Période de facturation<input name="billing_period" type="month" required value={invoiceBillingPeriod} onChange={(event) => setInvoiceBillingPeriod(event.target.value)} /></label>
                <label>Électricité consommée (kWh)<input name="electricity_quantity" type="number" min="0" step="0.01" defaultValue="0" required /></label>
                <label>Eau consommée (m³)<input name="water_quantity" type="number" min="0" step="0.01" value={invoiceWaterQuantity} required onChange={(event) => { setInvoiceWaterQuantity(event.target.value); if (Number(event.target.value) !== 0) setZeroConfirmed(false); }} /></label>
                <label>Barème approuvé applicable<input value={invoiceTariffRevision} readOnly placeholder={!invoiceCustomerId || !invoiceBillingPeriod ? "Sélectionnez l’abonné et la période" : "Aucun barème applicable"} /><small className="help">{!invoiceCustomerId || !invoiceBillingPeriod ? "Le système recherche le barème approuvé selon l’abonné et la période." : !invoiceCategory ? "La catégorie tarifaire de cet abonné doit être renseignée." : applicableTariffs.electricity.length === 0 ? `Aucun tarif électricité approuvé ne couvre ${invoiceBillingPeriod} pour ${invoiceCategory} en USD/kWh.` : applicableTariffs.electricity.length > 1 ? "Plusieurs tarifs d’électricité se chevauchent. Un administrateur doit corriger le catalogue." : waterTariffRequired && applicableTariffs.water.length === 0 ? `Tarif d’eau approuvé manquant pour ${invoiceBillingPeriod} · ${invoiceCategory} · USD/m³.` : waterTariffRequired && applicableTariffs.water.length > 1 ? "Plusieurs tarifs d’eau se chevauchent. Un administrateur doit corriger le catalogue." : applicableTariffs.revisions.length > 1 ? "Plusieurs références de barème sont applicables. Un administrateur doit corriger le catalogue." : applicableTariffs.revisions.length === 0 ? "Les tarifs d’électricité et d’eau doivent partager la référence d’approbation SEL." : "Référence chargée automatiquement depuis le catalogue central."}</small></label>
                <label>Date limite de paiement<input name="due_date" type="date" /></label>
                <label className="check-row invoice-zero-confirm"><input type="checkbox" checked={zeroConfirmed} onChange={(event) => setZeroConfirmed(event.target.checked)} /> Je confirme que le relevé d’eau est exactement de 0 m³.</label>
              </form>
            </Panel>
            <aside className="panel invoice-summary" aria-label="Actions suivantes">
              <p className="eyebrow">ÉMISSION</p>
              <h2>Actions suivantes</h2>
              <p className="muted">Vérifiez l’abonné, la période et les relevés avant de calculer la facture.</p>
              <div className="invoice-summary-empty"><strong>À calculer</strong><span>Le montant s’affichera après vérification par le moteur Rust.</span></div>
              <span className="status-badge pending">Aucune facture émise</span>
              <p className="help invoice-summary-help">Les tarifs, les taxes et les montants sont vérifiés par Rust avant l’émission.</p>
              <div className="invoice-summary-actions">
                <Button primary type="submit" form="utility-invoice-form" disabled={!invoiceTariffRevision}>Prévisualiser la facture</Button>
                <Button onClick={() => void returnToInvoiceHistory()}>Historique des factures</Button>
              </div>
            </aside>
          </div>
        </div>}
        {preview && <>
          <PageHeading eyebrow="SEL / FACTURATION / FACTURES / VÉRIFICATION" title="Vérifier une facture" description="Relisez les lignes, les taxes et le total avant l’émission définitive." action={<Button onClick={() => { setPreview(null); setInvoiceDraft(null); setInvoiceWorkspace("create"); }}>Corriger la saisie</Button>} />
          <div className="invoice-preview-metrics metrics">
            <Metric label="Abonné" value={text(invoiceCustomer?.customer_code, "—")} note={text(invoiceCustomer?.legal_name, "Dossier sélectionné")} />
            <Metric label="Période" value={invoiceBillingPeriod || "—"} note="Période de facturation" />
            <Metric label="Lignes" value={previewLines.length} note="Prestations calculées par Rust" />
            <Metric label="Total TTC" value={formatMoney(preview.total, preview.currency)} note="Montant calculé par le moteur Rust" />
          </div>
          <div className="invoice-verification-layout">
            <Preview data={preview} title="Aperçu de facture" />
            <Panel title="Actions suivantes">
              <p className="help">Aucune facture n’est encore émise. Vérifiez le document, puis confirmez son enregistrement central.</p>
              <span className="status-badge pending">En attente de confirmation</span>
              <div className="invoice-next-actions">
                <Button onClick={() => { setPreview(null); setInvoiceDraft(null); setInvoiceWorkspace("create"); }}>Corriger la saisie</Button>
                <Button onClick={() => void returnToInvoiceHistory()}>Retour à l’historique</Button>
                <Button primary disabled={invoiceIssueBusy} onClick={() => void confirmInvoice()}>{invoiceIssueBusy ? "Émission en cours…" : "Confirmer l’émission"}</Button>
              </div>
            </Panel>
          </div>
          <p className="help invoice-preview-status" role="status">Le calcul et les taxes viennent du moteur Rust. Cette prévisualisation n’a pas encore créé de facture.</p>
        </>}
      </>}

      {view === "payments" && <>
        {paymentWorkspace === "journal" && <>
          <PageHeading eyebrow="SEL / PAIEMENTS / ENCAISSEMENTS / JOURNAL" title="Journal de caisse" description="Historique des paiements retournés par le service central." action={<Button primary onClick={() => void openPaymentEntry()}>Enregistrer un paiement</Button>} />
          <div className="metrics payment-journal-metrics">
            <Metric label="Règlements" value={payments.length} note="Enregistrements du serveur SEL" />
            <Metric label="Confirmés" value={payments.filter((row) => text(row.status).toUpperCase() === "CENTRAL_ACCEPTED").length} note="Validés par le service central" />
            <Metric label="À examiner" value={payments.filter((row) => ["CONFLICT_REQUIRES_REVIEW", "REJECTED"].includes(text(row.status).toUpperCase())).length} note="Conflits ou refus retournés par le serveur" />
            <Metric label="Reçus disponibles" value={payments.filter((row) => text(row.payment_id ?? row.id, "")).length} note="Données structurées consultables" />
          </div>
          <div className="payment-journal-layout">
            <Panel title="Opérations de caisse"><p className="help payment-journal-caption">Les références, montants et statuts proviennent du serveur SEL.</p><Table className="payment-table" headers={["Référence", "Client", "Montant", "Mode", "État", "Document / corrections"]} empty={loading ? "Chargement des règlements du serveur…" : "Aucun règlement retourné par le serveur."} data={payments.map((row) => {
          const paymentId = text(row.payment_id ?? row.id, "");
          const paymentAmount = text(row.amount_received ?? row.received_amount, "");
          const canReverse = ["ADMIN", "CASHIER", "SUPERVISOR"].includes(text(session?.role).toUpperCase()) && paymentId && text(row.status).toUpperCase() === "CENTRAL_ACCEPTED";
          return [<div className="payment-reference"><strong title={text(row.payment_number ?? row.payment_id)}>{text(row.payment_number ?? row.payment_id)}</strong><small>{formatDateTime(row.paid_at ?? row.occurred_at)}</small></div>, customerLabel(row.customer_id, customers), formatMoney(paymentAmount, row.currency), domainLabel(row.method), <Status value={row.status} />, <div className="page-actions"><Button disabled={documentBusy} onClick={() => void openDocument("payment", paymentId)}>Reçu PDF</Button>{canReverse && <Button onClick={() => beginFinancialAction({ kind: "reverse-payment", id: paymentId, amount: paymentAmount, reference: text(row.payment_number ?? paymentId) })}>Contre-passer</Button>}</div>];
          })} /></Panel>
            <Panel title="Actions suivantes">
              <p className="help">Choisissez une étape ou ouvrez le reçu d’un règlement existant.</p>
              <div className="payment-journal-actions">
                <Button onClick={() => void openPaymentEntry()}>Enregistrer un règlement</Button>
                <Button disabled={!payments.length} onClick={() => payments[0] && void openDocument("payment", payments[0].payment_id ?? payments[0].id)}>Détail d’un règlement</Button>
                <Button onClick={() => void navigateTo("receivables")}>Suivi des créances</Button>
              </div>
              <p className="help">Le reçu PDF est créé sur ce poste à partir des données structurées centrales.</p>
            </Panel>
          </div>
        </>}
        {paymentWorkspace === "create" && <>
          <PaymentForm customers={customers} sales={sales} payments={payments} initialCustomerId={selectedCustomer} initialReceivableId={paymentPrefill?.receivableId ?? ""} initialScheduleId={paymentPrefill?.scheduleId ?? ""} initialAmount={paymentPrefill?.amount ?? ""} onPrefillConsumed={() => setPaymentPrefill(null)} onReturnToJournal={returnToPaymentJournal} onSubmit={submit} />
        </>}
      </>}

      {view === "solar" && <>
        <PageHeading eyebrow="COMMERCE / FINANCE" title={solarWorkspace === "sales" ? "Catalogue des kits solaires" : "Contrats et échéanciers"} description={solarWorkspace === "sales" ? "Consultez les produits approuvés et les ventes enregistrées par le serveur." : "Vérifiez les échéances d’une vente approuvée avant de créer son contrat."} action={solarWorkspace === "sales" ? <Button primary onClick={() => { setSolarWorkspace("sales"); setSolarProductCode(""); setSolarSaleOpen(true); }}>Créer une vente</Button> : undefined} />
        <div className="metrics solar-metrics">
          <Metric label="Kits commercialisables" value={solarProducts.length} note="Catalogue central approuvé" />
          <Metric label="Ventes enregistrées" value={sales.length} note="Historique chargé du serveur" />
          <Metric label="Contrats à échéances" value={sales.filter((sale) => Boolean(sale.installment_plan_id)).length} note="Échéanciers associés" />
          <Metric label="Connexion" value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Active" : "—"} note="Opérations centralisées" />
        </div>
        {solarWorkspace === "sales" ? <>
        {solarSaleOpen && <Panel title="Nouvelle vente solaire" action={<Button onClick={() => { setSolarSaleOpen(false); setSolarQuote(null); setSolarSaleDraft(null); setSolarProductCode(""); setDraftDirty(false); }}>Fermer</Button>}><form data-preserve-draft className="form-grid" onSubmit={quoteSolar} onChange={() => { setSolarQuote(null); setSolarSaleDraft(null); }}>
          <label className="span-two">Abonné<select name="customer_id" required defaultValue=""><option value="" disabled>Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
          <label>Kit solaire<select name="product_code" value={solarProductCode} onChange={(event) => setSolarProductCode(event.target.value)} required><option value="" disabled>Sélectionner un kit…</option>{solarProducts.map((product) => <option key={text(product.id)} value={text(product.sku)}>{text(product.sku)} — {text(product.label)} · {formatMoney(product.price_reference, product.currency)}</option>)}</select></label>
          <label>Devise du kit<input value={text(solarProducts.find((product) => text(product.sku) === solarProductCode)?.currency, PRIMARY_CURRENCY)} readOnly aria-label="Devise définie par le produit du catalogue" /></label>
          <label>Mode de règlement<select value={solarPaymentMode} onChange={(event) => setSolarPaymentMode(event.target.value as "CASH" | "INSTALLMENT")}><option value="CASH">Comptant</option><option value="INSTALLMENT">Paiement échelonné</option></select></label>
          <label className="span-two">Référence des conditions de vente approuvées<input name="terms_revision" placeholder="Référence communiquée par SEL" required /></label>
          <p className="help span-two">Le prix et la devise viennent du kit approuvé et sont revérifiés par Rust. Au comptant, l’étape suivante ouvre l’encaissement avec le montant prérempli. En paiement échelonné, l’étape suivante ouvre le contrat ; aucun échéancier n’est créé pour une vente comptant.</p>
          <div className="form-actions span-two"><Button primary type="submit">Vérifier le devis</Button></div>
        </form></Panel>}
        {solarQuote && <Panel title="Devis vérifié par Rust"><div className="quote-summary"><div><span>Produit</span><strong>{text(solarQuote.product_code)}</strong></div><div><span>Montant approuvé</span><strong>{formatMoney(solarQuote.principal, solarQuote.currency)}</strong></div><div><span>Mode</span><strong>{solarPaymentMode === "CASH" ? "Comptant" : "Paiement échelonné"}</strong></div><div><span>Conditions</span><strong>{text(solarQuote.terms_revision)}</strong></div></div><div className="form-actions"><Button primary onClick={() => void confirmSolarSale()}>Confirmer et créer la vente</Button></div></Panel>}
        <Panel title="Catalogue approuvé">
          <Table headers={["SKU", "Produit", "Type", "Prix de référence", "État commercial"]} empty="Aucun kit approuvé et applicable n’est disponible dans le catalogue SEL." data={solarProducts.map((product) => [<strong>{text(product.sku)}</strong>, text(product.label), domainLabel(text(product.product_type, "SOLAR_KIT")), formatMoney(product.price_reference, product.currency), <span className="status-badge success">Approuvé · vendable</span>])} />
        </Panel>
        <Panel title="Historique des ventes"><Table headers={["Référence", "Abonné", "Produit", "Principal", "État", "Document"]} empty="Aucune vente solaire retournée par le serveur." data={sales.map((row) => [<strong>{text(row.sale_id)}</strong>, customerLabel(row.customer_id, customers), text(row.product_code), formatMoney(row.principal, row.currency), <Status value={row.status} />, <Button disabled={documentBusy} onClick={() => void openDocument("solar", row.sale_id)}>Confirmation PDF</Button>])} /></Panel>
        </> :
        <SolarContractManager sales={sales} customers={customers} initialSaleId={solarContractSalePrefill} onInitialSaleConsumed={() => setSolarContractSalePrefill("")} onCreated={refresh} confirmAction={requestConfirmation} onRecordPayment={async (customerId, receivableId, scheduleId) => {
          if (await navigateTo("payments", customerId)) setPaymentPrefill({ receivableId, scheduleId });
        }} />
        }
      </>}

      {view === "receivables" && <>
        <PageHeading eyebrow="SEL / COMMERCE & FINANCE" title="Suivi des créances" description="Distinguer les soldes dus, échus et réglés." action={<Button onClick={() => { setSelectedReceivableId(""); setReceivableDetail(null); setReceivableDetailId(""); setReceivablesState((current) => ({ ...current, loading: true, error: "" })); centralRequest<Row>("/v1/receivables").then((data) => setReceivablesState({ data, loading: false, error: "" })).catch((error: unknown) => setReceivablesState({ data: null, loading: false, error: humanError(error) })); }}>Actualiser</Button>} />
        {receivablesState.error && <div className="notice error" role="alert">Impossible de charger les créances : {receivablesState.error}</div>}
        {receivablesState.loading && <div className="empty-state" role="status">Chargement des créances depuis le serveur SEL…</div>}
        {receivablesState.data && <>
          <div className="metrics receivable-metrics">
            <Metric label="Règlements" value={text(record(receivablesState.data.metrics).payment_count, "0")} note="Encaissements acceptés par SEL" />
            <Metric label="Créances" value={text(record(receivablesState.data.metrics).receivable_count, "0")} note="Obligations suivies" />
            <Metric label="Échéances" value={text(record(receivablesState.data.metrics).open_installment_count, "0")} note="Échéances restant à régler" />
            <Metric label="Statut" value={`${text(record(receivablesState.data.metrics).overdue_count, "0")} en retard`} note="Créances avec échéance impayée" />
          </div>
          <div className="receivable-workspace">
            <Panel title="Créances suivies"><p className="muted receivable-caption">Mise en page de référence · Soldes et états fournis par le serveur SEL.</p>
              {receivableDetailBusy && <p className="empty-state" role="status">Chargement du détail depuis le serveur…</p>}
              <Table headers={["Abonné", "Origine", "Solde", "État"]} empty={receivablesState.loading ? "Chargement des créances…" : "Aucune créance retournée par le serveur SEL."} data={rows<Row>(receivablesState.data.receivables).map((row) => {
                const id = text(row.receivable_id, "");
                return [<button type="button" className={`receivable-customer${selectedReceivableId === id ? " selected" : ""}`} aria-pressed={selectedReceivableId === id} onClick={() => { setSelectedReceivableId(id); setReceivableDetail(null); setReceivableDetailId(""); }}><strong>{text(row.customer_name)}</strong><small>{text(row.customer_code)}</small></button>, <span className="receivable-origin"><strong>{domainLabel(row.source_type)}</strong><small>{text(row.source_reference)}</small></span>, formatMoney(row.balance, row.currency), <Status value={row.status} />];
              })} />
            </Panel>
            <Panel title="Actions suivantes"><div className="receivable-next-actions">
              <p className="muted">{selectedReceivableId ? "Consultez le détail ou ouvrez un règlement pour l’abonné sélectionné." : "Choisissez une créance dans la liste pour continuer."}</p>
              <Button disabled={!selectedReceivableId || receivableDetailBusy} onClick={() => {
                const id = selectedReceivableId;
                setReceivableDetailBusy(true);
                centralRequest<Row>(`/v1/receivables/${encodeURIComponent(id)}`).then((detail) => { setReceivableDetailId(id); setReceivableDetail(detail); }).catch((error: unknown) => setNotice({ message: humanError(error), error: true })).finally(() => setReceivableDetailBusy(false));
              }}>› Détail d’une créance</Button>
              <Button primary disabled={!selectedReceivableId} onClick={async () => {
                const selected = rows<Row>(receivablesState.data?.receivables).find((item) => text(item.receivable_id) === selectedReceivableId);
                if (!selected || Number(selected.balance) <= 0) return;
                if (await navigateTo("payments", text(selected.customer_id))) setPaymentPrefill({ receivableId: selectedReceivableId, scheduleId: "" });
              }}>› Enregistrer un règlement</Button>
              {receivableDetail && receivableDetailId === selectedReceivableId && <dl className="receivable-detail"><dt>Référence</dt><dd>{text(receivableDetail.id, selectedReceivableId)}</dd><dt>Principal</dt><dd>{formatMoney(receivableDetail.principal, receivableDetail.currency)}</dd><dt>Réglé</dt><dd>{formatMoney(receivableDetail.paid, receivableDetail.currency)}</dd><dt>Crédité</dt><dd>{formatMoney(receivableDetail.credited, receivableDetail.currency)}</dd><dt>Solde</dt><dd>{formatMoney(receivableDetail.balance, receivableDetail.currency)}</dd><dt>En retard</dt><dd>{formatMoney(receivableDetail.overdue_amount, receivableDetail.currency)}</dd><dt>État</dt><dd><Status value={receivableDetail.status} /></dd></dl>}
              <p className="receivable-prototype-note">Les données financières sont lues sur le serveur SEL. Aucune opération n’est créée depuis cet écran.</p>
            </div></Panel>
          </div>
        </>}
      </>}

      {view === "server" && <>
        <PageHeading eyebrow="SEL / SERVICE CENTRAL" title="Connexion au serveur" description="Les abonnés, factures et paiements sont consultés et enregistrés directement sur le serveur SEL." action={<Button primary onClick={() => void refreshServerStatus()}>Actualiser l’état</Button>} />
        <Panel title="État de la connexion"><div className="summary-state"><Status value={serverStatus?.status === "CENTRAL_CONNECTED" ? "Connecté au serveur SEL" : serverStatus?.status === "CENTRAL_UNCONFIGURED" ? "Configuration serveur incomplète" : "Serveur indisponible"} /><p>{serverStatus?.status === "CENTRAL_CONNECTED" ? "Les opérations sont envoyées directement au serveur central." : serverStatus?.status === "CENTRAL_UNCONFIGURED" ? "Le service répond, mais la configuration de production doit être terminée avant toute opération." : "Aucune opération métier n’est possible sans connexion au serveur SEL."}</p></div></Panel>
      </>}

      {view === "admin" && <AdminConsole session={session} onSignedOut={() => void signOut()} confirmAction={requestConfirmation} draftDirty={draftDirty} />}

      {documentPreview && <div className="document-preview-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) closeDocumentPreview(); }}>
        <section className="document-preview-dialog" role="dialog" aria-modal="true" aria-label={documentPreview.title}>
          <header className="document-preview-toolbar"><div><p className="eyebrow">DOCUMENT · A4</p><h2>{documentPreview.title}</h2><p className="muted">Aperçu préparé à partir des données renvoyées par le serveur SEL.</p></div><div className="page-actions"><Button onClick={() => window.print()}>Imprimer</Button><Button primary onClick={() => void saveDocument()}>Enregistrer le PDF</Button><Button aria-label="Fermer l’aperçu" onClick={closeDocumentPreview}>Fermer</Button></div></header>
      <ClientPdfPreview blob={documentPreview.blob} />
        </section>
      </div>}
    {financialAction && <div className="action-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !financialActionBusy) setFinancialAction(null); }}>
        <section className="action-dialog" role="dialog" aria-modal="true" aria-labelledby="financial-action-title" aria-describedby="financial-action-description" onKeyDown={(event) => {
          if (event.key === "Escape" && !financialActionBusy) { event.preventDefault(); setFinancialAction(null); return; }
          if (event.key !== "Tab") return;
          const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('input:not([disabled]), textarea:not([disabled]), button:not([disabled])'));
          const first = focusable[0]; const last = focusable[focusable.length - 1];
          if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
          else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
        }}>
          <p className="eyebrow">CORRECTION FINANCIÈRE AUDITÉE</p>
          <h2 id="financial-action-title">{financialAction.kind === "void-invoice" ? "Annuler une facture" : financialAction.kind === "reverse-payment" ? "Contre-passer un règlement" : "Créer un avoir"}</h2>
          <p id="financial-action-description" className="muted">Référence : {financialAction.reference}. L’opération sera enregistrée par le serveur avec son motif ; l’écriture d’origine est conservée.</p>
          <form className="form-grid" onSubmit={(event) => void commitFinancialAction(event)}>
            {financialAction.kind !== "void-invoice" && <label>Montant à corriger<input name="amount" type="number" min="0.01" step="0.01" max={financialAction.amount || undefined} value={financialActionAmount} onChange={(event) => setFinancialActionAmount(event.target.value)} required /></label>}
            <label className="span-two">Motif obligatoire<textarea autoFocus name="reason" maxLength={500} value={financialActionReason} onChange={(event) => setFinancialActionReason(event.target.value)} required /></label>
            <div className="form-actions span-two"><Button type="button" disabled={financialActionBusy} onClick={() => setFinancialAction(null)}>Revenir sans corriger</Button><Button primary type="submit" disabled={financialActionBusy || !financialActionReason.trim() || (financialAction.kind !== "void-invoice" && !financialActionAmount)}>{financialActionBusy ? "Enregistrement…" : "Confirmer la correction"}</Button></div>
          </form>
        </section>
      </div>}
      {confirmationRequest && <div className="action-dialog-backdrop confirmation-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) finishConfirmation(false); }}>
        <section className="action-dialog confirmation-dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirmation-title" aria-describedby="confirmation-description" onKeyDown={(event) => {
          if (event.key === "Escape") { event.preventDefault(); finishConfirmation(false); return; }
          if (event.key !== "Tab") return;
          const focusable = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not([disabled])'));
          const first = focusable[0]; const last = focusable[focusable.length - 1];
          if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
          else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
        }}>
          <p className="eyebrow">{confirmationRequest.tone === "warning" ? "ACTION SUR LE DOSSIER" : "SAISIE EN COURS"}</p>
          <h2 id="confirmation-title">{confirmationRequest.title}</h2>
          <p id="confirmation-description" className="muted">{confirmationRequest.description}</p>
          <div className="form-actions">
            <Button ref={confirmationCancelRef} onClick={() => finishConfirmation(false)}>Annuler</Button>
            <Button primary className={confirmationRequest.tone === "warning" ? "button-warning" : undefined} onClick={() => finishConfirmation(true)}>{confirmationRequest.confirmLabel}</Button>
          </div>
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
      <div className="totals"><span>Sous-total <strong>{formatMoney(data.subtotal, data.currency)}</strong></span><span>TVA <strong>{formatMoney(data.tax, data.currency)}</strong></span><span>Total <strong>{formatMoney(data.total, data.currency)}</strong></span></div>
    {onConfirm && <div className="form-actions"><Button primary onClick={onConfirm}>Émettre cette facture</Button></div>}
  </Panel>;
}

function PaymentForm({ customers, sales, payments, initialCustomerId, initialReceivableId, initialScheduleId, initialAmount, onPrefillConsumed, onReturnToJournal, onSubmit }: { customers: Row[]; sales: Row[]; payments: Row[]; initialCustomerId: string; initialReceivableId: string; initialScheduleId: string; initialAmount: string; onPrefillConsumed: () => void; onReturnToJournal: () => Promise<void>; onSubmit: (form: HTMLFormElement, path: string, payload: Row, success: string) => Promise<boolean> }) {
  const formRef = useRef<HTMLFormElement>(null);
  const [customerId, setCustomerId] = useState(initialCustomerId);
  const [allocationRows, setAllocationRows] = useState([{ key: "allocation-0", receivableId: initialReceivableId, amount: "", scheduleId: initialScheduleId }]);
  const [receivables, setReceivables] = useState<Row[]>([]);
  const [receivablesLoading, setReceivablesLoading] = useState(false);
  const [receivablesError, setReceivablesError] = useState("");
  const [planSchedules, setPlanSchedules] = useState<Record<string, { schedule: Row[]; currency: string; loading: boolean; error: string }>>({});
  const [receivedAmount, setReceivedAmount] = useState(initialAmount);
  const [occurredAt, setOccurredAt] = useState(dateTimeInputValue);
  const [paymentMethod, setPaymentMethod] = useState("CASH");
  const [paymentReview, setPaymentReview] = useState<Row | null>(null);
  const [allocationStep, setAllocationStep] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);

  useEffect(() => { setCustomerId(initialCustomerId); }, [initialCustomerId]);
  useEffect(() => {
    if (initialReceivableId && initialAmount) setReceivedAmount(initialAmount);
  }, [initialReceivableId, initialAmount]);
  useEffect(() => {
    if (!initialReceivableId) return;
    setAllocationRows([{ key: `solar-prefill-${initialReceivableId}`, receivableId: initialReceivableId, amount: "", scheduleId: initialScheduleId }]);
    onPrefillConsumed();
  }, [initialReceivableId, initialScheduleId, onPrefillConsumed]);

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
  const currency = text(selectedReceivables.find(Boolean)?.currency, PRIMARY_CURRENCY);
  const duplicateAllocation = allocationRows.some((allocation, index) => allocation.receivableId && allocationRows.some((other, otherIndex) => otherIndex !== index && other.receivableId === allocation.receivableId));
  const currencyMismatch = selectedReceivables.some((item) => item && text(item.currency, PRIMARY_CURRENCY) !== currency);
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
  const openScheduleCount = Object.values(planSchedules).reduce((count, state) => count + state.schedule.filter((item) => text(item.status).toUpperCase() !== "PAID").length, 0);
  const paymentStep = paymentReview ? "confirm" : allocationStep ? "allocation" : "entry";

  useEffect(() => {
    window.scrollTo({ top: 0, behavior: "instant" });
  }, [paymentStep]);

  function updateAllocation(key: string, field: "receivableId" | "amount" | "scheduleId", value: string) {
    setAllocationRows((current) => current.map((allocation) => allocation.key === key
      ? { ...allocation, [field]: value, ...(field === "receivableId" ? { scheduleId: "" } : {}) }
      : allocation));
  }

  async function confirmPayment() {
    const form = formRef.current;
    if (!form || !paymentReview || savingRef.current) return;
    savingRef.current = true;
    setSaving(true);
    try {
      const saved = await onSubmit(form, "/v1/payments", paymentReview, "Règlement enregistré par le serveur SEL.");
      if (saved) {
        setPaymentReview(null);
        setAllocationStep(false);
        setCustomerId("");
        setAllocationRows([{ key: `allocation-${Date.now()}`, receivableId: "", amount: "", scheduleId: "" }]);
        setReceivedAmount("");
        setOccurredAt(dateTimeInputValue());
        setPaymentMethod("CASH");
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  }

  return <><PageHeading eyebrow={paymentStep === "entry" ? "SEL / PAIEMENTS / ENCAISSEMENTS / NOUVEAU RÈGLEMENT" : paymentStep === "allocation" ? "SEL / PAIEMENTS / ENCAISSEMENTS / AFFECTATION" : "SEL / PAIEMENTS / ENCAISSEMENTS / CONFIRMATION"} title={paymentStep === "entry" ? "Enregistrer un paiement" : paymentStep === "allocation" ? "Affecter un paiement aux créances" : "Confirmer l’encaissement"} description={paymentStep === "entry" ? "Distinguer l’identité du payeur, le montant et le mode d’encaissement." : paymentStep === "allocation" ? "Répartir le versement entre les obligations du même abonné." : "Dernière vérification avant l’écriture financière."} action={<Button primary={paymentStep === "allocation"} disabled={paymentStep === "allocation" && (paymentBlocked || Boolean(paymentReview))} onClick={() => paymentStep === "allocation" ? formRef.current?.requestSubmit() : paymentReview ? setPaymentReview(null) : void onReturnToJournal()}>{paymentStep === "confirm" ? "Modifier la saisie" : paymentStep === "allocation" ? "Vérifier l’affectation" : "Retour au journal"}</Button>} />
  <div className="metrics payment-entry-metrics">
    <Metric label="Règlements" value={payments.length} note="Valeur retournée par le serveur SEL" />
    <Metric label="Créances" value={receivablesLoading ? "…" : receivables.length} note={customerId ? "Disponibles pour cet abonné" : "Sélectionnez un abonné"} />
    <Metric label="Échéances" value={openScheduleCount} note="Ouvertes dans les échéanciers chargés" />
    <Metric label="Statut" value={receivablesError ? "À vérifier" : receivablesLoading ? "Chargement" : customerId ? "Prêt" : "En attente"} note="Données vérifiées par le serveur" />
  </div><div className="payment-workspace"><Panel title={paymentStep === "entry" ? "Saisie de l’encaissement" : paymentStep === "allocation" ? "Ventilation du paiement" : "Résumé des fonds reçus"}><p className="help payment-entry-caption" hidden={paymentStep === "confirm"}>Identité du payeur, date et mode d’encaissement. Le serveur SEL contrôle la créance et les montants avant toute écriture.</p><form ref={formRef} data-preserve-draft id="central-payment-form" className="form-grid" hidden={paymentStep === "confirm"} onChange={() => setPaymentReview(null)} onSubmit={(event) => {
    event.preventDefault();
    if (paymentBlocked || !event.currentTarget.reportValidity()) return;
    const form = new FormData(event.currentTarget);
    setPaymentReview({
      customer_id: form.get("customer_id"), received_amount: form.get("received_amount"), currency, method: form.get("method"), occurred_at: new Date(String(form.get("occurred_at"))).toISOString(),
      allocations: allocationRows.map((allocation) => ({ receivable_id: allocation.receivableId, amount: allocation.amount, schedule_id: allocation.scheduleId || null })),
    });
  }}>
    {paymentStep === "entry" && <>
      <label>Abonné<select name="customer_id" value={customerId} onChange={(event) => { setCustomerId(event.target.value); setAllocationRows([{ key: `allocation-${Date.now()}`, receivableId: "", amount: "", scheduleId: "" }]); }} required><option value="">Sélectionner…</option>{customers.map((row) => <option key={text(row.customer_id ?? row.id)} value={text(row.customer_id ?? row.id)}>{text(row.customer_code)} — {text(row.legal_name)}</option>)}</select></label>
      <label>Date du règlement<input name="occurred_at" type="datetime-local" value={occurredAt} onChange={(event) => setOccurredAt(event.target.value)} required /></label>
      <label>Montant reçu<input name="received_amount" type="number" min="0.01" step="0.01" value={receivedAmount} onChange={(event) => setReceivedAmount(event.target.value)} required /></label>
      <label>Devise<input value={currency} readOnly required aria-label="Devise déterminée par la créance" /></label>
      <label>Mode de règlement<select name="method" value={paymentMethod} onChange={(event) => setPaymentMethod(event.target.value)}><option value="CASH">Espèces</option><option value="BANK">Banque</option><option value="MOBILE_MONEY">Mobile Money</option><option value="OTHER">Autre</option></select></label>
    </>}
    {paymentStep === "allocation" && <>
    <input type="hidden" name="customer_id" value={customerId} />
    <input type="hidden" name="occurred_at" value={new Date(occurredAt).toISOString()} />
    <input type="hidden" name="received_amount" value={receivedAmount} />
    <input type="hidden" name="method" value={paymentMethod} />
    <div className="payment-draft-summary span-two"><span>Abonné<strong>{customerLabel(customerId, customers)}</strong></span><span>Montant reçu<strong>{formatMoney(receivedAmount, currency)}</strong></span><span>Mode<strong>{domainLabel(paymentMethod)}</strong></span></div>
    <div className="allocation-list span-two" id="payment-allocation-section"><div className="allocation-list-heading"><strong>Répartition sur les créances</strong><Button type="button" disabled={allocationRows.length >= 10 || receivables.length <= allocationRows.length} onClick={() => setAllocationRows((current) => [...current, { key: `allocation-${Date.now()}-${current.length}`, receivableId: "", amount: "", scheduleId: "" }])}>+ Ajouter une créance</Button></div>
      {allocationRows.map((allocation, index) => {
        const planId = installmentPlanByReceivable.get(allocation.receivableId);
        const planState = planId ? planSchedules[planId] : undefined;
        const openSchedule = planState?.schedule.filter((item) => text(item.status).toUpperCase() !== "PAID") ?? [];
        return <fieldset className="allocation-card" key={allocation.key}><legend>Affectation {index + 1}</legend>
      <label>Créance<select value={allocation.receivableId} onChange={(event) => updateAllocation(allocation.key, "receivableId", event.target.value)} required disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length}><option value="">{!customerId ? "Sélectionnez d’abord un abonné" : receivablesLoading ? "Chargement des créances…" : "Sélectionner une créance"}</option>{receivables.map((item) => { const id = text(item.receivable_id ?? item.id); const usedElsewhere = allocationRows.some((other) => other.key !== allocation.key && other.receivableId === id); const wrongCurrency = text(item.currency, PRIMARY_CURRENCY) !== currency; return <option key={id} value={id} disabled={usedElsewhere || wrongCurrency}>{domainLabel(item.source_type ?? item.status)} · {text(item.source_reference, id)} · {formatMoney(item.balance, item.currency)}{usedElsewhere ? " · déjà sélectionnée" : wrongCurrency ? " · devise différente" : ""}</option>; })}</select></label>
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
    <label className="span-two">Référence interne<input value="Générée par Rust à l’enregistrement" readOnly /></label>
    </>}
  </form><section className="payment-confirmation-summary" hidden={!paymentReview} aria-live="polite"><p className="help">Le serveur calculera et validera atomiquement le solde après réception.</p><dl><dt>Abonné</dt><dd>{paymentReview ? customerLabel(text(paymentReview.customer_id), customers) : "—"}</dd><dt>Total encaissé</dt><dd>{paymentReview ? formatMoney(paymentReview.received_amount, paymentReview.currency) : "—"}</dd><dt>Répartition</dt><dd>{paymentReview ? rows<Row>(paymentReview.allocations).length : 0} créance(s) sélectionnée(s)</dd><dt>Mode</dt><dd>{paymentReview ? domainLabel(paymentReview.method) : "—"}</dd><dt>Reçu</dt><dd>Généré après confirmation</dd></dl></section></Panel><Panel title="Actions suivantes"><div className="payment-summary">{paymentReview ? <section className="payment-review" aria-live="polite">
    <p className="eyebrow">DERNIÈRE VÉRIFICATION</p>
    <h2>Confirmer le règlement</h2>
    <div className="payment-summary-values">
      <div><span>Abonné</span><strong>{customerLabel(text(paymentReview.customer_id), customers)}</strong></div>
      <div><span>Montant reçu</span><strong>{formatMoney(paymentReview.received_amount, paymentReview.currency)} · {domainLabel(paymentReview.method)}</strong></div>
      <div><span>Date</span><strong>{new Date(String(paymentReview.occurred_at)).toLocaleString("fr-CD")}</strong></div>
      <div><span>Affectations</span><strong>{rows<Row>(paymentReview.allocations).length}</strong></div>
    </div>
    <ul>{rows<Row>(paymentReview.allocations).map((allocation) => {
      const receivable = receivables.find((item) => text(item.receivable_id ?? item.id) === text(allocation.receivable_id));
      return <li key={text(allocation.receivable_id)}>{domainLabel(receivable?.source_type ?? receivable?.status)} · {text(receivable?.source_reference, text(allocation.receivable_id))} — {formatMoney(allocation.amount, paymentReview.currency)}{allocation.schedule_id ? " · échéance solaire sélectionnée" : ""}</li>;
    })}</ul>
    <p className="help">Le serveur Rust recontrôlera atomiquement le montant et les soldes avant validation.</p>
    <div className="form-actions"><Button type="button" disabled={saving} onClick={() => setPaymentReview(null)}>Modifier</Button><Button primary type="button" disabled={saving} onClick={() => void confirmPayment()}>{saving ? "Enregistrement…" : "Confirmer et enregistrer"}</Button></div>
  </section> : paymentStep === "entry" ? <>
    <p className="help">Choisissez une étape ou revenez au journal.</p>
    <div className="payment-next-actions">
      <Button primary type="button" disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length} onClick={() => { if (formRef.current?.reportValidity()) setAllocationStep(true); }}>Affecter un paiement aux créances</Button>
      <Button type="button" onClick={() => void onReturnToJournal()}>Abandonner le brouillon non enregistré</Button>
      <Button type="button" onClick={() => void onReturnToJournal()}>Journal de caisse</Button>
    </div>
    <p className="help">La devise sera déterminée par les créances sélectionnées; Rust validera les soldes avant toute écriture.</p>
  </> : <>
    <p className="help">Vérifiez la répartition des fonds avant l’écriture financière.</p>
    <div className="payment-summary-values"><div><span>Abonné</span><strong>{customerId ? customerLabel(customerId, customers) : "—"}</strong></div><div><span>Créances sélectionnées</span><strong>{allocationRows.filter((allocation) => allocation.receivableId).length}</strong></div><div><span>Devise</span><strong>{currency}</strong></div></div>
    <p className="eyebrow">MONTANT REÇU</p><strong className="payment-amount">{receivedAmount ? formatMoney(receivedAmount, currency) : "—"}</strong>
    <span className={`status-badge${primaryReceivable ? " pending" : ""}`}>{primaryReceivable ? "En attente de vérification" : "Aucune créance sélectionnée"}</span>
    <div className="payment-next-actions">
      <Button primary type="submit" form="central-payment-form" disabled={paymentBlocked || Boolean(paymentReview)}>Vérifier l’affectation</Button>
      <Button type="button" onClick={() => setAllocationStep(false)}>Modifier la saisie</Button>
      <Button type="button" onClick={() => void onReturnToJournal()}>Journal de caisse</Button>
    </div>
    <p className="help">Le serveur Rust contrôle atomiquement les montants, devises et soldes.</p>
  </>}</div></Panel></div></>;
}

function ServiceAccountsManager({ customerId, canManageMeters, onConfirmReplacement, confirmAction }: { customerId: string; canManageMeters: boolean; onConfirmReplacement: (oldSerial: string, newSerial: string, reason: string) => Promise<boolean>; confirmAction: (request: ConfirmationRequest) => Promise<boolean> }) {
  const [accounts, setAccounts] = useState<Row[]>([]);
  const [metersByAccount, setMetersByAccount] = useState<Record<string, Row[]>>({});
  const [accountFormOpen, setAccountFormOpen] = useState(false);
  const [meterFormAccount, setMeterFormAccount] = useState("");
  const [replacingMeterId, setReplacingMeterId] = useState("");
  const [editingServiceAccount, setEditingServiceAccount] = useState("");
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
      const accepted = await confirmAction({
        title: "Vérifier le rattachement du service",
        description: `${domainLabel(serviceCode)} pour l’abonné ${customerId}. État : ${domainLabel(payload.connection_status)}. Contrat : ${payload.contract_ref || "non renseigné"}. Date d’activation : ${payload.activated_at || "non renseignée"}. Aucun compteur ne sera créé.`,
        confirmLabel: "Rattacher le service",
      });
      if (!accepted) return;
      await centralRequest(`/v1/customers/${encodeURIComponent(customerId)}/service-accounts`, "POST", payload);
      announceDraftSaved();
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
    const serial = String(form.get("serial") ?? "").trim();
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      const accepted = await confirmAction({
        title: "Vérifier le numéro du compteur",
        description: `Compteur ${serial}, service ${domainLabel(account.service_code)}, unité ${unit === "M3" ? "m³" : "kWh"}, abonné ${customerId}.`,
        confirmLabel: "Enregistrer le compteur",
      });
      if (!accepted) return;
      await centralRequest(`/v1/service-accounts/${encodeURIComponent(meterFormAccount)}/meters`, "POST", {
        serial, unit,
      });
      announceDraftSaved();
      setMeterFormAccount("");
      setMessage("Compteur enregistré. Les relevés pourront être saisis pour ce service.");
      await loadAccounts();
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function replaceMeter(event: FormEvent<HTMLFormElement>, meter: Row) {
    event.preventDefault();
    const meterId = text(meter.id, "");
    if (!meterId) return;
    const form = new FormData(event.currentTarget);
    const serial = String(form.get("serial") ?? "").trim();
    const reason = String(form.get("reason") ?? "").trim();
    if (!await onConfirmReplacement(text(meter.serial), serial, reason)) return;
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      await centralRequest<Row>(`/v1/meters/${encodeURIComponent(meterId)}/replace`, "POST", { serial, reason });
      announceDraftSaved();
      setReplacingMeterId("");
      setMessage(`Compteur ${text(meter.serial)} remplacé. Ses relevés et factures historiques sont conservés.`);
      await loadAccounts();
    } catch (error) { setMessage(humanError(error)); setMessageError(true); }
    finally { setSaving(false); }
  }

  async function updateServiceAccount(event: FormEvent<HTMLFormElement>, account: Row) {
    event.preventDefault();
    const accountId = text(account.id, "");
    if (!accountId) return;
    const form = new FormData(event.currentTarget);
    const payload = {
      contract_ref: String(form.get("contract_ref") ?? "").trim() || null,
      connection_status: String(form.get("connection_status") ?? "UNKNOWN"),
      activated_at: String(form.get("activated_at") ?? "") || null,
    };
    setSaving(true); setMessage(""); setMessageError(false);
    try {
      const accepted = await confirmAction({
        title: "Vérifier le raccordement",
        description: `Service ${domainLabel(account.service_code)} de l’abonné ${customerId}. État : ${domainLabel(payload.connection_status)}. Contrat : ${payload.contract_ref || "non renseigné"}. Date d’activation : ${payload.activated_at || "non renseignée"}.`,
        confirmLabel: "Enregistrer le raccordement",
      });
      if (!accepted) return;
      await centralRequest(`/v1/service-accounts/${encodeURIComponent(accountId)}`, "PATCH", payload, Number(account.revision));
      announceDraftSaved();
      setEditingServiceAccount("");
      setMessage("Raccordement mis à jour sur le serveur SEL.");
      await loadAccounts();
    } catch (error) {
      setMessage(`${humanError(error)}${String(error).includes("409") ? " Votre saisie est conservée. Comparez-la avec la dernière version du dossier avant de réessayer." : ""}`);
      setMessageError(true);
    } finally { setSaving(false); }
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
        <div className="service-account-head"><div><strong>{domainLabel(account.service_code)}</strong><span className="muted">{domainLabel(account.connection_status)}</span></div><div className="page-actions"><Button onClick={() => { announceDraftSaved(); setEditingServiceAccount((current) => current === accountId ? "" : accountId); }} disabled={saving}>{editingServiceAccount === accountId ? "Fermer la modification" : "Modifier le raccordement"}</Button>{canManageMeters && <Button onClick={() => { if (meterFormAccount === accountId) announceDraftSaved(); setMeterFormAccount((current) => current === accountId ? "" : accountId); }} disabled={saving}>{meterFormAccount === accountId ? "Annuler" : "Ajouter un compteur"}</Button>}</div></div>
        {contractRef && <p className="help">Contrat : {contractRef}</p>}
        {accountMeters.length ? <ul className="service-meter-list">{accountMeters.map((meter) => <li key={text(meter.id)}><strong>{text(meter.serial)}</strong><span>{formatUnit(meter.unit)}{meter.active === false ? " · Inactif" : " · Actif"}{canManageMeters && meter.active !== false && <Button type="button" onClick={() => setReplacingMeterId((current) => current === text(meter.id) ? "" : text(meter.id))} disabled={saving}>{replacingMeterId === text(meter.id) ? "Annuler" : "Remplacer"}</Button>}</span></li>)}</ul> : <p className="help">Aucun compteur enregistré. Le service reste utilisable pour la facturation manuelle.</p>}
        {editingServiceAccount === accountId && <form data-preserve-draft className="form-grid service-account-form" onSubmit={(event) => void updateServiceAccount(event, account)}>
          <label>État du raccordement<select name="connection_status" defaultValue={text(account.connection_status, "UNKNOWN")} disabled={saving}><option value="UNKNOWN">Inconnu</option><option value="CONNECTED">Raccordé</option><option value="NOT_CONNECTED">Non raccordé</option></select></label>
          <label>Référence du contrat<input name="contract_ref" maxLength={128} defaultValue={text(account.contract_ref, "")} disabled={saving} /></label>
          <label>Date d’activation<input name="activated_at" type="date" defaultValue={text(account.activated_at, "").slice(0, 10)} disabled={saving} /></label>
          <p className="help span-two">Modification protégée par révision du serveur. Une écriture concurrente est refusée ; les champs saisis restent à l’écran.</p>
          <div className="form-actions span-two"><Button type="button" onClick={() => { announceDraftSaved(); setEditingServiceAccount(""); }}>Annuler</Button><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer la modification"}</Button></div>
        </form>}
        {meterFormAccount === accountId && <form data-preserve-draft className="form-grid service-meter-form" onSubmit={(event) => void createMeter(event)}>
          <label>Numéro réel du compteur<input name="serial" maxLength={128} required placeholder="À saisir uniquement si le compteur existe" disabled={saving} /></label>
          <label>Unité<input value={unit === "M3" ? "m³" : "kWh"} readOnly /></label>
          <p className="help span-two">Ce champ est obligatoire uniquement pour enregistrer un compteur réel. Aucun numéro fictif ne sera créé.</p>
          <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer le compteur"}</Button></div>
        </form>}
        {replacingMeterId && accountMeters.find((meter) => text(meter.id) === replacingMeterId && meter.active !== false)?.service_account_id === accountId && <form data-preserve-draft className="form-grid service-meter-form" onSubmit={(event) => void replaceMeter(event, accountMeters.find((meter) => text(meter.id) === replacingMeterId)!)}>
          <p className="help span-two">Le remplacement est enregistré en une transaction serveur. L’ancien compteur devient inactif ; ses index, corrections et factures restent consultables.</p>
          <label>Nouveau numéro de compteur<input name="serial" maxLength={128} required placeholder="Numéro réel inscrit sur le compteur" disabled={saving} /></label>
          <label>Unité<input value={unit === "M3" ? "m³" : "kWh"} readOnly /></label>
          <label className="span-two">Motif du remplacement<textarea name="reason" maxLength={500} minLength={1} required placeholder="Panne, remplacement physique…" disabled={saving} /></label>
          <div className="form-actions span-two"><Button type="button" onClick={() => setReplacingMeterId("")}>Annuler</Button><Button primary type="submit" disabled={saving}>{saving ? "Remplacement…" : "Remplacer le compteur"}</Button></div>
        </form>}
      </article>;
    })}</div> : <div className="empty-state">Aucun service n’est encore rattaché. Vous pouvez ajouter l’eau et l’électricité sans créer de compteur.</div>}
    {accountFormOpen && availableServices.length > 0 && <form data-preserve-draft className="form-grid service-account-form" onSubmit={(event) => void createServiceAccount(event)}>
      <label>Service<select name="service_code" required defaultValue="" disabled={saving}><option value="" disabled>Sélectionner…</option>{availableServices.map((code) => <option key={code} value={code}>{domainLabel(code)}</option>)}</select></label>
      <label>État du raccordement<select name="connection_status" defaultValue="UNKNOWN" disabled={saving}><option value="UNKNOWN">Inconnu</option><option value="CONNECTED">Raccordé</option><option value="NOT_CONNECTED">Non raccordé</option></select></label>
      <label>Référence du contrat <span className="muted">(facultatif)</span><input name="contract_ref" maxLength={128} disabled={saving} /></label>
      <label>Date d’activation <span className="muted">(facultative)</span><input name="activated_at" type="date" disabled={saving} /></label>
      <p className="help span-two">Aucun numéro de compteur n’est demandé à cette étape. L’état « Inconnu » évite d’affirmer un raccordement non vérifié.</p>
      <div className="form-actions span-two"><Button type="button" onClick={() => { announceDraftSaved(); setAccountFormOpen(false); }}>Annuler</Button><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Rattacher le service"}</Button></div>
    </form>}
    {accountFormOpen && !availableServices.length && <p className="help">L’eau et l’électricité sont déjà rattachées à cet abonné.</p>}
    {message && <p role={messageError ? "alert" : "status"} className={`reading-message${messageError ? " error" : ""}`}>{message}</p>}
  </Panel>;
}

function MeterReadingsView({ customers, initialCustomerId, canCorrect, onOpenInvoices, onOpenCustomer }: { customers: Row[]; initialCustomerId: string; canCorrect: boolean; onOpenInvoices: () => void; onOpenCustomer: (customerId: string) => void }) {
  const [customerId, setCustomerId] = useState(initialCustomerId);
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

  useEffect(() => { setCustomerId(initialCustomerId); }, [initialCustomerId]);

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
      announceDraftSaved();
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
      announceDraftSaved();
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
        {meterId && <form data-preserve-draft className="form-grid reading-entry" onSubmit={(event) => void previewReading(event)} onChange={() => { setPreview(null); setPreviewInput(null); }}>
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
    {meterId && <Panel title="Historique des relevés"><Table headers={["Période", "Index précédent", "Nouvel index", "Consommation", "Source", "Correction", "Action"]} empty="Aucun relevé enregistré pour ce compteur." data={readings.map((reading) => [text(reading.period), formatQuantity(reading.previous, selectedMeter?.unit), formatQuantity(reading.current, selectedMeter?.unit), formatQuantity(reading.measured_quantity, selectedMeter?.unit), text(reading.source), reading.corrected_from_id ? text(reading.correction_reason) : "—", canCorrect && reading.id === latestReading?.id ? <button className="text-button" type="button" onClick={() => setCorrectingReading(reading)}>Corriger le dernier relevé</button> : "—"])} /></Panel>}
    {correctingReading && <Panel title={`Correction du relevé · ${text(correctingReading.period)}`}><form data-preserve-draft className="form-grid" onSubmit={(event) => void correctReading(event)}>
      <label>Nouvel index corrigé<input name="current" type="number" min="0" step="0.001" defaultValue={text(correctingReading.current, "")} required /></label>
      <label className="span-two">Motif de correction<input name="reason" maxLength={500} required /></label>
      <p className="help span-two">La correction crée une nouvelle entrée d’audit et ne supprime pas le relevé initial.</p>
      <div className="form-actions span-two"><Button primary type="submit" disabled={saving}>{saving ? "Enregistrement…" : "Enregistrer la correction"}</Button><Button type="button" onClick={() => { announceDraftSaved(); setCorrectingReading(null); }}>Annuler</Button></div>
    </form></Panel>}
  </>;
}

function SolarContractManager({ sales, customers, initialSaleId, onInitialSaleConsumed, onCreated, confirmAction, onRecordPayment }: { sales: Row[]; customers: Row[]; initialSaleId: string; onInitialSaleConsumed: () => void; onCreated: () => Promise<void>; confirmAction: (request: ConfirmationRequest) => Promise<boolean>; onRecordPayment: (customerId: string, receivableId: string, scheduleId: string) => void }) {
  const [saleId, setSaleId] = useState("");
  const [schedule, setSchedule] = useState([{ due_date: "", amount: "" }]);
  const [plan, setPlan] = useState<Row | null>(null);
  const [message, setMessage] = useState("");
  const [saving, setSaving] = useState(false);
  const availableSales = sales.filter((sale) => sale.receivable_id && !sale.installment_plan_id);

  useEffect(() => {
    if (!initialSaleId || !availableSales.some((sale) => text(sale.sale_id) === initialSaleId)) return;
    setSaleId(initialSaleId);
    onInitialSaleConsumed();
  }, [initialSaleId, availableSales, onInitialSaleConsumed]);

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
      const approvedTermsRevision = String(form.get("approved_terms_revision") ?? "").trim();
      const scheduleSummary = schedule.map((entry, index) => `Échéance ${index + 1} : ${entry.due_date}, ${entry.amount} ${sale.currency}`).join(" ; ");
      const accepted = await confirmAction({
        title: "Vérifier le contrat solaire",
        description: `Vente ${text(sale.sale_id)} · créance existante ${text(sale.receivable_id)} · principal ${formatMoney(sale.principal, sale.currency)}. Échéancier : ${scheduleSummary}. Conditions approuvées : ${approvedTermsRevision}. Aucun intérêt ni frais supplémentaire.`,
        confirmLabel: "Créer le contrat et l’échéancier",
      });
      if (!accepted) return;
      const result = await centralRequest<Row>("/v1/solar/contracts", "POST", {
        sale_id: sale.sale_id,
        receivable_id: sale.receivable_id,
        schedule: schedule.map((entry, index) => ({ sequence: index + 1, due_date: entry.due_date, amount: entry.amount })),
        currency: sale.currency,
        approved_terms_revision: approvedTermsRevision,
      });
      const planId = text(result.installment_plan_id, "");
      if (planId) setPlan(await centralRequest<Row>(`/v1/installment-plans/${encodeURIComponent(planId)}`));
      setMessage("Contrat et échéancier enregistrés par le serveur SEL.");
      announceDraftSaved();
      setSaleId(""); setSchedule([{ due_date: "", amount: "" }]);
      await onCreated();
    } catch (error) { setMessage(humanError(error)); }
    finally { setSaving(false); }
  }

  const planRows = rows<Row>(plan?.schedule);
  const nextOpenSchedule = planRows.find((item) => text(item.status).toUpperCase() !== "PAID");
  return <>
    <Panel title="Contrats et échéanciers solaires">
      {availableSales.length ? <form data-preserve-draft className="form-grid" onSubmit={(event) => void saveContract(event)}>
        <label className="span-two">Vente à financer<select value={saleId} onChange={(event) => setSaleId(event.target.value)} required disabled={saving}><option value="">Sélectionner une vente…</option>{availableSales.map((sale) => <option key={text(sale.sale_id)} value={text(sale.sale_id)}>{customerLabel(sale.customer_id, customers)} · {text(sale.sale_id)} · {formatMoney(sale.principal, sale.currency)}</option>)}</select></label>
        {schedule.map((entry, index) => <div className="schedule-entry span-two" key={index}>
          <label>Échéance {index + 1} · Date<input type="date" required value={entry.due_date} disabled={saving} onChange={(event) => setSchedule((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, due_date: event.target.value } : item))} /></label>
          <label>Montant dû<input type="number" min="0.01" step="0.01" required value={entry.amount} disabled={saving} onChange={(event) => setSchedule((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, amount: event.target.value } : item))} /></label>
          {schedule.length > 1 && <button className="text-button remove-installment" type="button" disabled={saving} onClick={() => setSchedule((current) => current.filter((_, itemIndex) => itemIndex !== index))}>Retirer</button>}
        </div>)}
        <div className="schedule-actions span-two"><button className="text-button" type="button" disabled={saving} onClick={() => setSchedule((current) => [...current, { due_date: "", amount: "" }])}>+ Ajouter une échéance</button></div>
        <label className="span-two">Référence des conditions approuvées<input name="approved_terms_revision" placeholder="Référence communiquée par SEL" required disabled={saving} /></label>
        <p className="help span-two">Le moteur vérifie les échéances et leur total par rapport à la créance. Aucun intérêt ni frais n’est ajouté.</p>
        <div className="form-actions span-two"><Button primary type="submit" disabled={saving || !saleId}>{saving ? "Vérification…" : "Vérifier le contrat"}</Button></div>
      </form> : <div className="empty-state">Aucune vente sans échéancier n’est disponible pour créer un contrat.</div>}
      {message && <p role="status" className="reading-message">{message}</p>}
      {sales.some((sale) => sale.installment_plan_id) && <div className="table-scroll"><table><thead><tr><th>Abonné</th><th>Vente</th><th>Montant</th><th>Situation</th><th>Échéancier</th></tr></thead><tbody>{sales.filter((sale) => sale.installment_plan_id).map((sale) => <tr key={text(sale.sale_id)}><td>{customerLabel(sale.customer_id, customers)}</td><td><strong>{text(sale.sale_id)}</strong></td><td>{formatMoney(sale.principal, sale.currency)}</td><td><Status value={sale.status} /></td><td><button className="text-button" onClick={() => void viewPlan(text(sale.installment_plan_id))}>Consulter</button></td></tr>)}</tbody></table></div>}
    </Panel>
    {plan && <Panel title="Échéancier consulté" action={nextOpenSchedule ? <Button primary onClick={() => onRecordPayment(text(plan.customer_id), text(plan.receivable_id), text(nextOpenSchedule.id))}>Enregistrer un versement</Button> : undefined}><div className="metrics compact-metrics"><Metric label="Principal" value={formatMoney(plan.principal, plan.currency)} note="Créance du contrat" /><Metric label="Payé" value={formatMoney(plan.paid, plan.currency)} note="Règlements affectés" /><Metric label="Restant" value={formatMoney(plan.remaining, plan.currency)} note="Solde de la créance" /><Metric label="En retard" value={formatMoney(plan.overdue_amount, plan.currency)} note="Échéances échues" /></div><Table headers={["N°", "Échéance", "Montant dû", "Payé", "Restant", "État"]} empty="Aucune échéance." data={planRows.map((item) => [text(item.sequence), text(item.due_date), formatMoney(item.amount_due, plan.currency), formatMoney(item.paid, plan.currency), formatMoney(item.remaining, plan.currency), <Status value={item.status} />])} /></Panel>}
  </>;
}

function CustomerStatement({ customerName, customerCode, statement, loading, error }: { customerName: string; customerCode: string; statement: Row | null; loading: boolean; error: string }) {
  return <Panel title={`Situation du compte · ${customerName}`}>
    {error ? <div className="notice error" role="alert">{error}</div> : loading ? <div className="empty-state" role="status">Chargement du relevé de cet abonné…</div> : statement ? <><div className="dossier-metrics metrics compact-metrics"><Metric label="Compte client" value={customerCode} note="Référence abonné" /><Metric label="Factures" value={rows(statement.invoices).length} note="Documents conservés" /><Metric label="Versements" value={rows(statement.payments).length} note="Encaissements enregistrés" /><Metric label="Créances" value={rows(statement.receivables).length} note="Obligations suivies" /></div><div className="columns"><div><p><strong>Factures</strong></p><Table headers={["Référence", "Période", "État"]} empty="Aucune facture dans le relevé." data={rows<Row>(statement.invoices).map((item) => [text(item.invoice_number ?? item.invoice_id), text(item.period ?? item.billing_period), <Status value={item.status} />])} /></div><div><p><strong>Créances</strong></p><Table headers={["Référence", "Solde", "Situation"]} empty="Aucune créance dans le relevé." data={rows<Row>(statement.receivables).map((item) => [text(item.id ?? item.receivable_id), formatMoney(item.balance, item.currency), <Status value={item.status} />])} /></div></div></> : <div className="empty-state">Aucune situation financière n’est disponible pour cet abonné.</div>}
  </Panel>;
}

type AdminCatalog = "tariffs" | "products" | "tax-profiles" | "fx-rates" | "communication-charge";
type AdminCatalogField = { name: string; label: string; type?: string; required?: boolean; step?: string; min?: string; max?: string; multiline?: boolean; placeholder?: string; defaultValue?: string; options?: { value: string; label: string }[] };

const adminCatalogDefinitions: Record<AdminCatalog, { title: string; fields: AdminCatalogField[]; columns: string[] }> = {
  tariffs: { title: "Catalogue tarifaire", fields: [
    { name: "service_code", label: "Service", options: [{ value: "ELECTRICITY", label: "Électricité" }, { value: "WATER", label: "Eau" }] },
    { name: "customer_category", label: "Catégorie d’abonné", options: [{ value: "RESIDENTIAL", label: "Résidentiel" }, { value: "COMMERCIAL", label: "Commercial" }] },
    { name: "unit", label: "Unité", options: [{ value: "KWH", label: "kWh" }, { value: "M3", label: "m³" }] },
    { name: "unit_price", label: "Prix unitaire", type: "number", step: "any" },
    { name: "currency", label: "Devise", options: CURRENCY_OPTIONS, defaultValue: PRIMARY_CURRENCY },
    { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["service_code", "customer_category", "unit_price", "status"] },
  products: { title: "Catalogue solaire", fields: [
    { name: "sku", label: "Référence produit" }, { name: "label", label: "Désignation" },
    { name: "price_reference", label: "Prix de référence", type: "number", step: "any" },
    { name: "currency", label: "Devise", options: CURRENCY_OPTIONS, defaultValue: PRIMARY_CURRENCY }, { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["sku", "label", "price_reference", "currency", "effective_from", "status"] },
  "tax-profiles": { title: "Paramètres de TVA", fields: [
    { name: "service_code", label: "Prestation concernée", options: [{ value: "ELECTRICITY", label: "Électricité" }, { value: "WATER", label: "Eau" }] },
    { name: "jurisdiction", label: "Juridiction fiscale", required: false, placeholder: "SEL par défaut" },
    { name: "base", label: "Assiette fiscale", required: false, placeholder: "SERVICE par défaut" },
    { name: "rate_percent", label: "Taux de TVA (%)", type: "number", step: "0.01", min: "0", max: "100" },
    { name: "source", label: "Document ou source fiscale" },
    { name: "change_reason", label: "Motif du changement", multiline: true },
    { name: "effective_from", label: "Date d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["service_code", "jurisdiction", "rate", "effective_from", "status"] },
  "fx-rates": { title: "Taux de change", fields: [
    { name: "from_currency", label: "Devise source", options: CURRENCY_OPTIONS, defaultValue: PRIMARY_CURRENCY }, { name: "to_currency", label: "Devise cible", options: CURRENCY_OPTIONS, defaultValue: "CDF" },
    { name: "value", label: "Taux", type: "number", step: "any" }, { name: "source", label: "Référence approuvée par SEL" },
    { name: "effective_from", label: "Début d’effet", type: "date" }, { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["from_currency", "to_currency", "value", "effective_from", "status"] },
  "communication-charge": { title: "Ligne de communication", fields: [
    { name: "amount", label: "Montant hors TVA", type: "number", step: "0.01" },
    { name: "currency", label: "Devise", options: CURRENCY_OPTIONS, defaultValue: PRIMARY_CURRENCY }, { name: "effective_from", label: "Début d’effet", type: "date" },
    { name: "effective_to", label: "Fin d’effet (facultative)", type: "date", required: false },
  ], columns: ["id", "label", "amount", "currency", "effective_from", "status"] },
};

function AdminCatalogPanel({ kind, items, busy, onCreate, onApprove, onRevoke, confirmAction, draftDirty, reviewingId, onReview, onCancelReview }: {
  kind: AdminCatalog; items: Row[]; busy: boolean;
  onCreate: (kind: AdminCatalog, values: Record<string, string>) => Promise<boolean>;
  onApprove: (kind: AdminCatalog, item: Row, approvalRef: string, supersedesId: string) => Promise<boolean>;
  onRevoke: (kind: AdminCatalog, item: Row, changeRef: string) => Promise<boolean>;
  confirmAction: (request: ConfirmationRequest) => Promise<boolean>;
  draftDirty: boolean;
  reviewingId: string;
  onReview: (id: string) => void;
  onCancelReview: () => void;
}) {
  const definition = adminCatalogDefinitions[kind];
  const addingLabel = kind === "products" ? "Ajouter un kit" : kind === "tax-profiles" ? "Ajouter un profil fiscal" : "Ajouter une version";
  const formTitle = kind === "products" ? "Nouveau kit solaire" : kind === "tax-profiles" ? "Nouveau profil de TVA" : `Nouvelle version · ${definition.title}`;
  const actionHelp = kind === "products"
    ? "Le kit devient vendable après approbation SEL. Un changement de prix crée une nouvelle version ; les ventes passées gardent leur prix. Pour changer la devise, créez une nouvelle référence produit."
    : kind === "communication-charge"
      ? "Cette ligne figure sur chaque facture eau + électricité et entre dans l’assiette de la TVA. Toute nouvelle valeur doit être approuvée par SEL."
      : kind === "tax-profiles"
        ? "Indiquez la source et la date d’effet. Le serveur conserve les versions précédentes et applique uniquement le profil approuvé correspondant à la période."
        : kind === "fx-rates"
          ? "Le taux est versionné avec sa source et sa date d’effet. USD est la devise principale ; n’enregistrez que des taux approuvés par SEL."
          : "La nouvelle version reste inactive jusqu’à son approbation SEL. Les opérations déjà émises gardent leur instantané d’origine.";
  const idField = kind === "tariffs" ? "tariff_id" : kind === "products" ? "product_id" : "id";
  const [revokingId, setRevokingId] = useState("");
  const [creating, setCreating] = useState(false);
  const [draftOpen, setDraftOpen] = useState(false);
  async function toggleDraft() {
    if (draftOpen && draftDirty && !await confirmAction({
      title: kind === "products" ? "Abandonner l’ajout du kit ?" : "Abandonner la saisie ?",
      description: "Les informations non enregistrées seront perdues.",
      confirmLabel: "Abandonner la saisie",
    })) return;
    if (draftOpen && draftDirty) announceDraftSaved();
    setDraftOpen((current) => !current);
  }
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const values = Object.fromEntries([...new FormData(form).entries()].map(([key, value]) => [key, String(value).trim()]));
    if (kind === "fx-rates") {
      const target = form.elements.namedItem("to_currency");
      if (target instanceof HTMLSelectElement) {
        const sameCurrency = values.from_currency === values.to_currency;
        target.setCustomValidity(sameCurrency ? "Choisissez une devise cible différente de la devise source." : "");
        if (sameCurrency) { target.reportValidity(); return; }
      }
    }
    setCreating(true);
    try {
      const summary = definition.fields.map((field) => `${field.label} : ${values[field.name] || "—"}`).join(". ");
      const accepted = await confirmAction({
        title: kind === "products" ? "Vérifier le nouveau kit" : `Vérifier · ${definition.title}`,
        description: `${summary}. ${actionHelp}`,
        confirmLabel: kind === "products" ? "Enregistrer le kit" : "Créer le brouillon",
      });
      if (accepted && await onCreate(kind, values)) { form.reset(); setDraftOpen(false); announceDraftSaved(); }
    } finally { setCreating(false); }
  }
  const reviewedItem = kind === "tariffs" && reviewingId ? items.find((item) => text(item.id ?? item.tariff_id, "") === reviewingId) : undefined;
  if (reviewedItem) {
    const payload = record(reviewedItem.payload);
    const tariffValue = formatMoney(reviewedItem.unit_price ?? payload.unit_price, text(reviewedItem.currency ?? payload.currency, PRIMARY_CURRENCY));
    return <Panel title="Détails soumis à validation" action={<Button onClick={async () => {
      if (draftDirty && !await confirmAction({ title: "Annuler la révision ?", description: "La référence non enregistrée sera perdue. Le brouillon du tarif reste inchangé.", confirmLabel: "Annuler la révision" })) return;
      onCancelReview();
    }}>Annuler la révision</Button>}>
      <p className="help">Informations et opérations contrôlées par le serveur SEL.</p>
      <dl className="admin-review-details">
        <div><dt>Service</dt><dd>{domainLabel(text(reviewedItem.service_code ?? payload.service_code))}</dd></div>
        <div><dt>Catégorie</dt><dd>{text(reviewedItem.customer_category ?? payload.customer_category, "—")}</dd></div>
        <div><dt>Prix unitaire</dt><dd>{tariffValue} / {formatUnit(text(reviewedItem.unit ?? payload.unit))}</dd></div>
        <div><dt>Date d’effet</dt><dd>{text(reviewedItem.effective_from ?? payload.effective_from, "—")}</dd></div>
        <div><dt>État</dt><dd><Status value={reviewedItem.status} /></dd></div>
      </dl>
      <form data-preserve-draft className="admin-approval-form" onSubmit={async (event) => {
        event.preventDefault();
        const values = new FormData(event.currentTarget);
        const accepted = await onApprove(kind, reviewedItem, String(values.get("approval_ref") ?? "").trim(), String(values.get("supersedes_id") ?? "").trim());
        if (accepted) onCancelReview();
      }}>
        <label>Numéro de révision<input name="approval_ref" aria-label="Référence de décision d’approbation SEL" placeholder="Référence SEL" required /></label>
        <label>Version remplacée (facultatif)<input name="supersedes_id" aria-label="Tarif remplacé (facultatif)" placeholder="Référence antérieure" /></label>
        <div className="form-actions span-two"><Button primary type="submit" disabled={busy || !reviewingId}>Confirmer l’approbation</Button></div>
      </form>
    </Panel>;
  }
  return <Panel title={definition.title} action={<Button primary onClick={() => void toggleDraft()}>{draftOpen ? "Fermer" : addingLabel}</Button>}>
    {draftOpen && <form data-preserve-draft className="form-grid admin-catalog-form" onSubmit={create} onChange={(event) => { const target = event.target; if (target instanceof HTMLSelectElement && ["from_currency", "to_currency"].includes(target.name)) { const destination = event.currentTarget.elements.namedItem("to_currency"); if (destination instanceof HTMLSelectElement) destination.setCustomValidity(""); } }} aria-label={formTitle}>
      {definition.fields.map((field) => <label key={field.name} className={field.multiline ? "span-two" : undefined}>{field.label}{field.options
        ? <select name={field.name} defaultValue={field.defaultValue ?? ""} required={field.required !== false} disabled={busy || creating}><option value="" disabled>Sélectionner…</option>{field.options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select>
        : field.multiline
          ? <textarea name={field.name} rows={3} maxLength={500} required={field.required !== false} disabled={busy || creating} />
          : <input name={field.name} type={field.type ?? "text"} step={field.step} min={field.min} max={field.max} placeholder={field.placeholder} required={field.required !== false} disabled={busy || creating} />}</label>)}
      <div className="form-actions span-two"><Button type="button" disabled={busy || creating} onClick={() => void toggleDraft()}>Annuler</Button><Button primary type="submit" disabled={busy || creating}>{creating ? "Vérification…" : kind === "products" ? "Enregistrer le kit" : "Créer le brouillon"}</Button></div>
      <p className="help span-two">{actionHelp}</p>
    </form>}
    <div id={kind === "tax-profiles" ? "tax-profile-version-history" : undefined}><Table headers={[...definition.columns.map((field) => ({ service_code: kind === "tax-profiles" ? "Prestation" : "Service", customer_category: "Catégorie", unit_price: "Tarif", jurisdiction: "Juridiction", rate: "TVA", effective_from: "Date d’effet", status: "État" } as Record<string, string>)[field] ?? (field === "approval_ref" ? "Référence d’approbation" : field.replace(/_/g, " "))), "Actions"]} empty={kind === "products" ? "Aucun kit n’est encore configuré. Ajoutez uniquement les références et prix communiqués par SEL." : kind === "tariffs" ? "Aucune version tarifaire n’est configurée pour cette rubrique." : "Aucune version configurée pour cette rubrique."} data={items.map((item) => {
      const id = text(item.id ?? item[idField], "");
      const status = text(item.status).toUpperCase();
      const draftFrom = text(item.effective_from, "");
      const supersededProduct = kind === "products" && status === "DRAFT" ? items
        .filter((candidate) => text(candidate.id ?? candidate.product_id, "") !== id
          && text(candidate.status).toUpperCase() === "APPROVED"
          && text(candidate.sku).trim().toUpperCase() === text(item.sku).trim().toUpperCase()
          && text(candidate.currency).trim().toUpperCase() === text(item.currency).trim().toUpperCase()
          && text(candidate.product_type, "SOLAR_KIT") === text(item.product_type, "SOLAR_KIT")
          && text(candidate.effective_from, "") < draftFrom
          && (!text(candidate.effective_to, "") || text(candidate.effective_to, "") >= draftFrom))
        .sort((left, right) => text(right.effective_from, "").localeCompare(text(left.effective_from, "")))[0] : undefined;
      const supersededProductId = text(supersededProduct?.id ?? supersededProduct?.product_id, "");
      const approval = status === "DRAFT" && kind === "tariffs" ? <Button type="button" disabled={busy || !id} onClick={() => onReview(id)}>Examiner la révision</Button> : status === "DRAFT" ? <form className="admin-approve-form" onSubmit={async (event) => {
        event.preventDefault();
        const values = new FormData(event.currentTarget);
        await onApprove(kind, item, String(values.get("approval_ref") ?? "").trim(), kind === "products" ? supersededProductId : String(values.get("supersedes_id") ?? "").trim());
      }}><input aria-label="Référence de la décision d’approbation SEL" name="approval_ref" placeholder="Réf. décision SEL" required />{kind === "products" ? <span className="help">{supersededProduct ? `Remplacera automatiquement la version du ${text(supersededProduct.effective_from)}.` : "Première version du kit, ou nouvelle période sans chevauchement."}</span> : <input aria-label={kind === "tariffs" ? "Tarif remplacé (facultatif)" : "Version remplacée (facultatif)"} name="supersedes_id" placeholder={kind === "tariffs" ? "Tarif antérieur (facultatif)" : "Version précédente (facultatif)"} />}<Button primary type="submit" disabled={busy || !id}>{kind === "products" ? "Approuver le kit" : "Approuver"}</Button></form> : status === "APPROVED" ? revokingId === id ? <form className="admin-approve-form" onSubmit={async (event) => {
        event.preventDefault();
        const form = event.currentTarget;
        const changeRef = String(new FormData(form).get("change_ref") ?? "").trim();
        if (await onRevoke(kind, item, changeRef)) setRevokingId("");
      }}><input aria-label="Motif ou référence de révocation" name="change_ref" placeholder="Motif de révocation SEL" required /><Button type="button" onClick={() => setRevokingId("")}>Annuler</Button><Button type="submit" disabled={busy || !id}>Confirmer la révocation</Button></form> : <div className="admin-approve-actions"><Button type="button" disabled={busy || !id} onClick={() => setRevokingId(id)}>Révoquer…</Button></div> : <Status value={status || "État inconnu"} />;
      return [...definition.columns.map((field) => {
        const payload = record(item.payload);
        const displayed = field === "label" ? payload.label : field === "amount" ? payload.amount : field === "currency" ? payload.currency : item[field] ?? payload[field];
        if (field === "status") return <Status value={item[field]} />;
        if (kind === "tariffs" && field === "service_code") return domainLabel(text(displayed));
        if (kind === "tariffs" && field === "unit_price") return <div className="admin-tariff-value"><strong>{formatMoney(item.unit_price, text(item.currency, PRIMARY_CURRENCY))}</strong><span className="help">{formatUnit(text(item.unit))} · depuis {text(item.effective_from)}</span></div>;
        if (kind === "tax-profiles" && field === "rate") return `${(Number(displayed) * 100).toLocaleString("fr-FR", { maximumFractionDigits: 4 })} %`;
        if (kind === "tax-profiles" && field === "service_code") return domainLabel(text(displayed));
        return text(displayed);
      }), kind === "communication-charge" && status === "APPROVED" ? <span className="help">Version protégée — remplacez-la par un nouveau brouillon approuvé.</span> : approval];
    })} /></div>
  </Panel>;
}

function AdminConsole({ session, onSignedOut, confirmAction, draftDirty }: { session: CentralSessionInfo; onSignedOut: () => void; confirmAction: (request: ConfirmationRequest) => Promise<boolean>; draftDirty: boolean }) {
  const [users, setUsers] = useState<Row[]>([]);
  const [tariffs, setTariffs] = useState<Row[]>([]);
  const [products, setProducts] = useState<Row[]>([]);
  const [taxProfiles, setTaxProfiles] = useState<Row[]>([]);
  const [fxRates, setFxRates] = useState<Row[]>([]);
  const [communicationCharges, setCommunicationCharges] = useState<Row[]>([]);
  const [auditEvents, setAuditEvents] = useState<Row[]>([]);
  const [configurationReadiness, setConfigurationReadiness] = useState<{ configured: boolean; effective_on: string; requirements: { key: string; label: string; ready: boolean; approved_current_count: number }[] } | null>(null);
  const [roleDrafts, setRoleDrafts] = useState<Record<string, string>>({});
  const [passwordTarget, setPasswordTarget] = useState("");
  const [adminArea, setAdminArea] = useState<"users" | "catalogs" | "audit">("users");
  const [catalogKind, setCatalogKind] = useState<AdminCatalog>("tariffs");
  const [reviewingTariffId, setReviewingTariffId] = useState("");
  const [adminUserView, setAdminUserView] = useState<"directory" | "create" | "security">("directory");
  const [selectedAdminUserId, setSelectedAdminUserId] = useState("");
  const [adminChangeReason, setAdminChangeReason] = useState("");
  const [userSearch, setUserSearch] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(false);
  const [catalogBusy, setCatalogBusy] = useState(false);
  const [userMutationBusy, setUserMutationBusy] = useState(false);
  const authenticated = useRef(false);

  async function navigateAdminArea(next: "users" | "catalogs" | "audit", openCreate = false) {
    if (draftDirty && !await confirmAction({
      title: "Abandonner les modifications ?",
      description: "Les saisies non enregistrées dans cette rubrique d’administration seront perdues.",
      confirmLabel: "Abandonner et continuer",
    })) return;
    if (draftDirty) {
      announceDraftSaved();
      setRoleDrafts({});
      setPasswordTarget("");
    }
    setAdminArea(next);
    setAdminUserView(next === "users" && openCreate ? "create" : "directory");
    setPasswordTarget("");
  }

  async function navigateAdminSection(section: "users" | "audit" | AdminCatalog) {
    const nextArea = section === "users" || section === "audit" ? section : "catalogs";
    if (nextArea === adminArea && (nextArea !== "catalogs" || section === catalogKind)) return;
    if (draftDirty && !await confirmAction({
      title: "Abandonner les modifications ?",
      description: "Les informations non enregistrées dans cette rubrique d’administration seront perdues.",
      confirmLabel: "Abandonner et continuer",
    })) return;
    if (draftDirty) announceDraftSaved();
    setRoleDrafts({});
    setPasswordTarget("");
    setReviewingTariffId("");
    setAdminUserView("directory");
    setMessage(""); setError(false);
    if (nextArea === "catalogs") setCatalogKind(section as AdminCatalog);
    setAdminArea(nextArea);
  }

  function clearAdminSession() {
    authenticated.current = false;
    setUsers([]); setTariffs([]); setProducts([]); setTaxProfiles([]); setFxRates([]); setCommunicationCharges([]); setAuditEvents([]);
    setConfigurationReadiness(null);
  }

  const loadData = useCallback(async () => {
    setLoading(true);
    const paths = [
      "/v1/admin/users",
      "/v1/admin/catalog/tariffs", "/v1/admin/catalog/products",
      "/v1/admin/catalog/tax-profiles", "/v1/admin/catalog/fx-rates", "/v1/admin/catalog/communication-charge", "/v1/admin/audit-events",
      "/v1/admin/configuration/readiness",
    ];
    const results = await Promise.allSettled(paths.map((path) => centralAdminRequest<unknown>(path)));
    if (!authenticated.current) return;
    const setters = [setUsers, setTariffs, setProducts, setTaxProfiles, setFxRates, setCommunicationCharges, setAuditEvents] as const;
    results.slice(0, setters.length).forEach((result, index) => {
      if (result.status === "fulfilled") setters[index](rows<Row>(result.value));
    });
    const readinessResult = results[7];
    if (readinessResult?.status === "fulfilled") {
      setConfigurationReadiness(readinessResult.value as { configured: boolean; effective_on: string; requirements: { key: string; label: string; ready: boolean; approved_current_count: number }[] });
    }
    const labels = ["utilisateurs", "tarifs", "kits solaires", "profils TVA", "taux de change", "ligne de communication", "journal d’audit", "diagnostic de configuration"];
    const failed = results.flatMap((result, index) => result.status === "rejected" ? [labels[index]] : []);
    if (failed.length) {
      const current = await centralSessionStatus().catch(() => ({ authenticated: false }));
      if (!current.authenticated) {
        clearAdminSession();
        setMessage("La session centrale a expiré. Reconnectez-vous pour continuer."); setError(true); setLoading(false);
        return;
      }
    }
    setMessage(failed.length ? `Chargement partiel : ${failed.join(", ")}. Les données déjà chargées sont conservées. Vérifiez la connexion puis actualisez.` : "");
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
    const identityRef = String(values.get("identity_ref") ?? "").trim();
    const role = String(values.get("role") ?? "BILLING_AGENT");
    const password = String(values.get("password") ?? "");
    const passwordConfirm = String(values.get("password_confirm") ?? "");
    const requirePasswordChange = values.get("require_password_change") === "on";
    if (password !== passwordConfirm) {
      setMessage("Les deux mots de passe ne correspondent pas."); setError(true);
      (form.elements.namedItem("password_confirm") as HTMLInputElement | null)?.focus();
      return;
    }
    const roleLabels: Record<string, string> = { ADMIN: "Administrateur", BILLING_AGENT: "Agent facturation", CASHIER: "Caissier", SUPERVISOR: "Superviseur", AUDITOR_READONLY: "Auditeur lecture seule" };
    setUserMutationBusy(true);
    try {
      const accepted = await confirmAction({
        title: "Vérifier la création du compte",
        description: `Identifiant : ${identityRef}. Rôle : ${roleLabels[role] ?? role}. ${requirePasswordChange ? "Le changement du mot de passe sera exigé à la première connexion." : "Le compte pourra se connecter avec le mot de passe défini."} Le mot de passe reste masqué.`,
        confirmLabel: "Créer le compte",
      });
      if (!accepted) return;
      await centralAdminRequest("/v1/admin/users", "POST", {
        identity_ref: identityRef,
        role,
        password,
        require_password_change: requirePasswordChange,
      });
      form.reset(); announceDraftSaved(); setAdminUserView("directory"); setMessage("Utilisateur créé par le serveur central."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setUserMutationBusy(false); }
  }

  async function toggleUser(user: Row) {
    const id = text(user.id ?? user.user_id, "");
    const status = text(user.status).toUpperCase() === "ACTIVE" ? "REVOKED" : "ACTIVE";
    if (!id) return;
    const identity = text(user.identity_ref, "ce compte SEL");
    const reason = adminChangeReason.trim();
    if (!reason) {
      setMessage("Saisissez le motif du changement avant de modifier les accès."); setError(true);
      return;
    }
    const disabling = status === "REVOKED";
    if (!await confirmAction({
      title: disabling ? "Désactiver ce compte ?" : "Réactiver ce compte ?",
      description: disabling
        ? `Le compte ${identity} ne pourra plus ouvrir de session. Ses sessions seront révoquées ; les opérations déjà enregistrées restent conservées. Motif : ${reason}`
        : `Le compte ${identity} pourra à nouveau ouvrir une session avec son rôle actuel. Motif : ${reason}`,
      confirmLabel: disabling ? "Désactiver le compte" : "Réactiver le compte",
      tone: disabling ? "warning" : "neutral",
    })) return;
    setUserMutationBusy(true);
    try {
      await centralAdminRequest(`/v1/admin/users/${encodeURIComponent(id)}`, "PATCH", { status, reason });
      setAdminChangeReason("");
      setMessage(status === "ACTIVE" ? "Compte réactivé." : "Compte révoqué. Les sessions associées sont révoquées côté serveur.");
      setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setUserMutationBusy(false); }
  }

  async function updateUserRole(user: Row) {
    const id = text(user.id ?? user.user_id, "");
    const role = roleDrafts[id];
    if (!id || !role || role === text(user.role)) return;
    const reason = adminChangeReason.trim();
    if (!reason) {
      setMessage("Saisissez le motif du changement avant de modifier les accès."); setError(true);
      return;
    }
    const roleLabels: Record<string, string> = { ADMIN: "Administrateur", BILLING_AGENT: "Agent facturation", CASHIER: "Caissier", SUPERVISOR: "Superviseur", AUDITOR_READONLY: "Auditeur lecture seule" };
    if (!await confirmAction({
      title: "Vérifier le changement de rôle",
      description: `Compte : ${text(user.identity_ref)}. Rôle actuel : ${roleLabels[text(user.role)] ?? text(user.role)}. Nouveau rôle : ${roleLabels[role] ?? role}. Motif : ${reason}. Les autorisations effectives seront appliquées par le serveur SEL.`,
      confirmLabel: "Enregistrer le rôle",
      tone: "warning",
    })) return;
    setUserMutationBusy(true);
    try {
      await centralAdminRequest(`/v1/admin/users/${encodeURIComponent(id)}`, "PATCH", { role, reason });
      setAdminChangeReason("");
      setRoleDrafts((current) => { const next = { ...current }; delete next[id]; return next; });
      announceDraftSaved(); setMessage("Rôle modifié par le serveur central."); setError(false); await loadData();
    } catch (reason) { setMessage(humanError(reason)); setError(true); }
    finally { setUserMutationBusy(false); }
  }

  async function resetUserPassword(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!passwordTarget) return;
    const form = event.currentTarget;
    const password = String(new FormData(form).get("password") ?? "");
    const user = users.find((item) => text(item.id ?? item.user_id) === passwordTarget);
    const identity = text(user?.identity_ref, "ce compte SEL");
    if (!await confirmAction({
      title: "Vérifier la réinitialisation du mot de passe",
      description: `Compte : ${identity}. Le nouveau mot de passe reste masqué. Les sessions existantes de ce compte seront révoquées par le serveur.`,
      confirmLabel: "Confirmer le changement",
      tone: "warning",
    })) return;
    setUserMutationBusy(true);
    try {
      await centralAdminRequest(`/v1/users/${encodeURIComponent(passwordTarget)}/password`, "POST", { password });
      form.reset(); announceDraftSaved(); setPasswordTarget(""); setMessage("Mot de passe mis à jour. Les sessions existantes sont révoquées côté serveur."); setError(false); await loadData();
    } catch (reason) { form.reset(); announceDraftSaved(); setMessage(humanError(reason)); setError(true); }
    finally { setUserMutationBusy(false); }
  }

  async function createCatalogDraft(kind: AdminCatalog, values: Record<string, string>): Promise<boolean> {
    setCatalogBusy(true);
    try {
      const body: Record<string, unknown> = { ...values, effective_to: values.effective_to || null };
      if (kind === "tax-profiles") {
        const percent = Number(values.rate_percent);
        if (!Number.isFinite(percent) || percent < 0 || percent > 100) throw new Error("Saisissez un taux de TVA compris entre 0 et 100 %.");
        body.rate = String(percent / 100);
        delete body.rate_percent;
      }
      await centralAdminRequest(`/v1/admin/catalog/${kind}`, "POST", body);
      setMessage(kind === "products" ? "Kit enregistré comme brouillon. Il pourra être vendu après son approbation par SEL." : `Brouillon ${adminCatalogDefinitions[kind].title.toLocaleLowerCase("fr")} transmis au serveur. Il reste inactif jusqu’à son approbation SEL.`); setError(false); await loadData();
      return true;
    } catch (reason) { setMessage(humanError(reason)); setError(true); return false; }
    finally { setCatalogBusy(false); }
  }

  async function approveCatalogDraft(kind: AdminCatalog, item: Row, approvalRef: string, supersedesId: string): Promise<boolean> {
    const id = text(item.id ?? item[kind === "tariffs" ? "tariff_id" : "product_id"], "");
    if (!id || !approvalRef) return false;
    setCatalogBusy(true);
    try {
      const label = text(item.label ?? record(item.payload).label ?? item.service_code ?? item.sku ?? id);
      const versionNote = kind === "products" ? supersedesId ? "Cette version remplace la version précédente approuvée du même kit." : "Cette version n’en remplace aucune." : "Une version approuvée peut être utilisée par les opérations à venir.";
      const accepted = await confirmAction({
        title: "Vérifier l’approbation du référentiel",
        description: `${adminCatalogDefinitions[kind].title} : ${label}. Référence d’approbation SEL : ${approvalRef}. ${versionNote}`,
        confirmLabel: "Approuver cette version",
        tone: "warning",
      });
      if (!accepted) return false;
      const supersedesKey = kind === "tariffs" ? "supersedes_tariff_id" : kind === "products" ? "supersedes_product_id" : "supersedes_id";
      const body = { approval_ref: approvalRef, [supersedesKey]: supersedesId || null };
      await centralAdminRequest(`/v1/admin/catalog/${kind}/${encodeURIComponent(id)}/approve`, "POST", body);
      setMessage("Référentiel approuvé par le serveur central avec la référence fournie."); setError(false); await loadData();
      return true;
    } catch (reason) { setMessage(humanError(reason)); setError(true); return false; }
    finally { setCatalogBusy(false); }
  }

  async function revokeCatalogItem(kind: AdminCatalog, item: Row, changeRef: string): Promise<boolean> {
    const id = text(item.id ?? item[kind === "tariffs" ? "tariff_id" : "product_id"], "");
    if (!id || !changeRef) return false;
    setCatalogBusy(true);
    try {
      const label = text(item.label ?? record(item.payload).label ?? item.service_code ?? item.sku ?? id);
      const accepted = await confirmAction({
        title: "Vérifier la révocation du référentiel",
        description: `${adminCatalogDefinitions[kind].title} : ${label}. Motif : ${changeRef}. La révocation sera historisée par le serveur SEL.`,
        confirmLabel: "Révoquer cette version",
        tone: "warning",
      });
      if (!accepted) return false;
      await centralAdminRequest(`/v1/admin/catalog/${kind}/${encodeURIComponent(id)}/revoke`, "POST", { change_ref: changeRef });
      setMessage("Révocation enregistrée par le serveur central avec la référence fournie."); setError(false); await loadData();
      return true;
    } catch (reason) { setMessage(humanError(reason)); setError(true); return false; }
    finally { setCatalogBusy(false); }
  }

  if (session.role !== "ADMIN") return <Panel title="Accès administrateur requis">
    <p className="muted">La session {text(session.role)} est authentifiée, mais ne possède pas le rôle ADMIN nécessaire à ces fonctions.</p>
    <div className="form-actions"><Button onClick={onSignedOut}>Fermer la session</Button></div>
  </Panel>;

  const filteredUsers = users.filter((user) => {
    const query = userSearch.trim().toLocaleLowerCase("fr");
    return !query || [user.identity_ref, user.role, user.status].some((value) => text(value, "").toLocaleLowerCase("fr").includes(query));
  });
  const catalogEntries = [...tariffs, ...products, ...taxProfiles, ...fxRates, ...communicationCharges];
  const approvedCatalogCount = catalogEntries.filter((item) => text(item.status).toUpperCase() === "APPROVED").length;
  const catalogItemsByKind: Record<AdminCatalog, Row[]> = { tariffs, products, "tax-profiles": taxProfiles, "fx-rates": fxRates, "communication-charge": communicationCharges };
  const selectedCatalogItems = catalogItemsByKind[catalogKind];
  const selectedCatalogItemCount = catalogKind === "products" ? new Set(selectedCatalogItems.map((item) => text(item.sku).trim().toUpperCase()).filter(Boolean)).size : selectedCatalogItems.length;
  const selectedApprovedCount = selectedCatalogItems.filter((item) => text(item.status).toUpperCase() === "APPROVED").length;
  const catalogBreadcrumb = catalogKind === "products" ? "ADMIN / KITS SOLAIRES" : catalogKind === "tariffs" ? "ADMIN / TARIFS" : catalogKind === "tax-profiles" ? "ADMIN / TVA" : catalogKind === "fx-rates" ? "ADMIN / TAUX DE CHANGE" : "ADMIN / LIGNE DE COMMUNICATION";
  const catalogDescription = catalogKind === "products"
    ? "Gérez les kits vendus par SEL. Le prix est figé dans chaque vente ; le paiement comptant ou échelonné se choisit au moment de la vente."
    : catalogKind === "tariffs"
      ? "Tarifs d’eau et d’électricité versionnés selon la catégorie d’abonné et leur période d’application."
      : catalogKind === "tax-profiles"
        ? "Profils de TVA versionnés et appliqués selon leur prestation et leur période d’effet."
        : catalogKind === "fx-rates"
          ? `Taux de change approuvés et datés. Devise principale : ${PRIMARY_CURRENCY}.`
          : "Montant de la ligne de communication appliqué aux factures et versionné dans le temps.";
  const selectedAdminUser = users.find((user) => text(user.id ?? user.user_id, "") === selectedAdminUserId) ?? null;

  return <>
    {message && <div className={`notice${error ? " error" : ""}`} role={error ? "alert" : "status"}>{message}</div>}
    {configurationReadiness && !configurationReadiness.configured && <section className="admin-readiness" aria-labelledby="admin-readiness-title">
      <div><p className="eyebrow">BLOCAGE DES OPÉRATIONS COMMERCIALES</p><h2 id="admin-readiness-title">Référentiels à finaliser</h2><p className="help">Le diagnostic reprend les critères du serveur SEL, à la date du {new Date(`${configurationReadiness.effective_on}T00:00:00`).toLocaleDateString("fr-FR")}. Une configuration manquante ou en doublon bloque la facturation.</p></div>
      <ul>{configurationReadiness.requirements.filter((item) => !item.ready).map((item) => <li key={item.key}><span>{item.label}</span><strong>{item.approved_current_count === 0 ? "À approuver" : `${item.approved_current_count} versions actives — corriger le doublon`}</strong></li>)}</ul>
      <Button primary onClick={() => void navigateAdminSection("tariffs")}>Ouvrir les référentiels</Button>
    </section>}
    <div className="admin-console-layout">
      <nav className="admin-sidebar" aria-label="Navigation Administration">
        <p className="eyebrow">ADMINISTRATION</p>
        <button className={adminArea === "users" ? "active" : ""} aria-current={adminArea === "users" ? "page" : undefined} onClick={() => void navigateAdminSection("users")}>Utilisateurs et accès</button>
        <span className="admin-sidebar-label">RÉFÉRENTIELS SEL</span>
        {([
          ["tariffs", "Tarifs eau et électricité"],
          ["tax-profiles", "TVA"],
          ["fx-rates", "Taux de change"],
          ["products", "Kits solaires"],
          ["communication-charge", "Ligne de communication"],
        ] as [AdminCatalog, string][]).map(([kind, label]) => <button key={kind} className={adminArea === "catalogs" && catalogKind === kind ? "active" : ""} aria-current={adminArea === "catalogs" && catalogKind === kind ? "page" : undefined} onClick={() => void navigateAdminSection(kind)}>{label}</button>)}
        <button className={adminArea === "audit" ? "active" : ""} aria-current={adminArea === "audit" ? "page" : undefined} onClick={() => void navigateAdminSection("audit")}>Journal d’audit</button>
        <Button className="admin-sidebar-refresh" disabled={loading} onClick={() => void loadData()}>{loading ? "Actualisation…" : "Actualiser les données"}</Button>
      </nav>
      <div className="admin-console-content">
    {loading && <p className="admin-loading" role="status">Actualisation des données depuis le serveur SEL…</p>}
    {adminArea === "users" && <>
      {adminUserView === "create" ? <>
        <header className="admin-page-heading"><div><p className="eyebrow">ADMIN / UTILISATEURS / CRÉATION</p><h2>Créer un utilisateur SEL</h2><p className="muted">Création isolée et attribution explicite d’un rôle.</p></div><Button onClick={() => void navigateAdminArea("users")}>Revenir au répertoire</Button></header>
        <div className="admin-metrics">
          <Metric label="Comptes" value={String(users.length)} note="Valeur du serveur SEL" />
          <Metric label="Référentiels approuvés" value={String(approvedCatalogCount)} note="Valeur du serveur SEL" />
          <Metric label="Comptes actifs" value={String(users.filter((user) => text(user.status).toUpperCase() === "ACTIVE").length)} note="Valeur du serveur SEL" />
          <Metric label="Contrôle" value="RBAC" note="Autorisations du serveur SEL" />
        </div>
        <div className="admin-directory-layout admin-create-layout">
          <Panel title="Identité et autorisations">
            <p className="help">Informations et opérations contrôlées par le serveur SEL.</p>
            <form id="admin-create-user-form" data-preserve-draft className="form-grid admin-create-user" onSubmit={(event) => void createUser(event)}>
              <label>Identifiant SEL<input name="identity_ref" autoComplete="off" required disabled={userMutationBusy} /></label>
              <label>Rôle initial<select name="role" defaultValue="BILLING_AGENT" disabled={userMutationBusy}><option value="BILLING_AGENT">Agent facturation</option><option value="CASHIER">Caissier</option><option value="SUPERVISOR">Superviseur</option><option value="AUDITOR_READONLY">Auditeur lecture seule</option><option value="ADMIN">Administrateur</option></select></label>
              <label>Mot de passe temporaire<input name="password" type="password" minLength={8} autoComplete="new-password" required disabled={userMutationBusy} /></label>
              <label>Confirmer le mot de passe<input name="password_confirm" type="password" minLength={8} autoComplete="new-password" required disabled={userMutationBusy} /></label>
              <label><span>Connexion initiale</span><span><input name="require_password_change" type="checkbox" defaultChecked disabled={userMutationBusy} /> Exiger le changement du mot de passe à la première connexion</span></label>
              <p className="help">Le compte est activé par le serveur SEL. Son état initial n’est pas configurable par l’API actuelle.</p>
              <div className="form-actions"><Button primary type="submit" disabled={userMutationBusy}>{userMutationBusy ? "Vérification…" : "Vérifier la création"}</Button></div>
            </form>
          </Panel>
          <Panel title="Actions suivantes"><p className="help">Vérifiez les informations saisies avant de créer le compte.</p><div className="admin-next-actions"><Button type="submit" form="admin-create-user-form">Vérifier le nouvel utilisateur</Button><Button onClick={() => void navigateAdminArea("users")}>Utilisateurs et autorisations</Button></div><p className="help">Le compte n’est enregistré qu’après confirmation explicite.</p></Panel>
        </div>
      </> : adminUserView === "security" && selectedAdminUser ? <>
        <header className="admin-page-heading"><div><p className="eyebrow">ADMIN / UTILISATEURS / SÉCURITÉ</p><h2>Modifier les accès d’un compte</h2><p className="muted">Changer un rôle, suspendre un compte ou réinitialiser son mot de passe.</p></div><Button onClick={() => void navigateAdminArea("users")}>Retour utilisateurs</Button></header>
        <div className="admin-metrics">
          <Metric label="Comptes" value={String(users.length)} note="Valeur du serveur SEL" />
          <Metric label="Référentiels approuvés" value={String(approvedCatalogCount)} note="Valeur du serveur SEL" />
          <Metric label="Comptes actifs" value={String(users.filter((user) => text(user.status).toUpperCase() === "ACTIVE").length)} note="Valeur du serveur SEL" />
          <Metric label="Contrôle" value="RBAC" note="Autorisations du serveur SEL" />
        </div>
        <div className="admin-directory-layout">
          <Panel title="Révision du compte">
            <p className="help">Les modifications et la révocation des sessions sont validées par le serveur SEL.</p>
            <dl className="customer-details admin-user-summary"><div><dt>Utilisateur sélectionné</dt><dd>{text(selectedAdminUser.identity_ref)}</dd></div><div><dt>Rôle actuel</dt><dd>{text(selectedAdminUser.role)}</dd></div><div><dt>État</dt><dd><Status value={selectedAdminUser.status} /></dd></div></dl>
            {text(selectedAdminUser.id ?? selectedAdminUser.user_id, "") === text(session.user_id, "") && <p className="notice">Votre propre rôle et statut ne peuvent pas être modifiés depuis cette session.</p>}
            <form data-preserve-draft className="admin-role-edit" onSubmit={(event) => { event.preventDefault(); void updateUserRole(selectedAdminUser); }}>
              <label>Nouveau rôle<select aria-label={`Nouveau rôle de ${text(selectedAdminUser.identity_ref)}`} value={roleDrafts[selectedAdminUserId] ?? text(selectedAdminUser.role)} onChange={(event) => setRoleDrafts((current) => ({ ...current, [selectedAdminUserId]: event.target.value }))} disabled={userMutationBusy || text(selectedAdminUser.id ?? selectedAdminUser.user_id, "") === text(session.user_id, "")}><option value="ADMIN">Administrateur</option><option value="BILLING_AGENT">Agent facturation</option><option value="CASHIER">Caissier</option><option value="SUPERVISOR">Superviseur</option><option value="AUDITOR_READONLY">Auditeur lecture seule</option></select></label>
              <label className="span-two">Motif du changement<textarea name="admin-change-reason" maxLength={500} minLength={1} required value={adminChangeReason} onChange={(event) => setAdminChangeReason(event.target.value)} disabled={userMutationBusy} placeholder="Expliquer la raison de la modification des accès…" /></label>
              <Button primary type="submit" disabled={userMutationBusy || !adminChangeReason.trim() || text(selectedAdminUser.id ?? selectedAdminUser.user_id, "") === text(session.user_id, "") || !roleDrafts[selectedAdminUserId] || roleDrafts[selectedAdminUserId] === text(selectedAdminUser.role)}>{userMutationBusy ? "Vérification…" : "Vérifier la modification"}</Button>
            </form>
            <div className="admin-security-actions"><Button disabled={userMutationBusy || !adminChangeReason.trim() || text(selectedAdminUser.id ?? selectedAdminUser.user_id, "") === text(session.user_id, "")} onClick={() => void toggleUser(selectedAdminUser)}>{text(selectedAdminUser.status).toUpperCase() === "ACTIVE" ? "Désactiver ce compte" : "Réactiver ce compte"}</Button><Button disabled={userMutationBusy} onClick={() => setPasswordTarget(selectedAdminUserId)}>Réinitialiser le mot de passe</Button></div>
            {passwordTarget === selectedAdminUserId && <form data-preserve-draft className="form-grid admin-password-reset" onSubmit={(event) => void resetUserPassword(event)}><p className="help span-two">Le nouveau mot de passe reste masqué. Les sessions existantes seront révoquées côté serveur.</p><label>Nouveau mot de passe<input type="password" name="password" minLength={8} autoComplete="new-password" required disabled={userMutationBusy} /></label><div className="page-actions"><Button type="button" disabled={userMutationBusy} onClick={() => { announceDraftSaved(); setPasswordTarget(""); }}>Annuler</Button><Button primary type="submit" disabled={userMutationBusy}>{userMutationBusy ? "Enregistrement…" : "Confirmer le changement"}</Button></div></form>}
          </Panel>
          <Panel title="Actions suivantes"><div className="admin-next-actions"><Button onClick={() => void navigateAdminArea("users")}>Utilisateurs et autorisations</Button><Button onClick={() => void navigateAdminArea("audit")}>Journal d’audit</Button></div><p className="help">Aucun compte ni historique n’est supprimé physiquement.</p></Panel>
        </div>
      </> : <>
        <header className="admin-page-heading"><div><p className="eyebrow">ADMIN / RÉPERTOIRE</p><h2>Utilisateurs et autorisations</h2><p className="muted">{loading ? "Actualisation des données du serveur SEL…" : "Administrer les comptes sans mélanger les autres référentiels."}</p></div><Button primary onClick={() => void navigateAdminArea("users", true)}>Créer un utilisateur</Button></header>
        <div className="admin-metrics">
          <Metric label="Comptes" value={String(users.length)} note="Valeur du serveur SEL" />
          <Metric label="Référentiels approuvés" value={String(approvedCatalogCount)} note="Valeur du serveur SEL" />
          <Metric label="Comptes actifs" value={String(users.filter((user) => text(user.status).toUpperCase() === "ACTIVE").length)} note="Valeur du serveur SEL" />
          <Metric label="Contrôle" value="RBAC" note="Autorisations du serveur SEL" />
        </div>
        <div className="admin-directory-layout">
          <Panel title="Répertoire des utilisateurs">
            <p className="help">Recherchez un compte, puis ouvrez son dossier de sécurité.</p>
            <input className="table-search" aria-label="Rechercher un compte" placeholder="Nom de compte, rôle ou état" value={userSearch} onChange={(event) => setUserSearch(event.target.value)} />
            <Table headers={["Identifiant", "Rôle", "État", "Action"]} empty={users.length ? "Aucun compte ne correspond à cette recherche." : "Aucun utilisateur retourné par le serveur."} data={filteredUsers.map((user) => {
              const id = text(user.id ?? user.user_id, "");
              const roleLabels: Record<string, string> = { ADMIN: "Administrateur", BILLING_AGENT: "Agent facturation", CASHIER: "Caissier", SUPERVISOR: "Superviseur", AUDITOR_READONLY: "Auditeur lecture seule" };
              return [text(user.identity_ref), roleLabels[text(user.role)] ?? text(user.role), <Status value={user.status} />, <Button aria-label={`Voir les accès de ${text(user.identity_ref)}`} disabled={userMutationBusy} onClick={() => { setMessage(""); setError(false); setAdminChangeReason(""); setSelectedAdminUserId(id); setAdminUserView("security"); }}>Voir</Button>];
            })} />
          </Panel>
          <Panel title="Actions suivantes"><p className="help">Choisissez une étape ou revenez au répertoire.</p><div className="admin-next-actions"><Button onClick={() => void navigateAdminArea("users", true)}>Créer un utilisateur SEL</Button><Button onClick={() => void navigateAdminArea("catalogs")}>Tarifs et référentiels</Button><Button onClick={() => void navigateAdminArea("audit")}>Journal d’audit</Button></div><p className="help">Les opérations ne sont enregistrées qu’après confirmation.</p></Panel>
        </div>
      </>}
    </>}
    {adminArea === "catalogs" && <>
      <header className="admin-page-heading"><div><p className="eyebrow">{reviewingTariffId ? "ADMIN / TARIFS / APPROBATION" : `${catalogBreadcrumb} / CATALOGUE`}</p><h2>{reviewingTariffId ? "Valider une révision tarifaire" : catalogKind === "tariffs" ? "Tarifs eau et électricité" : adminCatalogDefinitions[catalogKind].title}</h2><p className="muted">{reviewingTariffId ? "Vérifiez les montants et la période d’application avant activation." : catalogDescription}</p></div><span className="service-state">● Service central · données SEL</span></header>
      <div className="admin-metrics">
        <Metric label={catalogKind === "products" ? "Kits référencés" : "Versions"} value={String(selectedCatalogItemCount)} note={catalogKind === "products" ? `${selectedCatalogItems.length} version(s) dans le catalogue` : "Dans cette rubrique"} />
        <Metric label="Approuvées" value={String(selectedApprovedCount)} note={catalogKind === "products" ? "Versions de kit approuvées" : "Dans cette rubrique"} />
        <Metric label="Devise principale" value={PRIMARY_CURRENCY} note="Devise par défaut" />
        <Metric label="Contrôle" value="RBAC" note="Autorisations du serveur SEL" />
      </div>
      <div className="admin-directory-layout admin-reference-layout">
        <AdminCatalogPanel key={catalogKind} kind={catalogKind} items={catalogKind === "tariffs" ? tariffs : catalogKind === "products" ? products : catalogKind === "tax-profiles" ? taxProfiles : catalogKind === "fx-rates" ? fxRates : communicationCharges} busy={catalogBusy} onCreate={createCatalogDraft} onApprove={approveCatalogDraft} onRevoke={revokeCatalogItem} confirmAction={confirmAction} draftDirty={draftDirty} reviewingId={reviewingTariffId} onReview={setReviewingTariffId} onCancelReview={() => setReviewingTariffId("")} />
        <Panel title="Actions suivantes"><p className="help">{catalogKind === "tax-profiles" && !reviewingTariffId ? "Consultez les versions fiscales ou revenez aux tarifs." : reviewingTariffId ? "Choisissez une étape ou revenez au catalogue." : "Choisissez le référentiel à consulter ou à administrer."}</p><div className="admin-next-actions admin-reference-actions">
          {reviewingTariffId && <><Button onClick={() => setReviewingTariffId("")}>Quitter la revue tarifaire</Button><Button onClick={() => void navigateAdminSection("audit")}>Journal d’audit</Button></>}
          {catalogKind === "tax-profiles" && !reviewingTariffId && <><Button onClick={() => document.getElementById("tax-profile-version-history")?.scrollIntoView({ behavior: "smooth", block: "start" })}>Consulter les versions</Button><Button onClick={() => void navigateAdminSection("tariffs")}>Revenir aux tarifs</Button></>}
        </div><p className="help">{catalogKind === "products" ? "Seuls les kits approuvés sont proposés à la vente. Les ventes passées conservent leur prix enregistré." : "Les nouvelles versions restent inactives jusqu’à leur approbation par un administrateur SEL."}</p></Panel>
      </div>
    </>}
    {adminArea === "audit" && <Panel title="Journal d’audit récent"><p className="help">Lecture seule — 100 événements les plus récents associés à cette organisation. Les métadonnées sensibles ne sont pas affichées.</p><Table headers={["Action", "Agent", "Élément concerné", "Date"]} empty="Aucun événement d’audit disponible pour cette organisation." data={auditEvents.map((event) => [<strong>{text(event.event_type)}</strong>, text(event.actor_identity ?? event.actor_id ?? event.device_id), text(event.subject_id), formatDateTime(event.occurred_at)])} /></Panel>}
      </div>
    </div>
  </>;
}
