import type { Content, TDocumentDefinitions, TCreatedPdf } from "pdfmake/interfaces";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import selLogoSvg from "./assets/sel-logo-light.svg?raw";

type Data = Record<string, unknown>;
type PdfDocument = { blob: Blob; pdf: TCreatedPdf; filename: string; title: string };

let pdfRuntime: typeof import("pdfmake") | null = null;

async function loadPdfRuntime(): Promise<typeof import("pdfmake")> {
  if (pdfRuntime) return pdfRuntime;
  const [runtimeModule, fontModule] = await Promise.all([
    import("pdfmake"),
    import("pdfmake/build/vfs_fonts"),
  ]);
  const runtime = (runtimeModule.default ?? runtimeModule) as typeof import("pdfmake");
  runtime.addVirtualFileSystem(fontModule.default);
  pdfRuntime = runtime;
  return runtime;
}

const value = (row: Data | null | undefined, key: string, fallback = "—"): string => {
  const result = row?.[key];
  return result === null || result === undefined || result === "" ? fallback : String(result);
};

const object = (value: unknown): Data => value && typeof value === "object" && !Array.isArray(value) ? value as Data : {};
const list = (value: unknown): Data[] => Array.isArray(value) ? value.map(object) : [];

function money(amount: unknown, currency: unknown): string {
  const raw = String(amount ?? "");
  if (!/^-?\d+(?:\.\d+)?$/.test(raw)) return `${raw || "—"} ${String(currency ?? "")}`.trim();
  const [integer, fraction = ""] = raw.split(".");
  return `${integer.replace(/\B(?=(\d{3})+(?!\d))/g, " ")},${fraction.padEnd(2, "0").slice(0, 2)} ${String(currency ?? "")}`.trim();
}

function unitPrice(amount: unknown, currency: unknown): string {
  const raw = String(amount ?? "");
  if (!/^\d+(?:\.\d+)?$/.test(raw)) return `${raw || "—"} ${String(currency ?? "")}`.trim();
  const [integer, fraction = ""] = raw.split(".");
  const decimals = fraction.replace(/0+$/, "").padEnd(2, "0");
  return `${integer.replace(/\B(?=(\d{3})+(?!\d))/g, " ")},${decimals} ${String(currency ?? "")}`.trim();
}

function serviceUnit(unit: unknown): string {
  const code = String(unit ?? "").toUpperCase();
  return code === "KWH" ? "kWh" : code === "M3" ? "m³" : code === "FORFAIT" ? "forfait" : String(unit ?? "—");
}

function dateLabel(valueToFormat: unknown): string {
  const raw = String(valueToFormat ?? "");
  const match = raw.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (!match) return raw || "—";
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  return new Intl.DateTimeFormat("fr-FR", { day: "2-digit", month: "long", year: "numeric", timeZone: "UTC" }).format(date);
}

function periodLabel(valueToFormat: unknown): string {
  const raw = String(valueToFormat ?? "");
  const match = raw.match(/^(\d{4})-(\d{2})$/);
  if (!match) return raw || "—";
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, 1));
  return new Intl.DateTimeFormat("fr-FR", { month: "long", year: "numeric", timeZone: "UTC" }).format(date);
}

function statusLabel(status: unknown, invoice = false): string {
  const value = String(status ?? "—");
  if (invoice && value === "CENTRAL_ACCEPTED") return "Facture émise";
  const labels: Record<string, string> = {
    CENTRAL_ACCEPTED: "Confirmé par le serveur",
    PAYMENT_RECORDED: "Paiement enregistré",
    ISSUED: "Émis",
    UPCOMING: "À venir",
    OVERDUE: "En retard",
    PAID: "Payé",
    PARTIAL: "Partiellement payé",
    VOID: "Annulé",
  };
  return labels[value] ?? value;
}

function baseStyles(): TDocumentDefinitions["styles"] {
  return {
    brand: { color: "#152c4b", bold: true, fontSize: 9 },
    eyebrow: { color: "#075bbb", bold: true, fontSize: 7.5, characterSpacing: 0.45 },
    documentTitle: { color: "#012a5e", bold: true, fontSize: 21 },
    section: { color: "#012a5e", bold: true, fontSize: 8.5 },
    muted: { color: "#5b6f87", fontSize: 8 },
    body: { color: "#152c4b", fontSize: 9 },
    total: { color: "#ffffff", bold: true, fontSize: 20 },
  };
}

function footer(currentPage: number, pageCount: number, _pageSize?: unknown, issuerLine = "Document généré par SEL à partir des données du serveur"): Content {
  return {
    columns: [
      { text: issuerLine, style: "muted" },
      { text: `Page ${currentPage} / ${pageCount}`, alignment: "right", style: "muted" },
    ],
    margin: [45, 0, 45, 13],
  } as Content;
}

function statusColor(status: unknown): string {
  return String(status) === "VOID" ? "#a33131" : "#137a51";
}

function statusFill(status: unknown): string {
  return String(status) === "VOID" ? "#fce9e9" : "#e1f5ec";
}

function documentHeader(organizationName: string, reference: string, title: string, subtitle: string, status: string, invoice = false): Content[] {
  const rightColumn: Content[] = [
    { text: organizationName, style: "brand" },
    { text: "Document de facturation SEL", style: "muted", margin: [0, 6, 0, 0] },
  ];
  return [
    {
      columns: [
        { width: 132, svg: selLogoSvg, fit: [132, 73] },
        { width: "*", stack: rightColumn, margin: [0, 20, 5, 0] },
      ],
      columnGap: 8,
    } as Content,
    ...titleAndReference(title, subtitle, reference, status, invoice),
  ];
}

function separator(marginTop = 13, marginBottom = 13): Content {
  return { canvas: [{ type: "line", x1: 0, y1: 0, x2: 505, y2: 0, lineWidth: 0.6, lineColor: "#d9e3f0" }], margin: [0, marginTop, 0, marginBottom] };
}

function detailsTable(headers: string[], rows: Content[][]): Content {
  const widths = ["*", 65, 65, 65, 65, 65];
  return {
    table: {
      headerRows: 1,
      widths,
      body: [headers.map((text, index) => ({ text, style: "eyebrow", alignment: index === 0 ? "left" : "center", margin: [0, 2, 0, 2] })), ...rows],
    },
    layout: {
      fillColor: (row: number) => row === 0 ? "#ecf2f9" : undefined,
      hLineWidth: (index: number) => index === 0 || index === 1 ? 0 : 0.55,
      hLineColor: () => "#d9e3f0",
      vLineWidth: () => 0,
      paddingLeft: (index: number) => index === 0 ? 9 : 4,
      paddingRight: () => 4,
      paddingTop: (row: number) => row === 0 ? 9 : 15,
      paddingBottom: (row: number) => row === 0 ? 9 : 15,
    },
  } as Content;
}

function receiptAllocationsTable(allocations: Data[], currency: unknown): Content {
  const body: Content[][] = [
    ["OBLIGATION / DOCUMENT", "TOTAL", "DÉJÀ RÉGLÉ", "CE VERSEMENT", "RESTE DÛ"].map((text, index) => ({
      text, style: "eyebrow", alignment: index === 0 ? "left" : "right", margin: [0, 2, 0, 2],
    } as Content)),
    ...allocations.map((allocation) => {
      const kind = String(allocation.source_type ?? "").toUpperCase();
      const kindLabel = kind === "SALE" ? "Vente solaire" : kind === "INVOICE" ? "Facture de services" : "Créance";
      const description = value(allocation, "source_description", "");
      const reference = value(allocation, "source_reference", "Référence interne");
      const amount = money(allocation.amount, allocation.currency ?? currency);
      const principal = allocation.principal === undefined || allocation.principal === null ? "—" : money(allocation.principal, allocation.currency ?? currency);
      const paidBefore = allocation.paid_before === undefined || allocation.paid_before === null ? "—" : money(allocation.paid_before, allocation.currency ?? currency);
      const creditedBefore = allocation.credited_before === undefined || allocation.credited_before === null ? "" : String(allocation.credited_before);
      const taxSnapshot = object(allocation.tax_snapshot);
      const solarVat = String(taxSnapshot.service_code ?? "").toUpperCase() === "SOLAR_KIT"
        && taxSnapshot.rate !== undefined && taxSnapshot.amount !== undefined
        ? `TVA incluse au total · ${new Intl.NumberFormat("fr-FR", { style: "percent", maximumFractionDigits: 2 }).format(Number(taxSnapshot.rate))} · ${money(taxSnapshot.amount, allocation.currency ?? currency)}`
        : "";
      const schedules = list(allocation.schedule_allocations);
      const balance = allocation.balance_after === undefined || allocation.balance_after === null
        ? "À consulter au compte client"
        : money(allocation.balance_after, allocation.currency ?? currency);
      return [
        { stack: [
          { text: `${kindLabel}${description ? ` · ${description}` : ""}`, style: "body", bold: true },
          { text: reference, style: "muted", margin: [0, 2, 0, 0] },
          ...(solarVat ? [{ text: solarVat, style: "muted", margin: [0, 3, 0, 0] } as Content] : []),
          ...(creditedBefore && !["0", "0.0", "0.00"].includes(creditedBefore) ? [{ text: `Avoirs déjà appliqués : ${money(creditedBefore, allocation.currency ?? currency)}`, style: "muted", margin: [0, 3, 0, 0] } as Content] : []),
          ...schedules.map((schedule) => ({ text: `Échéance ${value(schedule, "sequence")} · ${dateLabel(schedule.due_date)} · ${money(schedule.amount, allocation.currency ?? currency)}`, style: "muted", margin: [0, 3, 0, 0] } as Content)),
        ] },
        { text: principal, style: "body", alignment: "right" },
        { text: paidBefore, style: "body", alignment: "right" },
        { text: amount, style: "body", alignment: "right", bold: true },
        { text: balance, style: "body", alignment: "right" },
      ] as Content[];
    }),
  ];
  return {
    table: { headerRows: 1, widths: ["*", 64, 72, 79, 92], body },
    layout: {
      fillColor: (row: number) => row === 0 ? "#ecf2f9" : undefined,
      hLineWidth: (index: number) => index === 0 || index === 1 ? 0 : 0.55,
      hLineColor: () => "#d9e3f0",
      vLineWidth: () => 0,
      paddingLeft: (index: number) => index === 0 ? 9 : 5,
      paddingRight: () => 5,
      paddingTop: (row: number) => row === 0 ? 9 : 12,
      paddingBottom: (row: number) => row === 0 ? 9 : 12,
    },
  } as Content;
}

function receiptSummary(received: string, allocated?: string, unallocated?: string): Content[] {
  return [
    separator(15, 14),
    { text: "RÉCAPITULATIF DU RÈGLEMENT", style: "muted", alignment: "right", bold: true, margin: [0, 0, 0, 8] } as Content,
    ...(allocated !== undefined ? [{ columns: [
      { text: "Affecté aux créances", style: "body" }, { text: allocated, style: "body", alignment: "right", bold: true },
    ], margin: [245, 0, 0, 6] } as Content] : []),
    ...(unallocated !== undefined ? [{ columns: [
      { text: "Reste non affecté", style: "body" }, { text: unallocated, style: "body", alignment: "right", bold: true },
    ], margin: [245, 0, 0, 8] } as Content] : []),
    { table: { widths: ["*"], body: [[{ stack: [
      { text: "MONTANT REÇU", color: "#a5c4cb", bold: true, fontSize: 7, margin: [0, 0, 0, 8] },
      { text: received, style: "total", alignment: "right" },
    ], fillColor: "#012a5e", margin: [11, 10, 11, 10] }]] },
    layout: { hLineWidth: () => 0, vLineWidth: () => 0 }, margin: [240, 7, 0, 0] } as Content,
  ];
}

function recipientAndDocument(customer: Data, documentRows: string[]): Content {
  const left: Content[] = [
    { text: "DESTINATAIRE", style: "eyebrow", margin: [0, 0, 0, 8] },
    { text: value(customer, "legal_name"), color: "#012a5e", bold: true, fontSize: 13, margin: [0, 0, 0, 6] },
    { text: `Code abonné : ${value(customer, "customer_code")}`, style: "muted", margin: [0, 0, 0, 5] },
    ...(value(customer, "address", "") ? [{ text: value(customer, "address", ""), style: "muted", margin: [0, 0, 0, 5] } as Content] : []),
    ...(value(customer, "phone", "") ? [{ text: value(customer, "phone", ""), style: "muted" } as Content] : []),
  ];
  const right: Content[] = [
    { text: "INFORMATIONS DU DOCUMENT", style: "eyebrow", margin: [0, 0, 0, 9] },
    ...documentRows.map((row) => ({ text: row, style: "body", margin: [0, 0, 0, 7] } as Content)),
  ];
  return {
    columns: [{ width: "*", stack: left }, { width: "*", stack: right }],
    columnGap: 20,
    margin: [0, 0, 0, 1],
  } as Content;
}

function titleAndReference(title: string, subtitle: string, reference: string, status: string, invoice = false): Content[] {
  return [
    { text: title.toLocaleUpperCase("fr"), style: "documentTitle", margin: [0, 18, 0, 3] },
    { text: subtitle, color: "#5b6f87", fontSize: 11, margin: [0, 0, 0, 18] },
    {
      columns: [
        { width: 150, table: { widths: ["*"], body: [[{ text: statusLabel(status, invoice).toLocaleUpperCase("fr"), color: statusColor(status), bold: true, fontSize: 7, margin: [7, 7, 5, 7] }]] }, layout: { fillColor: () => statusFill(status), hLineWidth: () => 0, vLineWidth: () => 0 } },
        { width: "*", stack: [
          { text: "RÉFÉRENCE DOCUMENT", style: "muted", alignment: "right", fontSize: 7 },
          { text: reference, color: "#152c4b", bold: true, fontSize: 10, alignment: "right", margin: [0, 3, 0, 0] },
        ] },
      ],
      columnGap: 8,
      margin: [0, 0, 0, 15],
    } as Content,
  ];
}

function financialSummary(label: string, subtotalLabel: string, subtotal: string, taxesLabel: string, taxes: string, total: string): Content[] {
  return [
    separator(15, 17),
    { columns: [{ text: "SYNTHÈSE FINANCIÈRE", style: "muted", alignment: "right", bold: true }, { text: "", width: 0 }], margin: [0, 0, 0, 8] } as Content,
    { columns: [{ text: subtotalLabel, style: "body" }, { text: subtotal, style: "body", alignment: "right", bold: true }] , margin: [245, 0, 0, 6] } as Content,
    ...(taxesLabel ? [{ columns: [{ text: taxesLabel, style: "body" }, { text: taxes, style: "body", alignment: "right", bold: true }], margin: [245, 0, 0, 8] } as Content] : []),
    {
      table: { widths: ["*"], body: [[{ stack: [
        { text: label.toLocaleUpperCase("fr"), color: "#a5c4cb", bold: true, fontSize: 7, margin: [0, 0, 0, 8] },
        { text: total, style: "total", alignment: "right" },
      ], fillColor: "#012a5e", margin: [11, 10, 11, 10] }]] },
      layout: { hLineWidth: () => 0, vLineWidth: () => 0 },
      margin: [240, 7, 0, 0],
    } as Content,
  ];
}

function qrFooter(reference: string, explanatoryLine: string, invoice = false): Content[] {
  return [
    separator(17, 11),
    {
      columns: [
        { width: 54, qr: reference, fit: 48, alignment: "center", margin: [2, 1, 2, 1] },
        { width: "*", stack: [
          { text: invoice ? "QR d’identification" : "Référence interne SEL", color: "#152c4b", bold: true, fontSize: 8, margin: [0, 3, 0, 5] },
          { text: invoice ? "Données internes de la facture · sans authentification fiscale" : "QR interne de repérage · sans authentification fiscale", style: "muted", fontSize: 7 },
          ...(!invoice && explanatoryLine ? [{ text: explanatoryLine, style: "muted", fontSize: 7, margin: [0, 5, 0, 0] } as Content] : []),
        ], margin: [10, 0, 0, 0] },
      ],
    } as Content,
  ];
}

function invoiceDefinition(data: Data): TDocumentDefinitions {
  const invoice = object(data.invoice);
  const identity = object(data.identity_snapshot);
  const customer = identity;
  const lines = list(invoice.lines);
  const body: Content[][] = [];
  for (const line of lines) {
    const service = value(line, "service_code");
    body.push([
      { text: service === "ELECTRICITY" ? "Électricité" : service === "WATER" ? "Eau" : service === "COMMUNICATION" ? "Ligne de communication" : service, style: "body" },
      { text: `${value(line, "quantity")} ${serviceUnit(line.unit)}`, style: "body", alignment: "center" },
      { text: line.unit_price === null ? "—" : unitPrice(line.unit_price, invoice.currency), style: "body", alignment: "center" },
      { text: money(line.subtotal, invoice.currency), style: "body", alignment: "center" },
      { text: money(line.tax, invoice.currency), style: "body", alignment: "center" },
      { text: money(line.total, invoice.currency), style: "body", alignment: "center", bold: true },
    ]);
  }
  const reference = value(invoice, "invoice_number", value(invoice, "invoice_id"));
  const internalQr = value(data, "qr_payload", value(data, "qr_internal_reference", value(data, "document_reference")));
  const issueTitle = String(invoice.status) === "VOID" ? "FACTURE ANNULÉE" : "FACTURE DE CONSOMMATION";
  const invoiceStatus = String(invoice.status ?? data.origin);
  const subtitle = "Électricité et eau · Facture combinée";
  const documentRows = [
    ...(data.issued_at ? [`Émission : ${dateLabel(data.issued_at)}`] : []),
    `Période : ${periodLabel(invoice.billing_period)}`,
    ...(invoice.due_date ? [`Échéance : ${dateLabel(invoice.due_date)}`] : []),
  ].filter((row) => !row.endsWith(": "));
  return {
    pageSize: "A4", pageMargins: [45, 26, 45, 36], defaultStyle: { font: "Roboto", fontSize: 9, color: "#152c4b" }, styles: baseStyles(),
    background: () => ({ canvas: [{ type: "rect", x: 0, y: 0, w: 595, h: 6, color: "#012a5e" }] }),
    content: [
      ...documentHeader(value(identity, "organization_name", "Société d’Énergie du Lualaba"), reference, issueTitle, subtitle, invoiceStatus, true),
      separator(17, 17),
      recipientAndDocument(customer, documentRows),
      separator(17, 17),
      { text: "DÉTAIL DES PRESTATIONS", style: "section", margin: [0, 0, 0, 13] },
      detailsTable(["DÉSIGNATION", "QUANTITÉ", "PRIX U.", "BASE", "TVA", "TOTAL"], body),
      ...financialSummary("Total de la facture", "Sous-total", money(invoice.subtotal, invoice.currency), "TVA", money(invoice.tax, invoice.currency), money(invoice.total, invoice.currency)),
      ...qrFooter(internalQr, "", true),
    ],
    footer: (currentPage, pageCount, pageSize) => footer(currentPage, pageCount, pageSize, "Facture émise par la Société d’Énergie du Lualaba, SEL"),
  };
}

function paymentDefinition(data: Data): TDocumentDefinitions {
  const payment = object(data.payment); const customer = object(data.customer);
  const allocations = list(data.allocations); const reversals = list(data.reversals);
  const reference = value(data, "document_reference");
  const amount = money(payment.amount_received, payment.currency);
  const methodLabels: Record<string, string> = { CASH: "Espèces", BANK: "Banque", MOBILE_MONEY: "Mobile Money", OTHER: "Autre" };
  const method = String(payment.method ?? "");
  const recorded = String(payment.status ?? "") === "CENTRAL_ACCEPTED";
  const amountAllocated = payment.amount_allocated === undefined ? undefined : money(payment.amount_allocated, payment.currency);
  const amountUnallocated = payment.amount_unallocated === undefined ? undefined : money(payment.amount_unallocated, payment.currency);
  const documentRows = [
    `Émission : ${dateLabel(payment.issued_at ?? payment.paid_at)}`,
    `Date du règlement : ${dateLabel(payment.paid_at)}`,
    `Mode : ${methodLabels[method] ?? value(payment, "method")}`,
  ];
  return {
    pageSize: "A4", pageMargins: [45, 26, 45, 36], defaultStyle: { font: "Roboto", fontSize: 9, color: "#152c4b" }, styles: baseStyles(),
    background: () => ({ canvas: [{ type: "rect", x: 0, y: 0, w: 595, h: 6, color: "#012a5e" }] }),
    content: [
      ...documentHeader(value(data, "organization_name", "Société d’Énergie du Lualaba"), reference, "Reçu de paiement", "Justificatif interne de règlement", recorded ? "PAYMENT_RECORDED" : String(payment.status)),
      separator(17, 17), recipientAndDocument(customer, documentRows), separator(17, 17),
      { text: "AFFECTATION AUX CRÉANCES", style: "section", margin: [0, 0, 0, 13] },
      ...(allocations.length ? [receiptAllocationsTable(allocations, payment.currency)] : [{ text: "Aucun montant n’a été affecté à une créance sur ce reçu.", style: "muted" } as Content]),
      ...(reversals.length ? [{ text: "Contre-écritures", style: "eyebrow", margin: [0, 10, 0, 5] } as Content, ...reversals.map((reversal) => ({ text: `${value(reversal, "reason")}  ·  ${money(reversal.amount, payment.currency)}`, style: "muted", margin: [0, 0, 0, 3] } as Content))] : []),
      ...receiptSummary(amount, amountAllocated, amountUnallocated),
      ...qrFooter(value(data, "qr_internal_reference"), allocations.some((allocation) => allocation.balance_after !== undefined)
        ? "Solde indiqué après ce versement, selon l’écriture enregistrée par SEL. Les échéances applicables restent celles du contrat."
        : "Les soldes et échéances applicables sont consultables dans le compte client."),
    ],
    footer: (currentPage, pageCount, pageSize) => footer(currentPage, pageCount, pageSize, "Reçu émis par la Société d’Énergie du Lualaba, SEL"),
  };
}

function solarDefinition(data: Data): TDocumentDefinitions {
  const sale = object(data.sale); const customer = object(data.customer);
  const lines = list(data.lines); const plan = object(data.installment_plan);
  const schedule = list(plan.schedule);
  const reference = value(data, "document_reference");
  const total = money(sale.principal, sale.currency);
  const documentRows = [
    `Émission : ${dateLabel(sale.sold_at)}`,
    ...(sale.contract_id ? [`Réf. contrat : ${value(sale, "contract_id")}`] : []),
    ...(schedule.length ? [`Échéances : ${schedule.length}`] : ["Mode : vente au comptant"]),
  ];
  const taxRate = lines[0]?.tax_snapshot && object(lines[0].tax_snapshot).rate !== undefined
    ? new Intl.NumberFormat("fr-FR", { style: "percent", maximumFractionDigits: 2 }).format(Number(object(lines[0].tax_snapshot).rate))
    : "TVA";
  const rows: Content[][] = lines.map((line) => [
    { text: value(line, "product_reference"), style: "body" },
    { text: value(line, "quantity"), style: "body", alignment: "center" },
    { text: money(line.unit_price, sale.currency), style: "body", alignment: "center" },
    { text: money(line.subtotal, sale.currency), style: "body", alignment: "center" },
    { text: money(line.tax_amount, sale.currency), style: "body", alignment: "center" },
    { text: money(line.total, sale.currency), style: "body", alignment: "center", bold: true },
  ]);
  return {
    pageSize: "A4", pageMargins: [45, 26, 45, 36], defaultStyle: { font: "Roboto", fontSize: 9, color: "#152c4b" }, styles: baseStyles(), footer,
    background: () => ({ canvas: [{ type: "rect", x: 0, y: 0, w: 595, h: 6, color: "#012a5e" }] }),
    content: [
      ...documentHeader(value(data, "organization_name", "Société d’Énergie du Lualaba"), reference, "Facture de vente", "Équipement solaire · Vente", String(data.origin ?? sale.status)),
      separator(17, 17), recipientAndDocument(customer, documentRows), separator(17, 17),
      { text: "DÉTAIL DES PRESTATIONS", style: "section", margin: [0, 0, 0, 13] },
      detailsTable(["DÉSIGNATION", "QUANTITÉ", "PRIX U.", "BASE", "TAXES", "TOTAL"], rows),
      ...(schedule.length ? [
        { text: `Conditions du contrat : ${schedule.length} échéance(s).`, style: "muted", margin: [0, 13, 0, 4] } as Content,
        { ...(schedule.length > 18 ? { pageBreak: "before" as const } : {}), table: { headerRows: 1, keepWithHeaderRows: 1, dontBreakRows: true, widths: [42, "*", 100, 104], body: [
          [{ text: "N°", style: "eyebrow" }, { text: "ÉCHÉANCE", style: "eyebrow" }, { text: "MONTANT", style: "eyebrow", alignment: "right" }, { text: "ÉTAT", style: "eyebrow", alignment: "right" }],
          ...schedule.map((item) => [value(item, "sequence"), dateLabel(item.due_date), { text: money(item.amount_due, sale.currency), alignment: "right" }, { text: statusLabel(item.status), alignment: "right" }]),
        ] }, layout: {
          hLineWidth: (index: number) => index === 0 ? 0 : 0.5,
          hLineColor: () => "#d9e3f0",
          vLineWidth: () => 0,
          paddingLeft: () => 6,
          paddingRight: () => 6,
          paddingTop: () => 3,
          paddingBottom: () => 3,
        } } as Content,
      ] : []),
      { unbreakable: true, stack: [
        ...financialSummary("Total TTC de la vente", "Sous-total HT", money(sale.subtotal, sale.currency), `TVA (${taxRate})`, money(sale.tax_amount, sale.currency), total),
        ...qrFooter(value(data, "qr_internal_reference"), "Document généré par SEL à partir des données du serveur."),
      ] } as Content,
    ],
  };
}

export async function buildClientPdf(documentType: "invoice" | "payment" | "solar", data: Data): Promise<PdfDocument> {
  const definitions = documentType === "invoice" ? invoiceDefinition(data) : documentType === "payment" ? paymentDefinition(data) : solarDefinition(data);
  const invoice = object(data.invoice);
  const reference = documentType === "invoice" ? value(invoice, "invoice_number", value(invoice, "invoice_id")) : value(data, "document_reference");
  const internalId = documentType === "invoice" ? value(invoice, "invoice_id") : documentType === "payment" ? value(object(data.payment), "payment_id") : value(object(data.sale), "sale_id");
  const title = documentType === "invoice" ? "Facture eau + électricité" : documentType === "payment" ? "Reçu de paiement" : "Facture de vente solaire";
  const filename = `${documentType === "invoice" ? "SEL-Facture" : documentType === "payment" ? "SEL-Recu" : "SEL-Vente-Solaire"}-${reference.replace(/[^a-zA-Z0-9_-]/g, "-")}-${internalId.replace(/[^a-zA-Z0-9_-]/g, "-")}.pdf`;
  const runtime = await loadPdfRuntime();
  const pdf = runtime.createPdf(definitions);
  return { blob: await pdf.getBlob(), pdf, filename, title };
}

export async function renderClientPdfPreview(blob: Blob, container: HTMLElement, onPageCount: (count: number) => void, signal: AbortSignal): Promise<() => void> {
  const pdfjs = await import("pdfjs-dist");
  if (signal.aborted) return () => {};
  pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
  const loadingTask = pdfjs.getDocument({ data: await blob.arrayBuffer() });
  const pdf: PDFDocumentProxy = await loadingTask.promise;
  const tasks: RenderTask[] = [];
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    for (const task of tasks) task.cancel();
    container.replaceChildren();
    void loadingTask.destroy();
  };
  if (signal.aborted) { await loadingTask.destroy(); return () => {}; }
  signal.addEventListener("abort", dispose, { once: true });
  container.replaceChildren();

  try {
    for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
      const page = await pdf.getPage(pageNumber);
      if (disposed) break;
      const viewport = page.getViewport({ scale: 1.5 });
      const pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
      const canvas = document.createElement("canvas");
      canvas.className = "document-pdf-page";
      canvas.setAttribute("role", "img");
      canvas.setAttribute("aria-label", `Page ${pageNumber} sur ${pdf.numPages}`);
      canvas.width = Math.floor(viewport.width * pixelRatio);
      canvas.height = Math.floor(viewport.height * pixelRatio);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      container.append(canvas);
      const context = canvas.getContext("2d");
      if (!context) throw new Error("L’aperçu du document n’est pas disponible.");
      const transform = pixelRatio === 1 ? undefined : [pixelRatio, 0, 0, pixelRatio, 0, 0];
      const task = page.render({ canvas, canvasContext: context, viewport, ...(transform ? { transform } : {}) });
      tasks.push(task);
      await task.promise;
      canvas.dataset.renderState = "complete";
    }
    if (!disposed) onPageCount(pdf.numPages);
  } catch (error) {
    for (const task of tasks) task.cancel();
    signal.removeEventListener("abort", dispose);
    if (signal.aborted) return () => {};
    await loadingTask.destroy();
    throw error;
  }

  signal.removeEventListener("abort", dispose);
  return dispose;
}
