from __future__ import annotations

import asyncio
import json
import os
import subprocess
import time
import urllib.request
from pathlib import Path

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[3]
APP = ROOT / "v2" / "windows-app"
OUTPUT = ROOT / "artifacts" / "phase7-visual"
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
    "customer_id": "demo-customer-0001",
    "product_code": "KIT-DÉMO",
    "principal": "120.00",
    "currency": "USD",
    "status": "LOCAL_ACCEPTED",
}]
PRODUCTS = [{
    "id": "demo-product-0001",
    "sku": "KIT-DÉMO",
    "label": "Kit solaire de démonstration",
    "price_reference": "120.00",
    "currency": "USD",
    "effective_from": "2026-01-01",
    "approval_ref": "DÉMO — DONNÉE FICTIVE",
}]


def mock_invoke_script() -> str:
    fixtures = json.dumps({
        "customers": CUSTOMERS,
        "invoices": INVOICES,
        "payments": PAYMENTS,
        "sales": SALES,
        "products": PRODUCTS,
    }, ensure_ascii=False)
    return f"""
      const fixtures = {fixtures};
      window.__TAURI_INTERNALS__ = {{ invoke: async (command, args = {{}}) => {{
        if (command === 'local_status') return {{status:'LOCAL_READY',pending:5,conflicts:0,rejected:0,last_successful_sync:'Aucune confirmée'}};
        if (command !== 'local_request') throw new Error('Unexpected visual smoke command: ' + command);
        const path = args.path || '';
        let body = [];
        if (path === '/local/v1/customers') body = fixtures.customers;
        else if (path.startsWith('/local/v1/customers/') && path.endsWith('/statement')) body = {{invoices:fixtures.invoices,receivables:[{{id:'demo-receivable-0001',balance:'8.02',currency:'USD',status:'OPEN',source_type:'UTILITY_INVOICE'}}]}};
        else if (path.startsWith('/local/v1/customers/')) body = fixtures.customers[0];
        else if (path.startsWith('/local/v1/invoices/')) body = {{...fixtures.invoices[0],lines:[{{service_code:'ELECTRICITY',quantity:'100',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}}],subtotal:'9.50',tax:'1.52'}};
        else if (path.startsWith('/local/v1/invoices')) body = fixtures.invoices;
        else if (path.startsWith('/local/v1/payments')) body = fixtures.payments;
        else if (path.startsWith('/local/v1/solar/sales')) body = fixtures.sales;
        else if (path.startsWith('/local/v1/solar/products')) body = fixtures.products;
        else if (path.startsWith('/local/v1/sync/trigger')) body = {{status:'PENDING_NETWORK',pending:5}};
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
            page = await browser.new_page(viewport={"width": 1440, "height": 900}, device_scale_factor=1)
            await page.add_init_script(mock_invoke_script())
            await page.goto(URL, wait_until="domcontentloaded")
            await page.get_by_role("heading", name="Vue d’ensemble").wait_for()
            views = [
                ("dashboard", "Vue d’ensemble"),
                ("customers", "Abonnés"),
                ("invoices", "Factures"),
                ("payments", "Encaissements"),
                ("solar", "Kits solaires"),
                ("receivables", "Créances"),
                ("sync", "Synchronisation"),
                ("admin", "Administration"),
            ]
            for slug, label in views:
                await page.get_by_role("button", name=label, exact=True).click()
                await page.wait_for_timeout(120)
                await page.screenshot(path=str(OUTPUT / f"{slug}-1440x900.png"), full_page=True)
                dimensions = await page.evaluate("({viewport: document.documentElement.clientWidth, content: document.documentElement.scrollWidth})")
                if dimensions["content"] > dimensions["viewport"]:
                    raise AssertionError(f"{slug} overflows horizontally: {dimensions}")
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
