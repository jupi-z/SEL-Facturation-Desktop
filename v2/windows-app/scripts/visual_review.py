from __future__ import annotations

import asyncio
import hashlib
from io import BytesIO
import json
import os
import subprocess
import time
import urllib.request
from pathlib import Path

from playwright.async_api import async_playwright
from pypdf import PdfReader


ROOT = Path(__file__).resolve().parents[3]
APP = ROOT / "v2" / "windows-app"
OUTPUT = ROOT / "artifacts" / "phase8-documents-figma"
URL = "http://127.0.0.1:1420"

CUSTOMERS = [{
    "id": "demo-customer-0001",
    "customer_code": "SEL-DÉMO-0001",
    "legal_name": "Client Alpha",
    "address": "Quartier Manika, Kolwezi",
    "phone": "+243 000 000 000",
    "status": "ACTIVE",
    "revision": 1,
}]
INVOICES = [{
    "invoice_id": "demo-invoice-0001",
    "invoice_number": "SEL-DÉMO-0001",
    "customer_id": "demo-customer-0001",
    "customer_name": "Client Alpha",
    "billing_period": "2026-10",
    "total": "11.02",
    "currency": "USD",
    "status": "LOCAL_PENDING",
}]
PAYMENTS = [{
    "payment_id": "demo-payment-0001",
    "customer_id": "demo-customer-0001",
    "amount_received": "3.00",
    "currency": "USD",
    "method": "CASH",
    "occurred_at": "2026-11-04T08:00:00Z",
    "status": "LOCAL_PENDING",
}]
SALES = [{
    "sale_id": "demo-sale-0001",
    "receivable_id": "demo-receivable-solar-0001",
    "customer_id": "demo-customer-0001",
    "product_code": "KIT-DÉMO",
    "principal": "120.00",
    "currency": "USD",
    "installment_plan_id": None,
    "status": "LOCAL_ACCEPTED",
}]
SOLAR_PLAN = {
    "id": "demo-plan-0001",
    "principal": "120.00",
    "paid": "0.00",
    "remaining": "120.00",
    "overdue_amount": "0.00",
    "currency": "USD",
    "schedule": [
        {"id": "demo-solar-schedule-0001", "sequence": 1, "due_date": "2026-12-01", "amount_due": "60.00", "paid": "0.00", "remaining": "60.00", "status": "UPCOMING"},
        {"id": "demo-solar-schedule-0002", "sequence": 2, "due_date": "2027-01-01", "amount_due": "60.00", "paid": "0.00", "remaining": "60.00", "status": "UPCOMING"},
    ],
}
PRODUCTS = [{
    "id": "demo-product-0001",
    "sku": "KIT-DÉMO",
    "label": "Kit solaire de démonstration",
    "price_reference": "120.00",
    "currency": "USD",
    "effective_from": "2026-01-01",
    "approval_ref": "DÉMO — DONNÉE FICTIVE",
}]
SERVICE_ACCOUNTS = [{
    "id": "demo-service-account-0001",
    "customer_id": "demo-customer-0001",
    "service_code": "ELECTRICITY",
    "connection_status": "CONNECTED",
}, {
    "id": "demo-service-account-water-0001",
    "customer_id": "demo-customer-0001",
    "service_code": "WATER",
    "connection_status": "CONNECTED",
}]
METERS = [{
    "id": "demo-meter-0001",
    "service_account_id": "demo-service-account-0001",
    "serial": "DEMO-KWH-001",
    "unit": "KWH",
    "active": True,
}, {
    "id": "demo-meter-water-0001",
    "service_account_id": "demo-service-account-water-0001",
    "serial": "DEMO-M3-001",
    "unit": "M3",
    "active": True,
}]
READINGS = [{
    "id": "demo-reading-0001",
    "meter_id": "demo-meter-0001",
    "period": "2026-10",
    "previous": "1000",
    "current": "1100",
    "measured_quantity": "100",
    "source": "MANUAL",
}, {
    "id": "demo-reading-water-0001",
    "meter_id": "demo-meter-water-0001",
    "period": "2026-10",
    "previous": "0",
    "current": "5",
    "measured_quantity": "5",
    "source": "MANUAL",
}]


def mock_invoke_script() -> str:
    fixtures = json.dumps({
        "customers": CUSTOMERS,
        "invoices": INVOICES,
        "payments": PAYMENTS,
        "sales": SALES,
        "products": PRODUCTS,
        "solarPlan": SOLAR_PLAN,
        "serviceAccounts": SERVICE_ACCOUNTS,
        "meters": METERS,
        "readings": READINGS,
    }, ensure_ascii=False)
    return f"""
      const fixtures = {fixtures};
      window.__TAURI_INTERNALS__ = {{ invoke: async (command, args = {{}}) => {{
        if (command === 'local_status') return {{status:'LOCAL_READY',pending:5,conflicts:1,rejected:1,last_successful_sync:'Aucune confirmée'}};
        if (command === 'central_session_status') return window.__ADMIN_DEMO_SESSION__ ? {{authenticated:true,user_id:'demo-admin-0001',organization_id:'demo-org-0001',role:'ADMIN'}} : {{authenticated:false}};
        if (command === 'central_login') {{ window.__ADMIN_DEMO_SESSION__ = true; return {{user_id:'demo-admin-0001',organization_id:'demo-org-0001',role:'ADMIN'}}; }}
        if (command === 'central_logout') {{ window.__ADMIN_DEMO_SESSION__ = false; return null; }}
        if (command === 'central_admin_request') {{
          const adminPath = args.path || '';
          let adminBody = [];
          if (adminPath === '/v1/admin/users') adminBody = [{{id:'demo-admin-0001',identity_ref:'admin-demo',role:'ADMIN',status:'ACTIVE'}},{{id:'demo-agent-0001',identity_ref:'agent-demo',role:'BILLING_AGENT',status:'ACTIVE'}}];
          else if (adminPath === '/v1/admin/devices') adminBody = [{{device_id:'demo-device-0001',status:'ACTIVE',last_seen_at:'2026-11-04T08:00:00Z'}}];
          else if (adminPath === '/v1/admin/offline-grants') adminBody = [{{grant_id:'demo-grant-0001',user_id:'demo-agent-0001',expires_at:'2026-12-01',status:'ACTIVE'}}];
          else if (adminPath === '/v1/admin/audit-events') adminBody = [{{id:'demo-audit-0001',event_type:'USER_CREATED',actor_identity:'admin-demo',subject_id:'demo-agent-0001',occurred_at:'2026-11-04T08:10:00Z',redacted_metadata:{{role:'BILLING_AGENT'}}}}];
          else if (adminPath.endsWith('/tariffs')) adminBody = [{{service_code:'ELECTRICITY',customer_category:'DEMO',unit_price:'0.095',currency:'USD',status:'APPROVED'}}];
          else if (adminPath.endsWith('/products')) adminBody = fixtures.products.map(product => ({{...product,status:'APPROVED'}}));
          else if (adminPath.endsWith('/tax-profiles')) adminBody = [{{service_code:'DEMO',jurisdiction:'DEMO',base:'NET',rate:'0.16',status:'APPROVED'}}];
          else if (adminPath.endsWith('/fx-rates')) adminBody = [{{from_currency:'USD',to_currency:'CDF',value:'DEMO',effective_from:'2026-01-01',status:'APPROVED'}}];
          return {{status:200,body:adminBody}};
        }}
        if (command !== 'local_request') throw new Error('Unexpected visual smoke command: ' + command);
        const path = args.path || '';
        let body = [];
        if (path === '/local/v1/customers') body = fixtures.customers;
        else if (path === '/local/v1/billing/utility/preview') {{
          const waterQuantity = Number(args.body?.water?.quantity ?? 0);
          body = waterQuantity > 0
            ? {{currency:'USD',subtotal:'11.00',tax:'1.76',total:'12.76',lines:[{{service_code:'ELECTRICITY',quantity:'100.00',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},{{service_code:'WATER',quantity:'5.00',unit:'M3',unit_price:'0.30',subtotal:'1.50',tax:'0.24',total:'1.74'}}]}}
            : {{currency:'USD',subtotal:'9.50',tax:'1.52',total:'11.02',lines:[{{service_code:'ELECTRICITY',quantity:'100.00',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},{{service_code:'WATER',quantity:'0.00',unit:'M3',unit_price:'0.00',subtotal:'0.00',tax:'0.00',total:'0.00'}}]}};
        }}
        else if (path.startsWith('/local/v1/customers/') && path.endsWith('/statement')) body = {{invoices:fixtures.invoices,receivables:[{{id:'demo-receivable-0001',balance:'8.02',currency:'USD',status:'OPEN',source_type:'UTILITY_INVOICE'}},{{id:'demo-receivable-solar-0001',balance:'120.00',currency:'USD',status:'OPEN',source_type:'SOLAR_SALE'}}]}};
        else if (path.startsWith('/local/v1/customers/') && path.endsWith('/service-accounts')) body = fixtures.serviceAccounts;
        else if (path.startsWith('/local/v1/service-accounts/') && path.endsWith('/meters')) {{ const serviceAccountId = path.split('/')[4]; body = fixtures.meters.filter((meter) => meter.service_account_id === serviceAccountId); }}
        else if (path.startsWith('/local/v1/meters/') && path.endsWith('/readings/preview')) {{ const meterId = path.split('/')[4]; body = meterId === 'demo-meter-water-0001' ? {{meter_id:meterId,period:'2026-11',previous:'5',current:'10',measured_quantity:'5',source:'MANUAL'}} : {{meter_id:meterId,period:'2026-11',previous:'1100',current:'1200',measured_quantity:'100',source:'MANUAL'}}; }}
        else if (path.startsWith('/local/v1/meters/') && path.endsWith('/readings')) {{ const meterId = path.split('/')[4]; body = fixtures.readings.filter((reading) => reading.meter_id === meterId); }}
        else if (path.startsWith('/local/v1/customers/')) body = fixtures.customers[0];
        else if (path.startsWith('/local/v1/invoices/') && path.endsWith('/document-data')) {{
          const positiveWater = window.__invoicePdfWaterPositive === true;
          const invoiceLines = [
            {{service_code:'ELECTRICITY',quantity:'100',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},
            positiveWater
              ? {{service_code:'WATER',quantity:'5',unit:'M3',unit_price:'0.30',subtotal:'1.50',tax:'0.24',total:'1.74'}}
              : {{service_code:'WATER',quantity:'0',unit:'M3',unit_price:'0',subtotal:'0',tax:'0',total:'0'}}
          ];
          body = {{document_schema_version:1,template_version:'v2-a4-2',document_reference:'SEL-DÉMO-0001',qr_internal_reference:'SEL-V2-demo-invoice-0001',origin:'LOCAL_PENDING',issued_at:'2026-11-03T10:30:00Z',identity_snapshot:{{organization_name:'SEL V2 local organization',customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi',phone:'+243 000 000 000'}},invoice:{{...fixtures.invoices[0],invoice_number:'SEL-DÉMO-0001',billing_period:'2026-10',due_date:'2026-11-15',subtotal:positiveWater?'11.00':'9.50',tax:positiveWater?'1.76':'1.52',total:positiveWater?'12.76':'11.02',lines:invoiceLines}}}};
        }}
        else if (path.startsWith('/local/v1/invoices/')) body = {{...fixtures.invoices[0],lines:[{{service_code:'ELECTRICITY',quantity:'100',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}}],subtotal:'9.50',tax:'1.52'}};
        else if (path.startsWith('/local/v1/invoices')) body = fixtures.invoices;
        else if (path.startsWith('/local/v1/payments/') && path.endsWith('/document-data')) body = {{document_schema_version:1,document_reference:'SEL-PAY-DEMO-0001',qr_internal_reference:'SEL:PAYMENT:demo-payment-0001',payment:{{payment_id:'demo-payment-0001',amount_received:'3.00',currency:'USD',method:'CASH',paid_at:'2026-11-04T08:00:00Z',status:'LOCAL_PENDING'}},customer:{{customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi'}},allocations:[{{receivable_id:'demo-receivable-0001',amount:'3.00',currency:'USD'}}],reversals:[],origin:'LOCAL'}};
        else if (path.startsWith('/local/v1/payments')) body = fixtures.payments;
        else if (path.startsWith('/local/v1/solar/sales/') && path.endsWith('/document-data')) {{
          const longSchedule = window.__solarDocumentLongSchedule === true
            ? Array.from({{length:36}},(_,index)=>({{sequence:index+1,due_date:`2027-${{String(Math.floor(index/12)+1).padStart(2,'0')}}-${{String((index%12)+1).padStart(2,'0')}}`,amount_due:'5.00',status:'UPCOMING'}}))
            : fixtures.solarPlan.schedule;
          body = {{document_schema_version:1,document_type:'SOLAR_SALE_CONFIRMATION',document_reference:'SEL-SOLAR-DEMO-0001',qr_internal_reference:'SEL:SOLAR-SALE:demo-sale-0001',sale:{{sale_id:'demo-sale-0001',receivable_id:'demo-receivable-solar-0001',customer_id:'demo-customer-0001',principal:'120.00',currency:'USD',sold_at:'2026-11-04T08:00:00Z',status:'LOCAL_ACCEPTED'}},customer:{{customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi'}},lines:[{{product_reference:'KIT-DÉMO',quantity:'1',unit_price:'120.00',total:'120.00'}}],installment_plan:{{currency:'USD',schedule:longSchedule}},origin:'LOCAL_ACCEPTED'}};
        }}
        else if (path.startsWith('/local/v1/solar/sales')) body = fixtures.sales;
        else if (path.startsWith('/local/v1/solar/products')) body = fixtures.products;
        else if (path.startsWith('/local/v1/solar/contracts')) {{ fixtures.sales[0].installment_plan_id = 'demo-plan-0001'; body = {{installment_plan_id:'demo-plan-0001',contract_id:'demo-contract-0001'}}; }}
        else if (path.startsWith('/local/v1/installment-plans/')) body = fixtures.solarPlan;
        else if (path.startsWith('/local/v1/sync/trigger')) body = {{status:'PENDING_NETWORK',pending:5}};
        else if (path.startsWith('/local/v1/sync/issues')) body = {{conflicts:1,rejected:1,items:[{{id:'demo-conflict-0001',event_id:'demo-event-0001',command_type:'UTILITY_INVOICE_ISSUED',status:'CONFLICT',reason:'Révision centrale différente',central_revision:4,created_at:'2026-11-04T08:12:00Z'}}]}};
        else if (path.startsWith('/local/v1/sync/status')) body = {{status:'LOCAL_READY',pending:5,conflicts:0,rejected:0,last_successful_sync:'Aucune confirmée'}};
        return {{status:200,body}};
      }}}};
    """


async def wait_for_server(process: subprocess.Popen[bytes]) -> None:
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Vite exited before ready with status {process.returncode}")
        try:
            with urllib.request.urlopen(URL, timeout=2):
                return
        except Exception:
            time.sleep(0.5)
    raise TimeoutError("Vite did not start within 60 seconds")


async def assert_pdf_rendered(page, label: str, expected_pages: int | None = 1) -> str:
    await page.wait_for_function("document.querySelector('.document-pdf-page[data-render-state=complete]') || (document.querySelector('.document-pdf-status')?.textContent && !document.querySelector('.document-pdf-status').textContent.startsWith('Préparation'))", timeout=15000)
    page_count = await page.locator(".document-pdf-page").count()
    if not page_count:
        status = await page.locator(".document-pdf-status").inner_text()
        raise AssertionError(f"{label} PDF could not be rendered: {status}")
    if expected_pages is not None and page_count != expected_pages:
        raise AssertionError(f"{label} should render {expected_pages} A4 page(s), got {page_count}")
    if expected_pages is None and page_count < 2:
        raise AssertionError(f"{label} long-document fixture should paginate, got {page_count} page(s)")
    canvas = page.locator(".document-pdf-page").first
    result = await canvas.evaluate("canvas => { const ctx = canvas.getContext('2d'); const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data; let ink = 0; for (let i = 0; i < data.length; i += 64) if (data[i] < 225 || data[i + 1] < 225 || data[i + 2] < 225) ink++; return {width:canvas.width,height:canvas.height,ink,status:document.querySelector('.document-pdf-status')?.textContent}; }")
    if result["width"] < 850 or result["height"] < 1200 or result["ink"] < 100:
        raise AssertionError(f"{label} PDF page was not visibly rendered: {result}")
    image_data = await canvas.evaluate("canvas => canvas.toDataURL('image/png')")
    return hashlib.sha256(image_data.encode("ascii")).hexdigest()


async def assert_generated_pdf_contract(page, label: str, expected_pages: int | None, required_text: tuple[str, ...]) -> PdfReader:
    captured = await page.evaluate("() => window.__SEL_PDF_CAPTURE || null")
    if not captured:
        raise AssertionError(f"{label}: the app-generated PDF bytes were not captured")
    reader = PdfReader(BytesIO(bytes(captured)))
    if expected_pages is not None and len(reader.pages) != expected_pages:
        raise AssertionError(f"{label}: expected {expected_pages} generated PDF page(s), got {len(reader.pages)}")
    if expected_pages is None and len(reader.pages) < 2:
        raise AssertionError(f"{label}: long document did not paginate")
    for index, pdf_page in enumerate(reader.pages, start=1):
        width, height = float(pdf_page.mediabox.width), float(pdf_page.mediabox.height)
        if not (590 <= width <= 600 and 837 <= height <= 847):
            raise AssertionError(f"{label}: page {index} is not A4 ({width:.2f} x {height:.2f} pt)")
    text = "\n".join(pdf_page.extract_text() or "" for pdf_page in reader.pages)
    for token in required_text:
        if token.casefold() not in text.casefold():
            raise AssertionError(f"{label}: expected PDF text {token!r} was missing")
    if "Référence interne SEL" not in text or "sans authentification fiscale" not in text:
        raise AssertionError(f"{label}: the QR is not clearly identified as an internal reference")
    for page_index, pdf_page in enumerate(reader.pages, start=1):
        fonts = pdf_page.get("/Resources", {}).get("/Font", {})
        embedded_fonts = []
        for font_ref in fonts.values():
            font = font_ref.get_object()
            candidates = [font]
            candidates.extend(
                descendant.get_object()
                for descendant in font.get("/DescendantFonts", [])
            )
            for candidate in candidates:
                descriptor_ref = candidate.get("/FontDescriptor")
                if descriptor_ref:
                    descriptor = descriptor_ref.get_object()
                    if any(descriptor.get(key) for key in ("/FontFile", "/FontFile2", "/FontFile3")):
                        embedded_fonts.append(candidate.get("/BaseFont", font.get("/BaseFont", "unknown")))
        if not embedded_fonts:
            raise AssertionError(f"{label}: page {page_index} has no embedded font program")
    return reader


async def assert_print_layout(page, label: str, expected_pages: int) -> None:
    print_bytes = await page.pdf(format="A4", print_background=True, prefer_css_page_size=True)
    reader = PdfReader(BytesIO(print_bytes))
    if len(reader.pages) != expected_pages:
        raise AssertionError(f"{label}: Windows/WebView print layout produced {len(reader.pages)} pages, expected {expected_pages}")
    for index, pdf_page in enumerate(reader.pages, start=1):
        width, height = float(pdf_page.mediabox.width), float(pdf_page.mediabox.height)
        if not (590 <= width <= 600 and 837 <= height <= 847):
            raise AssertionError(f"{label}: printed page {index} is not A4 ({width:.2f} x {height:.2f} pt)")


async def capture_views() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(
        ["npm.cmd", "run", "dev", "--", "--host", "127.0.0.1"],
        cwd=APP,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    try:
        await wait_for_server(process)
        async with async_playwright() as playwright:
            browser = await playwright.chromium.launch(headless=True)
            page = await browser.new_page(viewport={"width": 1440, "height": 900}, device_scale_factor=1, locale="fr-CD")
            await page.add_init_script(mock_invoke_script())
            await page.add_init_script("""(() => {
              const original = Blob.prototype.arrayBuffer;
              Blob.prototype.arrayBuffer = async function() {
                const bytes = await original.call(this);
                if (this.type.toLowerCase().includes('pdf')) window.__SEL_PDF_CAPTURE = Array.from(new Uint8Array(bytes));
                return bytes;
              };
            })();""")
            await page.goto(URL, wait_until="domcontentloaded")
            await page.get_by_role("heading", name="Vue d’ensemble").wait_for()
            views = [
                ("dashboard", "Vue d’ensemble"),
                ("customers", "Abonnés"),
                ("readings", "Relevés"),
                ("invoices", "Factures"),
                ("payments", "Encaissements"),
                ("solar", "Kits solaires"),
                ("receivables", "Créances"),
                ("sync", "Synchronisation"),
                ("admin", "Administration"),
            ]
            for slug, label in views:
                await page.get_by_role("button", name=label, exact=True).click()
                if slug == "readings":
                    selects = page.locator(".reading-filters select")
                    await selects.nth(0).select_option("demo-customer-0001")
                    await selects.nth(1).select_option("demo-service-account-0001")
                    await selects.nth(2).select_option("demo-meter-0001")
                    await page.locator('.reading-entry input[name="period"]').fill("2026-11")
                    await page.locator('.reading-entry input[name="current"]').fill("1200")
                    await page.get_by_role("button", name="Prévisualiser les index").click()
                    await page.locator(".reading-quantity").get_by_text("100 kWh", exact=True).wait_for()
                    await page.locator(".reading-filters select").nth(1).select_option("demo-service-account-water-0001")
                    await page.locator('.reading-filters select option[value="demo-meter-water-0001"]').wait_for(state="attached")
                    await page.locator(".reading-filters select").nth(2).select_option("demo-meter-water-0001")
                    await page.get_by_role("heading", name="Relevé d’eau au m³").wait_for()
                    await page.wait_for_function("document.querySelector('.reading-entry input[name=previous]')?.value === '5'")
                    await page.locator('.reading-entry input[name="period"]').fill("2026-11")
                    await page.locator('.reading-entry input[name="current"]').fill("10")
                    await page.get_by_role("button", name="Prévisualiser les index").click()
                    await page.locator(".reading-quantity").get_by_text("5 m³", exact=True).wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "water-reading-preview-1440x900.png"))
                if slug == "payments":
                    await page.locator('.payment-workspace form select[name="customer_id"]').select_option("demo-customer-0001")
                    await page.locator('.allocation-card select option[value="demo-receivable-0001"]').wait_for(state="attached")
                    await page.locator('.allocation-card select').first.select_option("demo-receivable-0001")
                    await page.locator('input[name="received_amount"]').fill("3.00")
                    await page.locator('.allocation-card input[type="number"]').fill("3.00")
                    await page.locator('input[name="occurred_at"]').fill("2026-11-04T10:00")
                if slug == "solar":
                    await page.get_by_label("Vente à financer").select_option("demo-sale-0001")
                    await page.get_by_label("Échéance 1 · Date").fill("2026-12-01")
                    await page.get_by_label("Montant dû").fill("60.00")
                    await page.get_by_role("button", name="+ Ajouter une échéance").click()
                    await page.get_by_label("Échéance 2 · Date").fill("2027-01-01")
                    await page.locator("label", has_text="Montant dû").nth(1).locator("input").fill("60.00")
                    await page.get_by_label("Référence des conditions approuvées").fill("DEMO-SOLAR-TERMS")
                    await page.get_by_role("button", name="Créer le contrat").click()
                    await page.get_by_role("heading", name="Échéancier consulté").wait_for()
                    await page.get_by_role("heading", name="Contrats et échéanciers solaires").evaluate(
                        "element => element.scrollIntoView({ block: 'start' })"
                    )
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "solar-contracts-1440x900.png"))
                await page.evaluate("window.scrollTo(0, 0)")
                await page.wait_for_timeout(120)
                await page.screenshot(path=str(OUTPUT / f"{slug}-1440x900.png"))
                if slug == "customers":
                    await page.get_by_role("button", name="+ Ajouter un abonné").click()
                    await page.get_by_role("heading", name="Nouvel abonné").wait_for()
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "customer-new-1440x900.png"))
                    await page.get_by_role("button", name="Fermer le formulaire").click()
                    await page.get_by_role("button", name="Ouvrir le dossier →").click()
                    await page.get_by_role("heading", name="Dossier abonné").wait_for()
                    await page.get_by_role("heading", name="Situation du compte · Client Alpha").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "customer-record-1440x900.png"))
                    await page.get_by_role("button", name="Modifier la fiche").click()
                    await page.get_by_label("Nom / raison sociale").wait_for()
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "customer-edit-1440x900.png"))
                    await page.get_by_role("button", name="Annuler la modification").click()
                if slug == "receivables":
                    await page.get_by_label("Abonné").select_option("demo-customer-0001")
                    await page.get_by_text("demo-receivable-0001").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "receivables-statement-1440x900.png"))
                if slug == "invoices":
                    await page.get_by_label("Abonné").select_option("demo-customer-0001")
                    await page.get_by_label("Période de facturation").fill("2026-10")
                    await page.get_by_label("Électricité consommée (kWh)").fill("100")
                    await page.get_by_label("Eau consommée (m³)").fill("0")
                    await page.get_by_label("Révision tarifaire approuvée").fill("DÉMO — DONNÉE FICTIVE")
                    await page.get_by_label("Je confirme que le relevé d’eau est exactement de 0 m³.").check()
                    await page.get_by_role("button", name="Prévisualiser la facture").click()
                    await page.get_by_role("heading", name="Prévisualisation de facture").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-preview-1440x900.png"))
                    await page.get_by_role("button", name="Modifier la saisie").click()
                    if await page.locator('input[name="electricity_quantity"]').input_value() != "100":
                        raise AssertionError("Invoice input was not preserved when returning from the preview")
                    await page.get_by_label("Eau consommée (m³)").fill("5")
                    await page.get_by_role("button", name="Prévisualiser la facture").click()
                    await page.get_by_role("heading", name="Prévisualisation de facture").wait_for()
                    await page.locator(".invoice-preview-metrics").get_by_text("5 m³", exact=True).wait_for()
                    await page.locator(".invoice-preview-metrics").get_by_text("18,56 USD", exact=True).wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-preview-water-positive-1440x900.png"))
                    await page.get_by_role("button", name="Retour à l’historique des factures", exact=True).click()
                    await page.get_by_role("heading", name="Factures eau + électricité").wait_for()
                    await page.get_by_role("button", name="Document PDF").first.click()
                    try:
                        await page.get_by_role("dialog", name="Facture eau + électricité").wait_for(timeout=5000)
                    except Exception as error:
                        notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                        raise AssertionError(f"Invoice PDF preview did not open: {notice}") from error
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-diagnostic.png"))
                    zero_water_pdf = await assert_pdf_rendered(page, "Invoice with 0 m³ of water")
                    await assert_generated_pdf_contract(page, "Invoice with 0 m³ of water", 1, ("Électricité", "Eau", "0 m³", "11,02 USD"))
                    await assert_print_layout(page, "Invoice with 0 m³ of water", 1)
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-preview-1440x900.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-lower-zero-water.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__invoicePdfWaterPositive = true")
                    await page.get_by_role("button", name="Document PDF").first.click()
                    await page.get_by_role("dialog", name="Facture eau + électricité").wait_for()
                    positive_water_pdf = await assert_pdf_rendered(page, "Invoice with 5 m³ of water")
                    await assert_generated_pdf_contract(page, "Invoice with 5 m³ of water", 1, ("Électricité", "Eau", "5 m³", "18,56 USD"))
                    await assert_print_layout(page, "Invoice with 5 m³ of water", 1)
                    if positive_water_pdf == zero_water_pdf:
                        raise AssertionError("0 m³ and 5 m³ invoice PDFs rendered identically")
                    await page.screenshot(path=str(OUTPUT / "invoice-water-positive-pdf-preview-1440x900.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-lower-water-positive.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                if slug == "solar":
                    await page.get_by_role("button", name="Confirmation PDF").first.click()
                    try:
                        await page.get_by_role("dialog", name="Facture de vente solaire").wait_for(timeout=5000)
                    except Exception as error:
                        notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                        raise AssertionError(f"Solar PDF preview did not open: {notice}") from error
                    await assert_pdf_rendered(page, "Solar sale")
                    await assert_generated_pdf_contract(page, "Solar sale", 1, ("Facture de vente", "KIT-DÉMO", "120,00 USD"))
                    await assert_print_layout(page, "Solar sale", 1)
                    await page.screenshot(path=str(OUTPUT / "solar-pdf-preview-1440x900.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__solarDocumentLongSchedule = true")
                    await page.get_by_role("button", name="Confirmation PDF").first.click()
                    await page.get_by_role("dialog", name="Facture de vente solaire").wait_for()
                    await assert_pdf_rendered(page, "Solar sale with 36 installments", expected_pages=None)
                    long_solar_pages = await page.locator(".document-pdf-page").count()
                    long_solar_pdf = await assert_generated_pdf_contract(page, "Solar sale with 36 installments", long_solar_pages, ("Facture de vente", "KIT-DÉMO", "ÉCHÉANCES", "mars 2027"))
                    if len(long_solar_pdf.pages) != 2 or "SYNTHÈSE FINANCIÈRE" not in (long_solar_pdf.pages[-1].extract_text() or "") or "Référence interne SEL" not in (long_solar_pdf.pages[-1].extract_text() or ""):
                        raise AssertionError("Long solar schedule must keep its financial summary and internal QR reference together on the final page")
                    await assert_print_layout(page, "Solar sale with 36 installments", long_solar_pages)
                    await page.screenshot(path=str(OUTPUT / "solar-pdf-multipage-1440x900.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "solar-pdf-multipage-last-page-1440x900.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__solarDocumentLongSchedule = false")
                    await page.get_by_role("button", name="Créer une vente").click()
                    await page.get_by_role("heading", name="Nouvelle vente solaire").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "solar-new-sale-1440x900.png"))
                    await page.get_by_role("button", name="Fermer", exact=True).click()
                if slug == "admin":
                    await page.get_by_role("heading", name="Connexion administrateur centrale").wait_for()
                    await page.screenshot(path=str(OUTPUT / "admin-login-1440x900.png"))
                    await page.evaluate("window.__ADMIN_DEMO_SESSION__ = true")
                    await page.get_by_role("button", name="Vue d’ensemble", exact=True).click()
                    await page.get_by_role("button", name="Administration", exact=True).click()
                    await page.get_by_role("heading", name="Utilisateurs et rôles").wait_for()
                    await page.get_by_role("cell", name="admin-demo").first.wait_for()
                    await page.screenshot(path=str(OUTPUT / "admin-full-1440x900.png"), full_page=True)
                dimensions = await page.evaluate("({viewport: document.documentElement.clientWidth, content: document.documentElement.scrollWidth})")
                if dimensions["content"] > dimensions["viewport"]:
                    raise AssertionError(f"{slug} overflows horizontally: {dimensions}")
            await page.get_by_role("button", name="Encaissements", exact=True).click()
            await page.locator('.payment-workspace form select[name="customer_id"]').select_option("demo-customer-0001")
            await page.get_by_role("button", name="+ Ajouter une créance").click()
            await page.locator('.allocation-card select').first.locator('option[value="demo-receivable-0001"]').wait_for(state="attached")
            await page.locator('.allocation-card select').nth(0).select_option("demo-receivable-0001")
            await page.locator('.allocation-card select').nth(1).select_option("demo-receivable-solar-0001")
            await page.locator('.allocation-card select option[value="demo-solar-schedule-0001"]').wait_for(state="attached")
            await page.locator('.allocation-card select').nth(2).select_option("demo-solar-schedule-0001")
            await page.locator('.allocation-card input[type="number"]').nth(0).fill("1.00")
            await page.locator('.allocation-card input[type="number"]').nth(1).fill("2.00")
            await page.locator('input[name="received_amount"]').fill("3.00")
            await page.get_by_role("button", name="Reçu PDF").first.click()
            try:
                await page.get_by_role("dialog", name="Reçu de paiement").wait_for(timeout=5000)
            except Exception as error:
                notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                raise AssertionError(f"Payment receipt PDF preview did not open: {notice}") from error
            await assert_pdf_rendered(page, "Payment receipt")
            await assert_generated_pdf_contract(page, "Payment receipt", 1, ("Reçu de paiement", "3,00 USD", "CASH"))
            await assert_print_layout(page, "Payment receipt", 1)
            await page.screenshot(path=str(OUTPUT / "payment-receipt-pdf-preview-1440x900.png"))
            await page.get_by_role("button", name="Fermer l’aperçu").click()
            await page.evaluate("window.scrollTo(0, 0)")
            await page.wait_for_timeout(120)
            await page.screenshot(path=str(OUTPUT / "payments-solar-1440x900.png"))
            await browser.close()
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    asyncio.run(capture_views())
