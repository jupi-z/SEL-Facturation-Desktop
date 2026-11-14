from __future__ import annotations

import asyncio
import hashlib
from io import BytesIO
import json
import os
import socket
import subprocess
import time
import urllib.request
from pathlib import Path

from playwright.async_api import async_playwright
from pypdf import PdfReader


ROOT = Path(__file__).resolve().parents[3]
APP = ROOT / "v2" / "windows-app"
OUTPUT = Path(os.environ.get("SEL_VISUAL_OUTPUT", ROOT / "artifacts" / "phase8-documents-figma"))
VIEWPORT_WIDTH = int(os.environ.get("SEL_VIEWPORT_WIDTH", "1440"))
VIEWPORT_HEIGHT = int(os.environ.get("SEL_VIEWPORT_HEIGHT", "900"))
SHOT_SUFFIX = f"{VIEWPORT_WIDTH}x{VIEWPORT_HEIGHT}"
FAILED_REQUESTS: list[str] = []

CUSTOMERS = [{
    "id": "demo-customer-0001",
    "customer_code": "SEL-DÉMO-0001",
    "legal_name": "Client Alpha",
    "customer_category": "DEMO",
    "address": "Quartier Manika, Kolwezi",
    "phone": "+243 000 000 000",
    "status": "ACTIVE",
    "revision": 1,
}, {
    "id": "demo-customer-0002",
    "customer_code": "SEL-DÉMO-0002",
    "legal_name": "Client Beta",
    "customer_category": "DEMO",
    "address": "Quartier Joli Site, Kolwezi",
    "phone": "+243 000 000 001",
    "status": "ACTIVE",
    "revision": 1,
}]
INVOICES = [{
    "invoice_id": "demo-invoice-0001",
    "invoice_number": "SEL-DÉMO-0001",
    "customer_id": "demo-customer-0001",
    "customer_name": "Client Alpha",
    "billing_period": "2026-10",
    "total": "22.62",
    "currency": "USD",
    "status": "CENTRAL_ACCEPTED",
}]
PAYMENTS = [{
    "payment_id": "demo-payment-0001",
    "customer_id": "demo-customer-0001",
    "amount_received": "3.00",
    "currency": "USD",
    "method": "CASH",
    "occurred_at": "2026-11-04T08:00:00Z",
    "status": "CENTRAL_ACCEPTED",
}]
SALES = [{
    "sale_id": "demo-sale-0001",
    "receivable_id": "demo-receivable-solar-0001",
    "customer_id": "demo-customer-0001",
    "product_code": "KIT-DÉMO",
    "subtotal": "120.00",
    "tax_amount": "19.20",
    "principal": "139.20",
    "currency": "USD",
    "installment_plan_id": None,
    "status": "CENTRAL_ACCEPTED",
}]
RECEIVABLES = [
    {"receivable_id": "demo-receivable-0001", "customer_id": "demo-customer-0001", "customer_code": "SEL-DÉMO-0001", "customer_name": "Client Alpha", "source_type": "INVOICE", "source_reference": "SEL-20261103-103000", "currency": "USD", "principal": "22.62", "paid": "3.00", "credited": "0.00", "balance": "19.62", "overdue_amount": "0.00", "status": "OPEN"},
    {"receivable_id": "demo-receivable-solar-0001", "customer_id": "demo-customer-0001", "customer_code": "SEL-DÉMO-0001", "customer_name": "Client Alpha", "source_type": "SALE", "source_reference": "demo-sale-0001", "currency": "USD", "principal": "139.20", "paid": "0.00", "credited": "0.00", "balance": "139.20", "overdue_amount": "0.00", "status": "OPEN"},
    {"receivable_id": "demo-receivable-0002", "customer_id": "demo-customer-0002", "customer_code": "SEL-DÉMO-0002", "customer_name": "Client Beta", "source_type": "INVOICE", "source_reference": "SEL-20261103-104000", "currency": "USD", "principal": "8.00", "paid": "0.00", "credited": "0.00", "balance": "8.00", "overdue_amount": "0.00", "status": "OPEN"},
]
SOLAR_PLAN = {
    "id": "demo-plan-0001",
    "receivable_id": "demo-receivable-solar-0001",
    "customer_id": "demo-customer-0001",
    "principal": "139.20",
    "paid": "0.00",
    "remaining": "139.20",
    "overdue_amount": "0.00",
    "currency": "USD",
    "schedule": [
        {"id": "demo-solar-schedule-0001", "sequence": 1, "due_date": "2026-12-01", "amount_due": "69.60", "paid": "0.00", "remaining": "69.60", "status": "UPCOMING"},
        {"id": "demo-solar-schedule-0002", "sequence": 2, "due_date": "2027-01-01", "amount_due": "69.60", "paid": "0.00", "remaining": "69.60", "status": "UPCOMING"},
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
        "receivables": RECEIVABLES,
        "sales": SALES,
        "products": PRODUCTS,
        "tariffs": [
            {"service_code": "ELECTRICITY", "customer_category": "DEMO", "unit": "KWH", "unit_price": "0.095", "currency": "USD", "effective_from": "2026-01-01", "approval_ref": "DEMO-APPROVED-REVISION", "status": "APPROVED"},
            {"service_code": "WATER", "customer_category": "DEMO", "unit": "M3", "unit_price": "0.30", "currency": "USD", "effective_from": "2026-01-01", "approval_ref": "DEMO-APPROVED-REVISION", "status": "APPROVED"},
        ],
        "solarPlan": SOLAR_PLAN,
        "serviceAccounts": SERVICE_ACCOUNTS,
        "meters": METERS,
        "readings": READINGS,
    }, ensure_ascii=False)
    return f"""
      const fixtures = {fixtures};
      window.__paymentPostCount = 0;
      window.__adminWriteLog = [];
      window.__centralWriteLog = [];
      window.__statementRequestStarted = false;
      window.__releaseCustomerStatement = null;
      window.__TAURI_INTERNALS__ = {{ invoke: async (command, args = {{}}) => {{
        if (command === 'central_status') return {{status:'CENTRAL_CONNECTED',configured:true}};
        if (command === 'central_session_status') return window.__ADMIN_DEMO_SESSION__ ? {{authenticated:true,user_id:'demo-admin-0001',organization_id:'demo-org-0001',role:'ADMIN'}} : {{authenticated:false}};
        if (command === 'central_login') {{ window.__ADMIN_DEMO_SESSION__ = true; return {{user_id:'demo-admin-0001',organization_id:'demo-org-0001',role:'ADMIN'}}; }}
        if (command === 'central_logout') {{ window.__ADMIN_DEMO_SESSION__ = false; return null; }}
        if (command === 'central_admin_request') {{
          const adminPath = args.path || '';
          if ((window.__adminReadFailures || []).includes(adminPath)) throw new Error('Synthetic administration read failure');
          if (args.method !== 'GET') window.__adminWriteLog.push({{path: adminPath, method: args.method, body: args.body}});
          if (window.__rejectDuplicateUser && adminPath === '/v1/admin/users' && args.method === 'POST') return {{status:409,body:{{code:'USER_IDENTITY_EXISTS',detail:'identity_ref is empty or already exists in this organization'}}}};
          if (window.__rejectCatalogDraft && adminPath.endsWith('/tariffs') && args.method === 'POST') return {{status:422,body:{{code:'DEMO_VALIDATION_ERROR',message:'Validation refusal simulated'}}}};
          let adminBody = [];
          if (adminPath === '/v1/admin/users') adminBody = [{{id:'demo-admin-0001',identity_ref:'admin-demo',role:'ADMIN',status:'ACTIVE'}},{{id:'demo-agent-0001',identity_ref:'agent-demo',role:'BILLING_AGENT',status:'ACTIVE'}}];
          else if (adminPath === '/v1/admin/audit-events') adminBody = [{{id:'demo-audit-0001',event_type:'USER_CREATED',actor_identity:'admin-demo',subject_id:'demo-agent-0001',occurred_at:'2026-11-04T08:10:00Z',redacted_metadata:{{role:'BILLING_AGENT'}}}}];
          else if (adminPath === '/v1/admin/catalog/tariffs') adminBody = [{{tariff_id:'demo-tariff-draft',service_code:'WATER',customer_category:'DEMO',unit:'M3',unit_price:'0.30',currency:'USD',effective_from:'2026-01-01',status:'DRAFT'}},{{tariff_id:'demo-tariff-approved',service_code:'ELECTRICITY',customer_category:'DEMO',unit:'KWH',unit_price:'0.095',currency:'USD',effective_from:'2026-01-01',approval_ref:'DEMO-APPROVED',status:'APPROVED'}}];
          else if (adminPath.endsWith('/tariffs')) adminBody = [{{service_code:'ELECTRICITY',customer_category:'DEMO',unit_price:'0.095',currency:'USD',status:'APPROVED'}}];
          else if (adminPath.endsWith('/products')) adminBody = fixtures.products.map(product => ({{...product,status:'APPROVED'}}));
          else if (adminPath.endsWith('/tax-profiles')) adminBody = [{{id:'demo-tax-approved',reference_type:'TAX_PROFILE',payload:{{service_code:'VAT',jurisdiction:'SEL',base:'SUBTOTAL',rate:'0.16',source:'DEMO-SEL-APPROVED'}},effective_from:'2026-01-01',status:'APPROVED'}}];
          else if (adminPath.endsWith('/fx-rates')) adminBody = [{{from_currency:'USD',to_currency:'CDF',value:'DEMO',effective_from:'2026-01-01',status:'APPROVED'}}];
          else if (adminPath === '/v1/admin/catalog/communication-charge') adminBody = [{{id:'demo-communication-charge',reference_type:'COMMUNICATION_CHARGE',natural_key:'SEL-COMMUNICATION',payload:{{label:'Ligne de communication — fixture',amount:'10.00',currency:'USD'}},effective_from:'2026-01-01',approval_ref:'DEMO-APPROVED',status:'APPROVED'}}];
          else if (adminPath === '/v1/admin/configuration/readiness') adminBody = {{configured:false,effective_on:'2026-11-11',requirements:[{{key:'electricity_residential',label:'Tarif électricité résidentiel',ready:false,approved_current_count:0}},{{key:'electricity_commercial',label:'Tarif électricité commercial',ready:true,approved_current_count:1}},{{key:'water_residential',label:'Tarif eau résidentiel',ready:true,approved_current_count:1}},{{key:'water_commercial',label:'Tarif eau commercial',ready:false,approved_current_count:2}},{{key:'vat_policy',label:'Règle centrale de TVA',ready:true,approved_current_count:1}},{{key:'communication_charge',label:'Ligne de communication',ready:false,approved_current_count:0}}]}};
          return {{status:200,body:adminBody}};
        }}
        if (command !== 'central_request') throw new Error('Unexpected visual smoke command: ' + command);
        const path = args.path || '';
        window.__centralRequestLog = window.__centralRequestLog || [];
        window.__centralRequestLog.push({{path,method:args.method}});
        if (args.method !== 'GET') window.__centralWriteLog.push({{path,method:args.method,body:args.body}});
        let body = [];
        if (path === '/v1/receivables') {{ body = {{receivables:fixtures.receivables,metrics:{{payment_count:1,receivable_count:3,open_installment_count:2,overdue_count:0}}}}; }}
        else if (path.startsWith('/v1/receivables/')) {{ const receivableId = path.split('/')[3]; const item = fixtures.receivables.find((entry) => entry.receivable_id === receivableId); body = item ? {{...item,id:item.receivable_id}} : {{}}; }}
        else if (path.endsWith('/statement')) {{
          const customerId = path.split('/')[3];
          if (customerId === window.__delayStatementForCustomer) {{
            window.__statementRequestStarted = true;
            await new Promise(resolve => {{ window.__releaseCustomerStatement = resolve; }});
          }}
          body = customerId === 'demo-customer-0001'
            ? {{invoices:[{{invoice_id:'demo-invoice-customer-0001',invoice_number:'STATEMENT-CUSTOMER-A',billing_period:'2026-10',status:'CENTRAL_ACCEPTED'}}],payments:[],receivables:[{{receivable_id:'demo-receivable-0001',balance:'19.62',currency:'USD',status:'OPEN',source_type:'INVOICE',source_reference:'SEL-20261103-103000'}},{{receivable_id:'demo-receivable-solar-0001',balance:'120.00',currency:'USD',status:'OPEN',source_type:'SALE',source_reference:'demo-sale-0001'}}]}}
            : customerId === 'demo-customer-0002'
              ? {{invoices:[{{invoice_id:'demo-invoice-customer-0002',invoice_number:'STATEMENT-CUSTOMER-B',billing_period:'2026-11',status:'CENTRAL_ACCEPTED'}}],payments:[],receivables:[{{receivable_id:'demo-receivable-0002',balance:'8.00',currency:'USD',status:'OPEN',source_type:'INVOICE'}}]}}
              : {{invoices:[],payments:[],receivables:[]}};
        }}
        else if (path === '/v1/customers' && args.method === 'POST') {{ body = {{...args.body,customer_id:'demo-customer-created-ux3',status:'ACTIVE',revision:1}}; }}
        else if (path.startsWith('/v1/customers/') && args.method === 'PATCH') body = {{...fixtures.customers[0],...args.body,revision:2}};
        else if (path.startsWith('/v1/customers/') && path.endsWith('/service-accounts') && args.method === 'POST') {{ const customerId = path.split('/')[3]; body = {{...args.body,id:'demo-service-account-added',customer_id:customerId,revision:1}}; fixtures.serviceAccounts.push(body); }}
        else if (path.startsWith('/v1/service-accounts/') && path.endsWith('/meters') && args.method === 'POST') {{ const serviceAccountId = path.split('/')[3]; body = {{...args.body,id:'demo-meter-added',service_account_id:serviceAccountId,active:true}}; fixtures.meters.push(body); }}
        else if (path.startsWith('/v1/service-accounts/') && args.method === 'PATCH') {{ const accountId = path.split('/')[3]; const account = fixtures.serviceAccounts.find((item) => item.id === accountId); if (account) Object.assign(account,args.body,{{revision:2}}); body = account ?? {{...args.body,id:accountId,revision:2}}; }}
        else if (path.startsWith('/v1/customers?') || path === '/v1/customers') body = fixtures.customers;
        else if (path === '/v1/catalog/tariffs') body = fixtures.tariffs;
        else if (path === '/v1/billing/utility/preview') {{
          const waterQuantity = Number(args.body?.water?.quantity ?? 0);
          body = waterQuantity > 0
            ? {{currency:'USD',subtotal:'21.00',tax:'3.36',total:'24.36',lines:[{{service_code:'ELECTRICITY',quantity:'100.00',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},{{service_code:'WATER',quantity:'5.00',unit:'M3',unit_price:'0.30',subtotal:'1.50',tax:'0.24',total:'1.74'}},{{service_code:'COMMUNICATION',quantity:'1',unit:'FORFAIT',unit_price:'10.00',subtotal:'10.00',tax:'1.60',total:'11.60'}}]}}
            : {{currency:'USD',subtotal:'19.50',tax:'3.12',total:'22.62',lines:[{{service_code:'ELECTRICITY',quantity:'100.00',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},{{service_code:'WATER',quantity:'0.00',unit:'M3',unit_price:'0.00',subtotal:'0.00',tax:'0.00',total:'0.00'}},{{service_code:'COMMUNICATION',quantity:'1',unit:'FORFAIT',unit_price:'10.00',subtotal:'10.00',tax:'1.60',total:'11.60'}}]}};
        }}
        else if (path === '/v1/billing/utility/issue') {{
          window.__invoiceIssueRequestStarted = true;
          if (window.__delayInvoiceIssue) await new Promise(resolve => {{ window.__releaseInvoiceIssue = resolve; }});
          body = {{invoice_id:'demo-invoice-issued-ux3',invoice_number:'SEL-DEMO-ISSUED',status:'CENTRAL_ACCEPTED',billing_period:args.body?.billing_period}};
          fixtures.invoices.unshift(body);
        }}
        else if (path.startsWith('/v1/customers/') && path.endsWith('/statement')) {{ const customerId = path.split('/')[3]; body = customerId === 'demo-customer-0001' ? {{invoices:fixtures.invoices,receivables:[{{receivable_id:'demo-receivable-0001',balance:'19.62',currency:'USD',status:'OPEN',source_type:'INVOICE',source_reference:'SEL-20261103-103000'}},{{receivable_id:'demo-receivable-solar-0001',balance:'120.00',currency:'USD',status:'OPEN',source_type:'SALE',source_reference:'demo-sale-0001'}}]}} : {{invoices:[],receivables:[]}}; }}
        else if (path.startsWith('/v1/customers/') && path.endsWith('/service-accounts')) {{ const customerId = path.split('/')[3]; body = fixtures.serviceAccounts.filter((account) => account.customer_id === customerId); }}
        else if (path.startsWith('/v1/service-accounts/') && path.endsWith('/meters')) {{ const serviceAccountId = path.split('/')[3]; body = fixtures.meters.filter((meter) => meter.service_account_id === serviceAccountId); }}
        else if (path.startsWith('/v1/meters/') && path.endsWith('/replace')) {{
          const meterId = path.split('/')[3];
          const oldMeter = fixtures.meters.find((meter) => meter.id === meterId);
          if (!oldMeter || !oldMeter.active) return {{status:409,body:{{code:'METER_NOT_ACTIVE'}}}};
          oldMeter.active = false;
          const replacement = {{id:'demo-meter-replacement-0001',service_account_id:oldMeter.service_account_id,serial:args.body.serial,unit:oldMeter.unit,active:true}};
          fixtures.meters.push(replacement);
          body = {{retired_meter:oldMeter,replacement_meter:replacement}};
        }}
        else if (path.startsWith('/v1/meters/') && path.endsWith('/readings/preview')) {{ const meterId = path.split('/')[3]; const previous = Number(args.body?.previous ?? 0); const current = Number(args.body?.current ?? 0); body = {{meter_id:meterId,period:args.body?.period ?? '2026-11',previous:String(previous),current:String(current),measured_quantity:String(current - previous),source:'MANUAL'}}; }}
        else if (path.startsWith('/v1/meters/') && path.endsWith('/readings')) {{ const meterId = path.split('/')[3]; body = fixtures.readings.filter((reading) => reading.meter_id === meterId); }}
        else if (path.startsWith('/v1/customers/')) body = fixtures.customers[0];
        else if (path.startsWith('/v1/invoices/') && path.endsWith('/document-data')) {{
          const positiveWater = window.__invoicePdfWaterPositive === true;
          const invoiceLines = [
            {{service_code:'ELECTRICITY',quantity:'100',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},
            positiveWater
              ? {{service_code:'WATER',quantity:'5',unit:'M3',unit_price:'0.30',subtotal:'1.50',tax:'0.24',total:'1.74'}}
              : {{service_code:'WATER',quantity:'0',unit:'M3',unit_price:'0',subtotal:'0',tax:'0',total:'0'}},
            {{service_code:'COMMUNICATION',quantity:'1',unit:'FORFAIT',unit_price:'10.00',subtotal:'10.00',tax:'1.60',total:'11.60'}}
          ];
          body = {{document_schema_version:1,template_version:'v2-a4-2',document_reference:'SEL-DÉMO-0001',qr_internal_reference:'SEL-V2-demo-invoice-0001',qr_payload:JSON.stringify({{schema:'SEL-INVOICE',version:1,issuer:{{code:'SEL',name:'Société d’Énergie du Lualaba'}},invoice:{{id:'demo-invoice-0001',reference:'SEL-DÉMO-0001',issued_at:'2026-11-03T10:30:00Z',period:'2026-10',due_date:'2026-11-15'}},customer:{{code:'SEL-DÉMO-0001'}},currency:'USD',lines:invoiceLines,totals:{{subtotal:positiveWater?'21.00':'19.50',tax:positiveWater?'3.36':'3.12',total:positiveWater?'24.36':'22.62'}},notice:'Identification interne SEL; sans authentification fiscale'}}),origin:'CENTRAL',issued_at:'2026-11-03T10:30:00Z',identity_snapshot:{{organization_name:'Société d’Énergie du Lualaba',customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi',phone:'+243 000 000 000'}},invoice:{{...fixtures.invoices[0],invoice_number:'SEL-DÉMO-0001',billing_period:'2026-10',due_date:'2026-11-15',subtotal:positiveWater?'21.00':'19.50',tax:positiveWater?'3.36':'3.12',total:positiveWater?'24.36':'22.62',lines:invoiceLines}}}};
        }}
        else if (path.startsWith('/v1/invoices/')) body = {{...fixtures.invoices[0],lines:[{{service_code:'ELECTRICITY',quantity:'100',unit:'KWH',unit_price:'0.095',subtotal:'9.50',tax:'1.52',total:'11.02'}},{{service_code:'WATER',quantity:'0',unit:'M3',unit_price:'0',subtotal:'0',tax:'0',total:'0'}},{{service_code:'COMMUNICATION',quantity:'1',unit:'FORFAIT',unit_price:'10.00',subtotal:'10.00',tax:'1.60',total:'11.60'}}],subtotal:'19.50',tax:'3.12'}};
        else if (path.startsWith('/v1/invoices')) body = fixtures.invoices;
        else if (path === '/v1/payments' && args.method === 'POST') {{ window.__paymentPostCount += 1; body = {{...fixtures.payments[0],payment_id:`demo-payment-created-${{String(window.__paymentPostCount).padStart(4,'0')}}`,amount_received:args.body?.received_amount}}; fixtures.payments.unshift(body); }}
        else if (path.startsWith('/v1/payments/') && path.endsWith('/document-data')) body = {{document_schema_version:2,document_type:'PAYMENT_RECEIPT',document_reference:'SEL-PAY-DEMO-0001',qr_internal_reference:'SEL:PAYMENT:demo-payment-0001',organization_name:'Société d’Énergie du Lualaba',issued_at:'2026-11-04T08:00:00Z',payment:{{payment_id:'demo-payment-0001',amount_received:'3.00',amount_allocated:'3.00',amount_unallocated:'0.00',currency:'USD',method:'CASH',paid_at:'2026-11-04T08:00:00Z',issued_at:'2026-11-04T08:00:00Z',status:'CENTRAL_ACCEPTED'}},customer:{{customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi'}},allocations:[{{receivable_id:'demo-receivable-0001',source_type:'INVOICE',source_reference:'SEL-20261103-103000',source_description:'',principal:'22.62',paid_before:'0.00',credited_before:'0.00',balance_before:'22.62',amount:'3.00',balance_after:'19.62',has_installment_plan:false,schedule_allocations:[],currency:'USD'}}],reversals:[],origin:'CENTRAL'}};
        else if (path.startsWith('/v1/payments')) body = fixtures.payments;
        else if (path.startsWith('/v1/solar/sales/') && path.endsWith('/document-data')) {{
          const longSchedule = window.__solarDocumentLongSchedule === true
            ? Array.from({{length:36}},(_,index)=>({{sequence:index+1,due_date:`2027-${{String(Math.floor(index/12)+1).padStart(2,'0')}}-${{String((index%12)+1).padStart(2,'0')}}`,amount_due:index===35?'3.75':'3.87',status:'UPCOMING'}}))
            : fixtures.solarPlan.schedule;
          body = {{document_schema_version:1,document_type:'SOLAR_SALE_CONFIRMATION',document_reference:'SEL-SOLAR-DEMO-0001',qr_internal_reference:'SEL:SOLAR-SALE:demo-sale-0001',sale:{{sale_id:'demo-sale-0001',receivable_id:'demo-receivable-solar-0001',customer_id:'demo-customer-0001',subtotal:'120.00',tax_amount:'19.20',principal:'139.20',currency:'USD',sold_at:'2026-11-04T08:00:00Z',status:'CENTRAL_ACCEPTED'}},customer:{{customer_code:'SEL-DÉMO-0001',legal_name:'Client Alpha',address:'Quartier Manika, Kolwezi'}},lines:[{{product_reference:'KIT-DÉMO',quantity:'1',unit_price:'120.00',subtotal:'120.00',tax_snapshot:{{rate:'0.16',amount:'19.20'}},tax_amount:'19.20',total:'139.20'}}],installment_plan:{{currency:'USD',schedule:longSchedule}},origin:'CENTRAL'}};
        }}
        else if (path.startsWith('/v1/solar/sales')) body = fixtures.sales;
        else if (path.startsWith('/v1/catalog/products')) body = fixtures.products;
        else if (path.startsWith('/v1/solar/contracts')) {{ fixtures.sales[0].installment_plan_id = 'demo-plan-0001'; body = {{installment_plan_id:'demo-plan-0001',contract_id:'demo-contract-0001'}}; }}
        else if (path.startsWith('/v1/installment-plans/')) body = fixtures.solarPlan;
        return {{status:200,body}};
      }}}};
    """


async def wait_for_server(process: subprocess.Popen[bytes], url: str) -> None:
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Vite exited before ready with status {process.returncode}")
        try:
            with urllib.request.urlopen(url, timeout=2):
                return
        except Exception:
            time.sleep(0.5)
    raise TimeoutError("Vite did not start within 60 seconds")


async def assert_pdf_rendered(page, label: str, expected_pages: int | None = 1) -> str:
    await page.wait_for_function("document.querySelector('.document-pdf-page[data-render-state=complete]') || (document.querySelector('.document-pdf-status')?.textContent && !document.querySelector('.document-pdf-status').textContent.startsWith('Préparation'))", timeout=15000)
    page_count = await page.locator(".document-pdf-page").count()
    if not page_count:
        status = await page.locator(".document-pdf-status").inner_text()
        raise AssertionError(f"{label} PDF could not be rendered: {status}; failed requests: {FAILED_REQUESTS[-8:]}")
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
            raise AssertionError(f"{label}: expected PDF text {token!r} was missing; extracted PDF text: {text!r}")
    if label.startswith("Invoice"):
        for token in ("QR d’identification", "Données internes de la facture"):
            if token.casefold() not in text.casefold():
                raise AssertionError(f"{label}: expected invoice QR label {token!r} was missing")
        if "Confirmé par le serveur".casefold() in text.casefold():
            raise AssertionError(f"{label}: invoice still exposes an internal synchronization state")
    elif "Référence interne SEL" not in text or "sans authentification fiscale" not in text:
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
    with socket.socket() as port_probe:
        port_probe.bind(("127.0.0.1", 0))
        port = port_probe.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    process = subprocess.Popen(
        ["npm.cmd", "run", "dev", "--", "--host", "127.0.0.1", "--port", str(port), "--strictPort"],
        cwd=APP,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    try:
        await wait_for_server(process, url)
        async with async_playwright() as playwright:
            browser_channel = os.environ.get("SEL_VISUAL_BROWSER")
            browser_options = {"headless": True}
            if browser_channel:
                browser_options["channel"] = browser_channel
            browser = await playwright.chromium.launch(**browser_options)
            page = await browser.new_page(viewport={"width": VIEWPORT_WIDTH, "height": VIEWPORT_HEIGHT}, device_scale_factor=1, locale="fr-CD")
            browser_errors: list[str] = []
            FAILED_REQUESTS.clear()
            page.on("pageerror", lambda error: browser_errors.append(str(error)))
            page.on("requestfailed", lambda request: FAILED_REQUESTS.append(f"{request.url}: {request.failure}"))
            await page.add_init_script(mock_invoke_script())
            await page.add_init_script("""(() => {
              const original = Blob.prototype.arrayBuffer;
              Blob.prototype.arrayBuffer = async function() {
                const bytes = await original.call(this);
                if (this.type.toLowerCase().includes('pdf')) window.__SEL_PDF_CAPTURE = Array.from(new Uint8Array(bytes));
                return bytes;
              };
            })();""")
            await page.goto(url, wait_until="domcontentloaded")
            await page.get_by_role("heading", name="Connexion à SEL Facturation").wait_for()
            await page.screenshot(path=str(OUTPUT / f"login-{SHOT_SUFFIX}.png"))
            await page.get_by_label("Identifiant SEL").fill("agent-demo")
            await page.get_by_label("Mot de passe").fill("synthetic-password")
            await page.get_by_role("button", name="Se connecter").click()
            try:
                await page.get_by_role("heading", name="Bienvenue dans SEL Facturation").wait_for(timeout=10000)
            except Exception as error:
                await page.screenshot(path=str(OUTPUT / "login-after-submit-diagnostic.png"))
                body = (await page.locator("body").inner_text())[:2000]
                raise AssertionError(f"Dashboard did not open after synthetic login. Page errors: {browser_errors}; body: {body}") from error
            views = [
                ("dashboard", "Accueil", None),
                ("customers", "Abonnés", None),
                ("readings", "Facturation", "Relevés"),
                ("invoices", "Facturation", "Factures"),
                ("payments", "Paiements", "Journal de caisse"),
                ("solar", "Solaire", None),
                ("receivables", "Paiements", "Créances"),
                ("server", "Administration", "État du serveur"),
                ("admin", "Administration", None),
            ]
            requested_views = {item.strip() for item in os.environ.get("SEL_VISUAL_VIEWS", "").split(",") if item.strip()}
            if requested_views:
                views = [item for item in views if item[0] in requested_views]
            for slug, domain_label, page_label in views:
                await page.get_by_role("navigation", name="Navigation principale").get_by_role(
                    "button", name=domain_label, exact=True
                ).click()
                pending_confirmation = page.get_by_role("alertdialog")
                try:
                    await pending_confirmation.wait_for(timeout=500)
                except Exception:
                    pass
                if await pending_confirmation.count():
                    await pending_confirmation.get_by_role("button", name="Abandonner et continuer").click()
                if slug == "server":
                    await page.get_by_role("button", name="État du serveur · diagnostic").click()
                elif page_label:
                    await page.locator(".workspace-tabs").get_by_role("button", name=page_label, exact=True).click()
                if slug == "customers":
                    await page.get_by_role("row").filter(has_text="SEL-DÉMO-0001").get_by_role("button", name="Ouvrir le dossier").click()
                    await page.get_by_text("STATEMENT-CUSTOMER-A", exact=True).wait_for()
                    await page.get_by_role("button", name="Relevés", exact=True).click()
                    await page.locator(".reading-filters select").nth(0).select_option("demo-customer-0002")
                    await page.evaluate("window.__delayStatementForCustomer = 'demo-customer-0002'")
                    await page.locator(".service-empty-state").get_by_role("button", name="Ouvrir le dossier abonné").click()
                    await page.wait_for_function("window.__statementRequestStarted === true")
                    dossier_statement = page.locator(".panel").filter(has_text="Situation du compte")
                    stale_invoice = await dossier_statement.get_by_text("STATEMENT-CUSTOMER-A", exact=True).count()
                    if stale_invoice:
                        raise AssertionError("A previous customer's statement remained visible while the next customer's statement was loading")
                    await page.evaluate("window.__releaseCustomerStatement?.()")
                    await page.get_by_text("STATEMENT-CUSTOMER-B", exact=True).wait_for()
                    await page.screenshot(path=str(OUTPUT / f"customer-statement-switch-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Retour au répertoire").click()
                if slug == "readings":
                    selects = page.locator(".reading-filters select")
                    await selects.nth(0).select_option("demo-customer-0001")
                    await selects.nth(1).select_option("demo-service-account-0001")
                    await page.locator('.reading-filters select option[value="demo-meter-replacement-0001"]').wait_for(state="attached")
                    await selects.nth(2).select_option("demo-meter-replacement-0001")
                    await page.locator('.reading-entry input[name="period"]').fill("2026-11")
                    await page.locator('.reading-entry input[name="previous"]').fill("0")
                    await page.locator('.reading-entry input[name="current"]').fill("100")
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
                    await page.screenshot(path=str(OUTPUT / f"water-reading-preview-{SHOT_SUFFIX}.png"))
                if slug == "payments":
                    await page.get_by_role("heading", name="Journal de caisse").wait_for()
                    if await page.locator(".payment-journal-metrics .metric").count() != 4:
                        raise AssertionError("Payment journal must show four factual metrics")
                    await page.screenshot(path=str(OUTPUT / f"payment-journal-f01-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Enregistrer un paiement", exact=True).first.click()
                    await page.get_by_role("heading", name="Enregistrer un paiement").wait_for()
                    if await page.locator(".payment-entry-metrics .metric").count() != 4:
                        raise AssertionError("Payment entry must show four server-derived summary metrics")
                    await page.screenshot(path=str(OUTPUT / f"payment-entry-f02-{SHOT_SUFFIX}.png"))
                    await page.locator('.payment-workspace form select[name="customer_id"]').select_option("demo-customer-0001")
                    await page.locator('input[name="received_amount"]').fill("3.00")
                    await page.locator('input[name="occurred_at"]').fill("2026-11-04T10:00")
                    await page.get_by_role("button", name="Affecter un paiement aux créances").click()
                    await page.get_by_role("heading", name="Affecter un paiement aux créances").wait_for()
                    await page.locator('.allocation-card select option[value="demo-receivable-0001"]').wait_for(state="attached")
                    await page.locator('.allocation-card select').first.select_option("demo-receivable-0001")
                    await page.locator('.allocation-card input[type="number"]').fill("3.00")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"payment-allocation-f03-{SHOT_SUFFIX}.png"))
                    await page.locator(".page-heading > button").click()
                    await page.get_by_role("heading", name="Confirmer le règlement").wait_for()
                    await page.screenshot(path=str(OUTPUT / f"payment-confirmation-f04-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__paymentPostCount") != 0:
                        raise AssertionError("Payment review step must not post a payment to the server")
                    await page.get_by_role("button", name="Confirmer et enregistrer").click()
                    await page.wait_for_function("window.__paymentPostCount === 1")
                if slug == "solar":
                    await page.get_by_role("button", name="Créer une vente").click()
                    await page.locator('form[data-preserve-draft] select[name="customer_id"]').select_option("demo-customer-0001")
                    await page.get_by_role("button", name="Contrats et échéanciers").click()
                    solar_draft_review = page.get_by_role("alertdialog", name="Abandonner la saisie solaire ?")
                    await solar_draft_review.wait_for()
                    await solar_draft_review.get_by_role("button", name="Annuler").click()
                    if not await page.locator('form[data-preserve-draft]').count():
                        raise AssertionError("Canceling solar workspace navigation must preserve the open sale form")
                    await page.get_by_role("button", name="Contrats et échéanciers").click()
                    await page.get_by_role("alertdialog", name="Abandonner la saisie solaire ?").get_by_role("button", name="Abandonner et continuer").click()
                    await page.get_by_role("heading", name="Contrats et échéanciers solaires").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"solar-contract-workspace-{SHOT_SUFFIX}.png"))
                    await page.get_by_label("Vente à financer").select_option("demo-sale-0001")
                    await page.get_by_label("Échéance 1 · Date").fill("2026-12-01")
                    await page.get_by_label("Montant dû").fill("60.00")
                    await page.get_by_role("button", name="+ Ajouter une échéance").click()
                    await page.get_by_label("Échéance 2 · Date").fill("2027-01-01")
                    await page.locator("label", has_text="Montant dû").nth(1).locator("input").fill("60.00")
                    await page.get_by_label("Référence des conditions approuvées").fill("DEMO-SOLAR-TERMS")
                    await page.get_by_role("button", name="Vérifier le contrat").click()
                    contract_review = page.get_by_role("alertdialog", name="Vérifier le contrat solaire")
                    await contract_review.wait_for()
                    contract_review_text = await contract_review.inner_text()
                    if (
                        "demo-sale-0001" not in contract_review_text
                        or "créance existante" not in contract_review_text
                        or "Échéancier" not in contract_review_text
                        or "DEMO-SOLAR-TERMS" not in contract_review_text
                        or "Aucun intérêt ni frais supplémentaire" not in contract_review_text
                    ):
                        raise AssertionError("Solar contract review must show its existing debt, schedule policy and terms")
                    await page.screenshot(path=str(OUTPUT / f"solar-contract-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.path === '/v1/solar/contracts' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Solar contract must not be created before explicit confirmation")
                    await contract_review.get_by_role("button", name="Annuler").click()
                    if await page.get_by_label("Référence des conditions approuvées").input_value() != "DEMO-SOLAR-TERMS":
                        raise AssertionError("Canceling solar contract review must preserve the entered terms reference")
                    await page.get_by_role("button", name="Vérifier le contrat").click()
                    await page.get_by_role("alertdialog", name="Vérifier le contrat solaire").get_by_role("button", name="Créer le contrat et l’échéancier").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.path === '/v1/solar/contracts' && entry.method === 'POST').length === 1")
                    await page.get_by_role("heading", name="Échéancier consulté").wait_for()
                    await page.get_by_role("heading", name="Contrats et échéanciers solaires").evaluate(
                        "element => element.scrollIntoView({ block: 'start' })"
                    )
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"solar-contracts-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Enregistrer un versement").click()
                    await page.get_by_role("heading", name="Enregistrer un paiement").wait_for()
                    await page.wait_for_function("document.querySelector('#central-payment-form select[name=customer_id]')?.value === 'demo-customer-0001'")
                    await page.locator('input[name="received_amount"]').fill("10.00")
                    await page.get_by_role("button", name="Affecter un paiement aux créances").click()
                    await page.get_by_role("heading", name="Affecter un paiement aux créances").wait_for()
                    if await page.locator(".allocation-card select").first.input_value() != "demo-receivable-solar-0001":
                        raise AssertionError("Solar plan payment shortcut must preselect its original receivable")
                    if await page.locator(".allocation-card select").nth(1).input_value() != "demo-solar-schedule-0001":
                        raise AssertionError("Solar plan payment shortcut must preselect the next open installment")
                    await page.screenshot(path=str(OUTPUT / f"solar-payment-prefill-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("navigation", name="Navigation principale").get_by_role("button", name="Solaire", exact=True).click()
                    solar_payment_draft = page.get_by_role("alertdialog", name="Abandonner la saisie ?")
                    if await solar_payment_draft.count():
                        await solar_payment_draft.get_by_role("button", name="Abandonner et continuer").click()
                    await page.locator(".workspace-tabs").get_by_role("button", name="Catalogue et ventes", exact=True).click()
                    await page.get_by_role("heading", name="Catalogue des kits solaires").wait_for()
                await page.evaluate("window.scrollTo(0, 0)")
                await page.wait_for_timeout(120)
                await page.screenshot(path=str(OUTPUT / f"{slug}-{SHOT_SUFFIX}.png"))
                if slug == "customers":
                    await page.get_by_role("button", name="+ Ajouter un abonné").click()
                    await page.get_by_role("heading", name="Créer un abonné").wait_for()
                    customer_form = page.locator("form.customer-create-form")
                    await customer_form.locator('input[name="customer_code"]').fill("SEL-DÉMO-CRÉATION")
                    await customer_form.locator('input[name="legal_name"]').fill("Client de test UX3")
                    await customer_form.locator('select[name="customer_category"]').select_option("RESIDENTIAL")
                    await customer_form.locator('input[name="address"]').fill("Quartier de démonstration")
                    await customer_form.locator('input[name="phone"]').fill("+243000000099")
                    await customer_form.get_by_role("button", name="Vérifier la création").click()
                    customer_review = page.get_by_role("alertdialog", name="Vérifier le nouvel abonné")
                    await customer_review.wait_for()
                    customer_review_text = await customer_review.inner_text()
                    if "SEL-DÉMO-CRÉATION" not in customer_review_text or "Client de test UX3" not in customer_review_text or "Résidentiel" not in customer_review_text or "Actif" not in customer_review_text:
                        raise AssertionError("Customer review must summarize the entered customer and the API-defined initial state")
                    await page.screenshot(path=str(OUTPUT / f"customer-create-confirmation-{SHOT_SUFFIX}.png"))
                    await customer_review.get_by_role("button", name="Annuler").click()
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.path === '/v1/customers' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Canceling customer creation must not send a write request")
                    if await customer_form.locator('input[name="customer_code"]').input_value() != "SEL-DÉMO-CRÉATION":
                        raise AssertionError("Canceling customer review must preserve the entered customer data")
                    await customer_form.get_by_role("button", name="Vérifier la création").click()
                    await page.get_by_role("alertdialog", name="Vérifier le nouvel abonné").get_by_role("button", name="Créer l’abonné").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.path === '/v1/customers' && entry.method === 'POST').length === 1")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"customer-new-{SHOT_SUFFIX}.png"))
                    close_customer_form = page.get_by_role("button", name="Fermer le formulaire")
                    if await close_customer_form.count():
                        await close_customer_form.click()
                    await page.get_by_role("button", name="Ouvrir le dossier →").first.click()
                    await page.get_by_role("heading", name="Dossier abonné").wait_for()
                    await page.get_by_role("heading", name="Situation du compte · Client Alpha").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"customer-record-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Modifier la fiche").click()
                    await page.get_by_label("Nom / raison sociale").wait_for()
                    await page.get_by_label("Catégorie tarifaire").select_option("RESIDENTIAL")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"customer-edit-{SHOT_SUFFIX}.png"))
                    await page.get_by_label("Nom / raison sociale").fill("Client Alpha modifié")
                    await page.get_by_role("button", name="Vérifier les modifications").click()
                    edit_review = page.get_by_role("alertdialog", name="Vérifier les modifications")
                    await edit_review.wait_for()
                    if "Client Alpha modifié" not in await edit_review.inner_text():
                        raise AssertionError("Customer edit review must summarize the submitted values")
                    await page.screenshot(path=str(OUTPUT / f"customer-edit-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path.includes('/v1/customers/')).length") != 0:
                        raise AssertionError("Customer edits must not be sent before confirmation")
                    await edit_review.get_by_role("button", name="Annuler").click()
                    if await page.get_by_label("Nom / raison sociale").input_value() != "Client Alpha modifié":
                        raise AssertionError("Canceling customer edit review must preserve the entered values")
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path.includes('/v1/customers/')).length") != 0:
                        raise AssertionError("Canceling customer edit review must not send a write request")
                    await page.get_by_role("button", name="Vérifier les modifications").click()
                    await page.get_by_role("alertdialog", name="Vérifier les modifications").get_by_role("button", name="Enregistrer les modifications").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path.includes('/v1/customers/')).length === 1")
                    archive_button = page.get_by_role("button", name="Archiver", exact=True)
                    await archive_button.click()
                    confirmation = page.get_by_role("alertdialog", name="Archiver cet abonné ?")
                    await confirmation.wait_for()
                    await page.screenshot(path=str(OUTPUT / f"customer-archive-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("document.activeElement?.textContent?.trim()") != "Annuler":
                        raise AssertionError("Opening a confirmation must move keyboard focus into the dialog")
                    await page.keyboard.press("Shift+Tab")
                    if await page.evaluate("document.activeElement?.textContent?.trim()") != "Archiver l’abonné":
                        raise AssertionError("Shift+Tab from the first dialog action must wrap to the last action")
                    await page.keyboard.press("Tab")
                    if await page.evaluate("document.activeElement?.textContent?.trim()") != "Annuler":
                        raise AssertionError("Tab from the last dialog action must wrap to the first action")
                    await page.keyboard.press("Escape")
                    await confirmation.wait_for(state="detached")
                    if await page.evaluate("document.activeElement?.textContent?.trim()") != "Archiver":
                        raise AssertionError("Customer archive confirmation did not restore focus to its trigger")
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path === '/v1/customers/demo-customer-0001' && entry.body?.status === 'ARCHIVED').length") != 0:
                        raise AssertionError("Escape must cancel the archive without sending a write")
                    await archive_button.click()
                    confirmation = page.get_by_role("alertdialog", name="Archiver cet abonné ?")
                    await confirmation.wait_for()
                    await confirmation.get_by_role("button", name="Annuler").click()
                    await confirmation.wait_for(state="detached")
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path === '/v1/customers/demo-customer-0001' && entry.body?.status === 'ARCHIVED').length") != 0:
                        raise AssertionError("Canceling the archive dialog must not send a write")
                    await page.get_by_role("button", name="Remplacer", exact=True).first.click()
                    await page.get_by_label("Nouveau numéro de compteur").fill("DEMO-KWH-REPLACEMENT")
                    await page.get_by_label("Motif du remplacement").fill("Essai UX synthétique")
                    await page.get_by_role("button", name="Remplacer le compteur").click()
                    replacement_confirmation = page.get_by_role("alertdialog", name="Remplacer ce compteur ?")
                    await replacement_confirmation.wait_for()
                    await page.screenshot(path=str(OUTPUT / f"meter-replacement-confirmation-{SHOT_SUFFIX}.png"))
                    await replacement_confirmation.get_by_role("button", name="Confirmer le remplacement").click()
                    await page.get_by_text("Compteur DEMO-KWH-001 remplacé.", exact=False).wait_for()
                    await page.get_by_text("DEMO-KWH-REPLACEMENT", exact=True).wait_for()
                    await page.get_by_role("button", name="Retour au répertoire").click()
                    await page.locator("tbody tr").nth(1).get_by_role("button", name="Ouvrir le dossier →").click()
                    await page.get_by_role("heading", name="Services et compteurs").wait_for()
                    await page.get_by_role("button", name="Rattacher un service").click()
                    await page.get_by_label("Service").wait_for()
                    await page.get_by_role("heading", name="Services et compteurs").evaluate(
                        "element => element.scrollIntoView({ block: 'center' })"
                    )
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"service-account-without-meter-{SHOT_SUFFIX}.png"))
                    await page.get_by_label("Service").select_option("WATER")
                    await page.get_by_label("État du raccordement").select_option("CONNECTED")
                    await page.get_by_role("button", name="Rattacher le service").click()
                    service_review = page.get_by_role("alertdialog", name="Vérifier le rattachement du service")
                    await service_review.wait_for()
                    if "Eau" not in await service_review.inner_text() or "Aucun compteur ne sera créé" not in await service_review.inner_text():
                        raise AssertionError("Service attachment review must clearly show service and no meter creation")
                    await page.screenshot(path=str(OUTPUT / f"service-account-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'POST' && entry.path.endsWith('/service-accounts')).length") != 0:
                        raise AssertionError("Service attachment must not be sent before confirmation")
                    await service_review.get_by_role("button", name="Annuler").click()
                    if await page.get_by_label("Service").input_value() != "WATER":
                        raise AssertionError("Canceling service attachment review must preserve the chosen service")
                    await page.get_by_role("button", name="Rattacher le service").click()
                    await page.get_by_role("alertdialog", name="Vérifier le rattachement du service").get_by_role("button", name="Rattacher le service").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.method === 'POST' && entry.path.endsWith('/service-accounts')).length === 1")
                    await page.get_by_role("button", name="Ajouter un compteur").click()
                    await page.get_by_label("Numéro réel du compteur").fill("DEMO-WATER-002")
                    await page.get_by_role("button", name="Enregistrer le compteur").click()
                    meter_review = page.get_by_role("alertdialog", name="Vérifier le numéro du compteur")
                    await meter_review.wait_for()
                    if "DEMO-WATER-002" not in await meter_review.inner_text() or "m³" not in await meter_review.inner_text():
                        raise AssertionError("Meter confirmation must identify the actual serial and unit")
                    await page.screenshot(path=str(OUTPUT / f"meter-create-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'POST' && entry.path.endsWith('/meters')).length") != 0:
                        raise AssertionError("Meter must not be recorded before confirmation")
                    await meter_review.get_by_role("button", name="Annuler").click()
                    if await page.get_by_label("Numéro réel du compteur").input_value() != "DEMO-WATER-002":
                        raise AssertionError("Canceling meter confirmation must preserve the entered serial")
                    await page.get_by_role("button", name="Enregistrer le compteur").click()
                    await page.get_by_role("alertdialog", name="Vérifier le numéro du compteur").get_by_role("button", name="Enregistrer le compteur").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.method === 'POST' && entry.path.endsWith('/meters')).length === 1")
                    await page.get_by_text("DEMO-WATER-002", exact=True).wait_for()
                    await page.get_by_role("button", name="Modifier le raccordement").click()
                    await page.get_by_label("État du raccordement").select_option("NOT_CONNECTED")
                    await page.get_by_role("button", name="Enregistrer la modification").click()
                    account_review = page.get_by_role("alertdialog", name="Vérifier le raccordement")
                    await account_review.wait_for()
                    await page.screenshot(path=str(OUTPUT / f"service-account-edit-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path.includes('/service-accounts/')).length") != 0:
                        raise AssertionError("Service-account edits must wait for confirmation")
                    await account_review.get_by_role("button", name="Annuler").click()
                    if await page.get_by_label("État du raccordement").input_value() != "NOT_CONNECTED":
                        raise AssertionError("Canceling service-account review must preserve the entered state")
                    await page.get_by_role("button", name="Enregistrer la modification").click()
                    await page.get_by_role("alertdialog", name="Vérifier le raccordement").get_by_role("button", name="Enregistrer le raccordement").click()
                    await page.wait_for_function("window.__centralWriteLog.filter(entry => entry.method === 'PATCH' && entry.path.includes('/service-accounts/')).length === 1")
                if slug == "receivables":
                    await page.get_by_role("heading", name="Suivi des créances").wait_for()
                    await page.locator(".receivable-customer").filter(has_text="Client Alpha").first.wait_for()
                    await page.get_by_text("Client Beta", exact=True).wait_for()
                    if await page.locator(".receivable-metrics .metric").count() != 4:
                        raise AssertionError("Receivables F08 must show the four central server metrics")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"receivables-register-f08-{SHOT_SUFFIX}.png"))
                    await page.locator(".receivable-customer").first.click()
                    writes_before_detail = await page.evaluate("window.__centralWriteLog.length")
                    await page.get_by_role("button", name="› Détail d’une créance").click()
                    await page.locator(".receivable-detail").get_by_text("19,62", exact=False).wait_for()
                    if await page.evaluate("window.__centralWriteLog.length") != writes_before_detail:
                        raise AssertionError("Opening receivable details must be read-only")
                    await page.screenshot(path=str(OUTPUT / f"receivables-detail-f08-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="› Enregistrer un règlement").click()
                    await page.get_by_role("heading", name="Enregistrer un paiement").wait_for()
                    if await page.locator('#central-payment-form select[name="customer_id"]').input_value() != "demo-customer-0001":
                        raise AssertionError("Receivable payment shortcut must preselect its customer")
                    await page.locator('#central-payment-form input[name="received_amount"]').fill("3.00")
                    await page.get_by_role("button", name="Affecter un paiement aux créances").click()
                    await page.wait_for_timeout(500)
                    if await page.locator('.allocation-card select option[value="demo-receivable-0001"]').count() != 1:
                        payment_diagnostic = await page.evaluate("({body:document.body.innerText,requests:window.__centralRequestLog})")
                        await page.screenshot(path=str(OUTPUT / f"receivables-payment-diagnostic-{SHOT_SUFFIX}.png"))
                        raise AssertionError(f"Receivable payment shortcut did not load its selected receivable: {payment_diagnostic}")
                    if await page.locator('.allocation-card select').first.input_value() != "demo-receivable-0001":
                        raise AssertionError("Receivable action must preselect the chosen obligation in payment entry")
                    if not await page.locator(".page-heading > button").is_disabled():
                        raise AssertionError("Payment allocation review must remain disabled until an amount is assigned")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"receivables-payment-prefill-f08-{SHOT_SUFFIX}.png"))
                if slug == "invoices":
                    await page.get_by_role("heading", name="Historique des factures").wait_for()
                    if await page.locator(".invoice-history-metrics .metric").count() != 4:
                        raise AssertionError("Invoice history must show the four server-derived metrics from Figma F07")
                    await page.get_by_role("heading", name="Actions suivantes").wait_for()
                    await page.get_by_role("button", name="Détail d’une facture").wait_for()
                    await page.screenshot(path=str(OUTPUT / f"invoice-history-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Nouvelle facture", exact=True).click()
                    await page.get_by_role("heading", name="Émettre une facture combinée").wait_for()
                    await page.screenshot(path=str(OUTPUT / f"invoice-issue-f05-{SHOT_SUFFIX}.png"))
                    await page.locator('select[name="customer_id"]').select_option("demo-customer-0001")
                    await page.get_by_label("Période de facturation").fill("2026-10")
                    await page.get_by_label("Électricité consommée (kWh)").fill("100")
                    await page.get_by_label("Eau consommée (m³)").fill("0")
                    await page.get_by_label("Je confirme que le relevé d’eau est exactement de 0 m³.").check()
                    await page.get_by_role("button", name="Prévisualiser la facture").click()
                    await page.get_by_role("heading", name="Vérifier une facture").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"invoice-verification-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Corriger la saisie").first.click()
                    if await page.locator('input[name="electricity_quantity"]').input_value() != "100":
                        raise AssertionError("Invoice input was not preserved when returning from the preview")
                    await page.get_by_label("Eau consommée (m³)").fill("5")
                    await page.get_by_role("button", name="Prévisualiser la facture").click()
                    await page.get_by_role("heading", name="Vérifier une facture").wait_for()
                    await page.locator(".invoice-verification-layout").get_by_text("5 m³", exact=True).wait_for()
                    await page.locator(".invoice-preview-metrics").get_by_text("24,36 USD", exact=True).wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"invoice-verification-water-positive-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Retour à l’historique", exact=True).click()
                    await page.get_by_role("alertdialog", name="Abandonner la facture ?").wait_for()
                    await page.get_by_role("alertdialog").get_by_role("button", name="Abandonner et continuer").click()
                    await page.get_by_role("heading", name="Historique des factures").wait_for()
                    await page.get_by_role("button", name="Document PDF").first.click()
                    try:
                        await page.get_by_role("dialog", name="Facture eau + électricité").wait_for(timeout=5000)
                    except Exception as error:
                        notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                        raise AssertionError(f"Invoice PDF preview did not open: {notice}") from error
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-diagnostic.png"))
                    zero_water_pdf = await assert_pdf_rendered(page, "Invoice with 0 m³ of water")
                    await assert_generated_pdf_contract(page, "Invoice with 0 m³ of water", 1, ("Électricité", "Eau", "0 m³", "Ligne de communication", "22,62 USD", "FACTURE ÉMISE", "Facture émise par la Société d’Énergie du Lualaba, SEL"))
                    await assert_print_layout(page, "Invoice with 0 m³ of water", 1)
                    await page.screenshot(path=str(OUTPUT / f"invoice-pdf-preview-{SHOT_SUFFIX}.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-lower-zero-water.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__invoicePdfWaterPositive = true")
                    await page.get_by_role("button", name="Document PDF").first.click()
                    await page.get_by_role("dialog", name="Facture eau + électricité").wait_for()
                    positive_water_pdf = await assert_pdf_rendered(page, "Invoice with 5 m³ of water")
                    await assert_generated_pdf_contract(page, "Invoice with 5 m³ of water", 1, ("Électricité", "Eau", "5 m³", "Ligne de communication", "24,36 USD", "FACTURE ÉMISE", "Facture émise par la Société d’Énergie du Lualaba, SEL"))
                    await assert_print_layout(page, "Invoice with 5 m³ of water", 1)
                    if positive_water_pdf == zero_water_pdf:
                        raise AssertionError("0 m³ and 5 m³ invoice PDFs rendered identically")
                    await page.screenshot(path=str(OUTPUT / f"invoice-water-positive-pdf-preview-{SHOT_SUFFIX}.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / "invoice-pdf-lower-water-positive.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.get_by_role("heading", name="Historique des factures").wait_for()
                    await page.get_by_role("button", name="Nouvelle facture", exact=True).click()
                    await page.get_by_role("heading", name="Émettre une facture combinée").wait_for()
                    await page.locator('select[name="customer_id"]').select_option("demo-customer-0001")
                    await page.get_by_label("Période de facturation").fill("2026-10")
                    await page.get_by_label("Électricité consommée (kWh)").fill("100")
                    await page.get_by_label("Eau consommée (m³)").fill("0")
                    await page.get_by_label("Je confirme que le relevé d’eau est exactement de 0 m³.").check()
                    await page.get_by_role("button", name="Prévisualiser la facture").click()
                    await page.get_by_role("heading", name="Vérifier une facture").wait_for()
                    await page.evaluate("window.__delayInvoiceIssue = true; window.__invoiceIssueRequestStarted = false")
                    await page.get_by_role("button", name="Confirmer l’émission").click()
                    await page.wait_for_function("window.__invoiceIssueRequestStarted === true")
                    issue_button = page.get_by_role("button", name="Émission en cours…")
                    await issue_button.dispatch_event("click")
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.path === '/v1/billing/utility/issue' && entry.method === 'POST').length") != 1:
                        raise AssertionError("Repeated confirmation created more than one invoice issue request")
                    await page.evaluate("window.__releaseInvoiceIssue?.()")
                    await page.get_by_role("heading", name="Historique des factures").wait_for()
                    if await page.evaluate("window.__centralWriteLog.filter(entry => entry.path === '/v1/billing/utility/issue' && entry.method === 'POST').length") != 1:
                        raise AssertionError("Invoice issue was submitted more than once")
                if slug == "solar":
                    await page.get_by_role("button", name="Catalogue et ventes").click()
                    await page.get_by_role("button", name="Confirmation PDF").first.click()
                    try:
                        await page.get_by_role("dialog", name="Facture de vente solaire").wait_for(timeout=5000)
                    except Exception as error:
                        notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                        raise AssertionError(f"Solar PDF preview did not open: {notice}") from error
                    await assert_pdf_rendered(page, "Solar sale")
                    await assert_generated_pdf_contract(page, "Solar sale", 1, ("Facture de vente", "KIT-DÉMO", "Sous-total HT", "TVA (16", "139,20 USD"))
                    await assert_print_layout(page, "Solar sale", 1)
                    await page.screenshot(path=str(OUTPUT / f"solar-pdf-preview-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__solarDocumentLongSchedule = true")
                    await page.get_by_role("button", name="Confirmation PDF").first.click()
                    await page.get_by_role("dialog", name="Facture de vente solaire").wait_for()
                    await assert_pdf_rendered(page, "Solar sale with 36 installments", expected_pages=None)
                    long_solar_pages = await page.locator(".document-pdf-page").count()
                    long_solar_pdf = await assert_generated_pdf_contract(page, "Solar sale with 36 installments", long_solar_pages, ("Facture de vente", "KIT-DÉMO", "Sous-total HT", "TVA (16", "139,20 USD", "ÉCHÉANCES", "mars 2027"))
                    last_solar_page = long_solar_pdf.pages[-1].extract_text() or ""
                    if len(long_solar_pdf.pages) not in (2, 3) or "SYNTHÈSE FINANCIÈRE" not in last_solar_page or "Référence interne SEL" not in last_solar_page:
                        raise AssertionError(f"Long solar document must place its financial summary and internal QR reference together on the final page (pages={len(long_solar_pdf.pages)}, final_page={last_solar_page!r})")
                    await assert_print_layout(page, "Solar sale with 36 installments", long_solar_pages)
                    await page.screenshot(path=str(OUTPUT / f"solar-pdf-multipage-{SHOT_SUFFIX}.png"))
                    await page.locator(".document-pdf-stage").evaluate("element => element.scrollTop = element.scrollHeight")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"solar-pdf-multipage-last-page-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Fermer l’aperçu").click()
                    await page.evaluate("window.__solarDocumentLongSchedule = false")
                    await page.get_by_role("button", name="Créer une vente").click()
                    await page.get_by_role("heading", name="Nouvelle vente solaire").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.wait_for_timeout(120)
                    await page.screenshot(path=str(OUTPUT / f"solar-new-sale-{SHOT_SUFFIX}.png"))
                    await page.get_by_role("button", name="Fermer", exact=True).click()
                if slug == "admin":
                    await page.get_by_role("heading", name="Utilisateurs et autorisations").wait_for()
                    await page.get_by_role("cell", name="admin-demo").first.wait_for()
                    await page.get_by_role("heading", name="Référentiels à finaliser").wait_for()
                    await page.get_by_text("Tarif eau commercial").wait_for()
                    admin_nav = page.get_by_role("navigation", name="Navigation Administration")
                    await admin_nav.get_by_role("button", name="TVA", exact=True).wait_for()
                    await page.evaluate("window.__adminReadFailures = ['/v1/admin/catalog/communication-charge', '/v1/admin/configuration/readiness']")
                    await page.get_by_role("button", name="Actualiser les données").click()
                    partial_load_notice = page.get_by_role("alert").filter(has_text="Chargement partiel")
                    await partial_load_notice.wait_for()
                    if "Les données déjà chargées sont conservées." not in await partial_load_notice.inner_text():
                        raise AssertionError("Partial administration refresh must say previously loaded data was preserved")
                    await admin_nav.get_by_role("button", name="Ligne de communication", exact=True).click()
                    await page.get_by_role("heading", name="Ligne de communication").first.wait_for()
                    await page.get_by_role("row").filter(has_text="Ligne de communication — fixture").wait_for()
                    await page.evaluate("window.__adminReadFailures = []")
                    await page.get_by_role("button", name="Actualiser les données").click()
                    await admin_nav.get_by_role("button", name="Utilisateurs et accès", exact=True).click()
                    await page.get_by_role("heading", name="Utilisateurs et autorisations").wait_for()
                    await page.screenshot(path=str(OUTPUT / f"admin-full-{SHOT_SUFFIX}.png"), full_page=True)
                    await page.screenshot(path=str(OUTPUT / f"admin-user-directory-{SHOT_SUFFIX}.png"), full_page=True)
                    await page.get_by_role("button", name="Créer un utilisateur", exact=True).click()
                    await page.get_by_role("heading", name="Créer un utilisateur SEL").wait_for()
                    await page.screenshot(path=str(OUTPUT / f"admin-user-create-a02-{SHOT_SUFFIX}.png"), full_page=True)
                    user_form = page.locator("form.admin-create-user")
                    await user_form.locator('input[name="identity_ref"]').fill("agent-test-ux3")
                    await user_form.locator('select[name="role"]').select_option("CASHIER")
                    await user_form.locator('input[name="password"]').fill("synthetic-secret-123")
                    await user_form.locator('input[name="password_confirm"]').fill("synthetic-secret-mismatch")
                    await user_form.get_by_role("button", name="Vérifier la création").click()
                    await page.get_by_role("alert").filter(has_text="Les deux mots de passe ne correspondent pas.").wait_for()
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Mismatched initial passwords must not send a write request")
                    await user_form.locator('input[name="password_confirm"]').fill("synthetic-secret-123")
                    await page.get_by_role("button", name="Vérifier le nouvel utilisateur").click()
                    user_review = page.get_by_role("alertdialog", name="Vérifier la création du compte")
                    await user_review.wait_for()
                    review_text = await user_review.inner_text()
                    if "agent-test-ux3" not in review_text or "Caissier" not in review_text or "synthetic-secret-123" in review_text:
                        raise AssertionError("User confirmation must show identity and role while keeping the password secret")
                    await page.screenshot(path=str(OUTPUT / f"admin-user-confirmation-{SHOT_SUFFIX}.png"))
                    await user_review.get_by_role("button", name="Annuler").click()
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Canceling user creation must not send a write request")
                    await user_form.get_by_role("button", name="Vérifier la création").click()
                    await page.get_by_role("alertdialog", name="Vérifier la création du compte").get_by_role("button", name="Créer le compte").click()
                    await page.wait_for_function("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users' && entry.method === 'POST').length === 1")
                    await page.get_by_role("button", name="Créer un utilisateur", exact=True).click()
                    duplicate_form = page.locator("form.admin-create-user")
                    await duplicate_form.locator('input[name="identity_ref"]').fill("admin-demo")
                    await duplicate_form.locator('select[name="role"]').select_option("BILLING_AGENT")
                    await duplicate_form.locator('input[name="password"]').fill("synthetic-secret-duplicate")
                    await duplicate_form.locator('input[name="password_confirm"]').fill("synthetic-secret-duplicate")
                    await page.evaluate("window.__rejectDuplicateUser = true")
                    await duplicate_form.get_by_role("button", name="Vérifier la création").click()
                    await page.get_by_role("alertdialog", name="Vérifier la création du compte").get_by_role("button", name="Créer le compte").click()
                    duplicate_notice = page.get_by_role("alert")
                    await duplicate_notice.get_by_text("Cet identifiant SEL est déjà utilisé ou invalide.").wait_for()
                    duplicate_text = await duplicate_notice.inner_text()
                    if "identity_ref is empty" in duplicate_text or "/v1/admin/users" in duplicate_text:
                        raise AssertionError("Duplicate identity conflict must be localized without exposing raw API details")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-duplicate-user-conflict-{SHOT_SUFFIX}.png"), full_page=True)
                    await page.evaluate("window.__rejectDuplicateUser = false")
                    await page.get_by_role("button", name="Revenir au répertoire").click()
                    duplicate_discard = page.get_by_role("alertdialog", name="Abandonner les modifications ?")
                    await duplicate_discard.get_by_role("button", name="Abandonner et continuer").click()
                    agent_row = page.get_by_role("row").filter(has_text="agent-demo")
                    await agent_row.get_by_role("button", name="Voir").click()
                    await page.get_by_role("heading", name="Modifier les accès d’un compte").wait_for()
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-user-security-a03-{SHOT_SUFFIX}.png"), full_page=True)
                    agent_role = page.get_by_label("Nouveau rôle de agent-demo")
                    await agent_role.select_option("SUPERVISOR")
                    await page.get_by_label("Motif du changement").fill("Réaffectation de responsabilités (fixture)")
                    await page.get_by_role("button", name="Vérifier la modification").click()
                    await page.get_by_role("alertdialog", name="Vérifier le changement de rôle").wait_for()
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users/demo-agent-0001' && entry.body.role === 'SUPERVISOR').length") != 0:
                        raise AssertionError("Role updates must wait for explicit confirmation")
                    await page.get_by_role("alertdialog", name="Vérifier le changement de rôle").get_by_role("button", name="Enregistrer le rôle").click()
                    await page.wait_for_function("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users/demo-agent-0001' && entry.body.role === 'SUPERVISOR' && entry.body.reason === 'Réaffectation de responsabilités (fixture)').length === 1")
                    await page.get_by_role("button", name="Retour utilisateurs").click()
                    admin_row = page.get_by_role("row").filter(has_text="admin-demo")
                    await admin_row.get_by_role("button", name="Voir").click()
                    admin_role = page.get_by_label("Nouveau rôle de admin-demo")
                    if not await admin_role.is_disabled():
                        raise AssertionError("The current administrator must not be able to edit their own role")
                    admin_status_action = page.get_by_role("button", name="Désactiver ce compte")
                    if not await admin_status_action.is_disabled():
                        raise AssertionError("The current administrator must not be able to disable their own account")
                    locked_role_button = page.get_by_role("button", name="Vérifier la modification")
                    locked_style = await locked_role_button.evaluate("element => getComputedStyle(element).backgroundColor")
                    if locked_style != "rgb(237, 241, 246)":
                        raise AssertionError("A disabled primary action must not look active")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-self-access-locked-{SHOT_SUFFIX}.png"), full_page=True)
                    await page.get_by_role("button", name="Retour utilisateurs").click()
                    discard_role_review = page.get_by_role("alertdialog", name="Abandonner les modifications ?")
                    if await discard_role_review.count():
                        await discard_role_review.get_by_role("button", name="Abandonner et continuer").click()
                    agent_row = page.get_by_role("row").filter(has_text="agent-demo")
                    await agent_row.get_by_role("button", name="Voir").click()
                    await page.get_by_label("Motif du changement").fill("Fin de mission (fixture)")
                    await page.get_by_role("button", name="Désactiver ce compte").click()
                    await page.get_by_role("alertdialog", name="Désactiver ce compte ?").wait_for()
                    await page.get_by_role("alertdialog", name="Désactiver ce compte ?").get_by_role("button", name="Désactiver le compte").click()
                    await page.wait_for_function("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users/demo-agent-0001' && entry.body.status === 'REVOKED' && entry.body.reason === 'Fin de mission (fixture)').length === 1")
                    await page.get_by_role("button", name="Réinitialiser le mot de passe").click()
                    reset_form = page.locator("form.admin-password-reset")
                    await reset_form.get_by_label("Nouveau mot de passe").fill("synthetic-password-456")
                    await reset_form.get_by_role("button", name="Confirmer le changement").click()
                    password_review = page.get_by_role("alertdialog", name="Vérifier la réinitialisation du mot de passe")
                    await password_review.wait_for()
                    password_path = "/v1/users/demo-agent-0001/password"
                    if await page.evaluate(f"window.__adminWriteLog.filter(entry => entry.path === '{password_path}' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Password reset must wait for explicit confirmation")
                    await password_review.get_by_role("button", name="Annuler").click()
                    if await reset_form.get_by_label("Nouveau mot de passe").input_value() != "synthetic-password-456":
                        raise AssertionError("Canceling password reset review must preserve the entered password")
                    await reset_form.get_by_role("button", name="Confirmer le changement").click()
                    await page.get_by_role("alertdialog", name="Vérifier la réinitialisation du mot de passe").get_by_role("button", name="Confirmer le changement").click()
                    await page.wait_for_function(f"window.__adminWriteLog.filter(entry => entry.path === '{password_path}' && entry.method === 'POST').length === 1")
                    await page.get_by_role("button", name="Retour utilisateurs").click()
                    abandon_admin_review = page.get_by_role("alertdialog", name="Abandonner les modifications ?")
                    if await abandon_admin_review.count():
                        await abandon_admin_review.get_by_role("button", name="Abandonner et continuer").click()
                    await page.get_by_role("button", name="Tarifs et référentiels", exact=True).click()
                    catalog_discard = page.get_by_role("alertdialog", name="Abandonner les modifications ?")
                    try:
                        await catalog_discard.wait_for(timeout=1000)
                    except Exception:
                        pass
                    if await catalog_discard.count():
                        await catalog_discard.get_by_role("button", name="Abandonner et continuer").click()
                    await page.get_by_role("heading", name="Ligne de communication").first.wait_for()
                    await admin_nav.get_by_role("button", name="Tarifs eau et électricité", exact=True).click()
                    try:
                        await page.get_by_role("heading", name="Tarifs eau et électricité").wait_for(timeout=5000)
                    except Exception:
                        await page.screenshot(path=str(OUTPUT / f"admin-catalog-navigation-diagnostic-{SHOT_SUFFIX}.png"), full_page=True)
                        print("Admin catalog navigation diagnostic:", (await page.locator("body").inner_text())[:2500].encode("ascii", "replace").decode())
                        raise
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-tariffs-catalog-a04-{SHOT_SUFFIX}.png"))
                    tariff_panel = page.locator(".admin-reference-layout > .panel").first
                    for catalog_label, catalog_heading in [
                        ("TVA", "Règle centrale de TVA"),
                        ("Taux de change", "Taux de change"),
                        ("Kits solaires", "Catalogue solaire"),
                        ("Ligne de communication", "Ligne de communication"),
                    ]:
                        await admin_nav.get_by_role("button", name=catalog_label, exact=True).click()
                        await page.get_by_role("heading", name=catalog_heading).first.wait_for()
                        if catalog_label == "Taux de change":
                            await page.get_by_role("button", name="Ajouter une version").click()
                            fx_form = page.locator("form.admin-catalog-form")
                            if await fx_form.locator('select[name="from_currency"]').input_value() != "USD" or await fx_form.locator('select[name="to_currency"]').input_value() != "CDF":
                                raise AssertionError("FX catalog must default from the primary USD currency to CDF")
                            for currency_field in ("from_currency", "to_currency"):
                                options = await fx_form.locator(f'select[name="{currency_field}"] option:not([disabled])').evaluate_all("elements => elements.map(element => element.value)")
                                if options != ["USD", "CDF"]:
                                    raise AssertionError(f"FX {currency_field} options must be limited to configured SEL currencies, got {options}")
                            await fx_form.locator('button[type="button"]').first.click()
                        elif catalog_label == "Kits solaires":
                            await page.get_by_role("button", name="Ajouter un kit").click()
                            product_form = page.locator("form.admin-catalog-form")
                            product_currency = product_form.locator('select[name="currency"]')
                            if await product_currency.input_value() != "USD":
                                raise AssertionError("New solar catalog products must default to the primary USD currency")
                            product_options = await product_currency.locator('option:not([disabled])').evaluate_all("elements => elements.map(element => element.value)")
                            if product_options != ["USD", "CDF"]:
                                raise AssertionError(f"Solar product currency options must be limited to USD/CDF, got {product_options}")
                            await product_form.locator('button[type="button"]').first.click()
                    await admin_nav.get_by_role("button", name="Tarifs eau et électricité", exact=True).click()
                    await page.get_by_role("heading", name="Tarifs eau et électricité").wait_for()
                    await admin_nav.get_by_role("button", name="TVA", exact=True).click()
                    await page.get_by_role("heading", name="Règle centrale de TVA").first.wait_for()
                    tax_form = page.locator("form.admin-catalog-form").first
                    await page.get_by_role("button", name="Ajouter un profil fiscal").click()
                    tax_form = page.locator("form.admin-catalog-form").first
                    await tax_form.locator('input[name="rate_percent"]').fill("16")
                    await tax_form.locator('input[name="source"]').fill("DEMO-TAX-APPROVAL")
                    await tax_form.locator('textarea[name="change_reason"]').fill("Révision fiscale de démonstration")
                    await tax_form.locator('input[name="effective_from"]').fill("2026-11-10")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-tax-profile-a06-{SHOT_SUFFIX}.png"))
                    await tax_form.locator('button[type="submit"]').click()
                    tax_review = page.get_by_role("alertdialog", name="Vérifier · Règle centrale de TVA")
                    await tax_review.wait_for()
                    await tax_review.get_by_role("button", name="Créer le brouillon").click()
                    await page.wait_for_function("window.__adminWriteLog.some(entry => entry.path === '/v1/admin/catalog/tax-profiles' && entry.method === 'POST')")
                    tax_body = await page.evaluate("window.__adminWriteLog.find(entry => entry.path === '/v1/admin/catalog/tax-profiles' && entry.method === 'POST').body")
                    if tax_body.get("rate") != "0.16" or tax_body.get("change_reason") != "Révision fiscale de démonstration":
                        raise AssertionError("Tax rate must be normalized to the Rust fraction and change reason persisted")
                    if tax_body.get("service_code") != "VAT" or tax_body.get("base") != "SUBTOTAL":
                        raise AssertionError("Every new tax draft must use the one central VAT rule on the subtotal")
                    await admin_nav.get_by_role("button", name="Tarifs eau et électricité", exact=True).click()
                    await page.get_by_role("heading", name="Tarifs eau et électricité").wait_for()
                    await admin_nav.get_by_role("button", name="Kits solaires", exact=True).wait_for()
                    draft_tariff = tariff_panel.get_by_role("row").filter(has_text="Brouillon").first
                    await draft_tariff.get_by_role("button", name="Examiner la révision").click()
                    review_panel = page.locator(".admin-reference-layout > .panel").first
                    await review_panel.get_by_role("heading", name="Détails soumis à validation").wait_for()
                    await review_panel.get_by_label("Référence de décision d’approbation SEL").fill("DEMO-APPROVAL-UX3")
                    if await page.locator(".admin-reference-actions .button").count() != 2:
                        raise AssertionError("Tariff approval review must expose only its two contextual next actions")
                    if "USD / m³" not in await review_panel.inner_text():
                        raise AssertionError("Tariff approval review must show the configured currency and unit")
                    await page.evaluate("window.scrollTo(0, 0)")
                    await page.screenshot(path=str(OUTPUT / f"admin-catalog-approval-{SHOT_SUFFIX}.png"))
                    await review_panel.get_by_role("button", name="Annuler la révision").click()
                    abandon_tariff_review = page.get_by_role("alertdialog", name="Annuler la révision ?")
                    await abandon_tariff_review.wait_for()
                    await abandon_tariff_review.get_by_role("button", name="Annuler", exact=True).click()
                    if await review_panel.get_by_label("Référence de décision d’approbation SEL").input_value() != "DEMO-APPROVAL-UX3":
                        raise AssertionError("Canceling tariff review navigation must preserve the approval reference")
                    await review_panel.get_by_role("button", name="Confirmer l’approbation").click()
                    approval_review = page.get_by_role("alertdialog", name="Vérifier l’approbation du référentiel")
                    await approval_review.wait_for()
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/catalog/tariffs/demo-tariff-draft/approve').length") != 0:
                        raise AssertionError("Catalog approval must wait for explicit confirmation")
                    await approval_review.get_by_role("button", name="Annuler").click()
                    if await review_panel.get_by_label("Référence de décision d’approbation SEL").input_value() != "DEMO-APPROVAL-UX3":
                        raise AssertionError("Canceling approval confirmation must preserve its entered reference")
                    await review_panel.get_by_role("button", name="Confirmer l’approbation").click()
                    await page.get_by_role("alertdialog", name="Vérifier l’approbation du référentiel").get_by_role("button", name="Approuver cette version").click()
                    await page.wait_for_function("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/catalog/tariffs/demo-tariff-draft/approve').length === 1")
                    approved_tariff = tariff_panel.get_by_role("row").filter(has_text="Approuvé").last
                    await approved_tariff.get_by_role("button", name="Révoquer…").click()
                    await approved_tariff.get_by_label("Motif ou référence de révocation").fill("DEMO-REVOKE-UX3")
                    await approved_tariff.get_by_role("button", name="Confirmer la révocation").click()
                    revocation_review = page.get_by_role("alertdialog", name="Vérifier la révocation du référentiel")
                    await revocation_review.wait_for()
                    await page.screenshot(path=str(OUTPUT / f"admin-catalog-revocation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/catalog/tariffs/demo-tariff-approved/revoke').length") != 0:
                        raise AssertionError("Catalog revocation must wait for explicit confirmation")
                    await revocation_review.get_by_role("button", name="Annuler").click()
                    if await approved_tariff.get_by_label("Motif ou référence de révocation").input_value() != "DEMO-REVOKE-UX3":
                        raise AssertionError("Canceling revocation review must preserve its reason")
                    await approved_tariff.get_by_role("button", name="Confirmer la révocation").click()
                    await page.get_by_role("alertdialog", name="Vérifier la révocation du référentiel").get_by_role("button", name="Révoquer cette version").click()
                    await page.wait_for_function("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/catalog/tariffs/demo-tariff-approved/revoke').length === 1")
                    await page.get_by_role("button", name="Ajouter une version").click()
                    tariff_form = page.locator("form.admin-catalog-form").first
                    await tariff_form.locator('select[name="service_code"]').select_option("ELECTRICITY")
                    await tariff_form.locator('select[name="customer_category"]').select_option("RESIDENTIAL")
                    await tariff_form.locator('select[name="unit"]').select_option("KWH")
                    await tariff_form.locator('input[name="unit_price"]').fill("0.095")
                    await tariff_form.locator('select[name="currency"]').select_option("USD")
                    await tariff_form.locator('input[name="effective_from"]').fill("2026-01-01")
                    await page.evaluate("window.__rejectCatalogDraft = true")
                    await tariff_form.locator('button[type="submit"]').click()
                    draft_review = page.get_by_role("alertdialog", name="Vérifier · Catalogue tarifaire")
                    await draft_review.wait_for()
                    await page.screenshot(path=str(OUTPUT / f"admin-catalog-draft-confirmation-{SHOT_SUFFIX}.png"))
                    if await page.evaluate("window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/catalog/tariffs' && entry.method === 'POST').length") != 0:
                        raise AssertionError("Catalog drafts must not be submitted before explicit review")
                    await draft_review.get_by_role("button", name="Annuler").click()
                    if await tariff_form.locator('input[name="unit_price"]').input_value() != "0.095":
                        raise AssertionError("Canceling catalog draft review must preserve the entered values")
                    await tariff_form.locator('button[type="submit"]').click()
                    draft_review = page.get_by_role("alertdialog", name="Vérifier · Catalogue tarifaire")
                    await draft_review.get_by_role("button", name="Créer le brouillon").click()
                    rejected_draft_notice = page.get_by_role("alert")
                    await rejected_draft_notice.get_by_text("Certaines données ne sont pas acceptées par les règles SEL.").wait_for()
                    if "Validation refusal simulated" in await rejected_draft_notice.inner_text():
                        raise AssertionError("Raw server validation details must not leak into the localized UI")
                    if await tariff_form.locator('select[name="service_code"]').input_value() != "ELECTRICITY":
                        raise AssertionError("Catalog draft input must be retained when the server rejects creation")
                    await page.evaluate("window.__rejectCatalogDraft = false")
                    await admin_nav.get_by_role("button", name="TVA", exact=True).click()
                    abandon_catalog = page.get_by_role("alertdialog", name="Abandonner les modifications ?")
                    await abandon_catalog.wait_for()
                    await abandon_catalog.get_by_role("button", name="Annuler").click()
                    if await tariff_form.locator('input[name="unit_price"]').input_value() != "0.095":
                        raise AssertionError("Canceling catalog navigation must preserve the tariff draft")
                    await admin_nav.get_by_role("button", name="TVA", exact=True).click()
                    await page.get_by_role("alertdialog", name="Abandonner les modifications ?").get_by_role("button", name="Abandonner et continuer").click()
                    await page.get_by_role("heading", name="Règle centrale de TVA").first.wait_for()
                    await admin_nav.get_by_role("button", name="Tarifs eau et électricité", exact=True).click()
                    await page.get_by_role("heading", name="Tarifs eau et électricité").wait_for()
                dimensions = await page.evaluate("({viewport: document.documentElement.clientWidth, content: document.documentElement.scrollWidth})")
                if dimensions["content"] > dimensions["viewport"]:
                    raise AssertionError(f"{slug} overflows horizontally: {dimensions}")
            if requested_views:
                return
            await page.get_by_role("button", name="Paiements", exact=True).click()
            pending_admin_draft = page.get_by_role("alertdialog", name="Abandonner la saisie ?")
            if await pending_admin_draft.count():
                await pending_admin_draft.get_by_role("button", name="Abandonner et continuer").click()
            await page.locator(".workspace-tabs").get_by_role("button", name="Journal de caisse", exact=True).click()
            await page.get_by_role("button", name="Enregistrer un paiement", exact=True).first.click()
            await page.get_by_role("heading", name="Enregistrer un paiement").wait_for()
            await page.locator('.payment-workspace form select[name="customer_id"]').select_option("demo-customer-0001")
            await page.locator('input[name="received_amount"]').fill("3.00")
            await page.get_by_role("button", name="Affecter un paiement aux créances").click()
            await page.get_by_role("heading", name="Affecter un paiement aux créances").wait_for()
            if await page.evaluate("window.scrollY") != 0:
                raise AssertionError("Payment allocation stage must return to the page heading")
            await page.get_by_role("button", name="+ Ajouter une créance").click()
            await page.locator('.allocation-card select').first.locator('option[value="demo-receivable-0001"]').wait_for(state="attached")
            await page.locator('.allocation-card select').nth(0).select_option("demo-receivable-0001")
            await page.locator('.allocation-card select').nth(1).select_option("demo-receivable-solar-0001")
            await page.locator('.allocation-card select option[value="demo-solar-schedule-0001"]').wait_for(state="attached")
            await page.locator('.allocation-card select').nth(2).select_option("demo-solar-schedule-0001")
            await page.locator('.allocation-card input[type="number"]').nth(0).fill("1.00")
            await page.locator('.allocation-card input[type="number"]').nth(1).fill("2.00")
            await page.locator(".page-heading > button").click()
            await page.get_by_role("heading", name="Confirmer le règlement").wait_for()
            await page.get_by_role("button", name="Confirmer et enregistrer", exact=True).click()
            await page.wait_for_function("window.__paymentPostCount === 2")
            await page.get_by_role("button", name="Journal de caisse", exact=True).last.click()
            await page.get_by_role("heading", name="Journal de caisse").wait_for()
            await page.get_by_role("button", name="Reçu PDF").first.wait_for()
            await page.get_by_role("button", name="Reçu PDF").first.click()
            try:
                await page.get_by_role("dialog", name="Reçu de paiement").wait_for(timeout=5000)
            except Exception as error:
                notice = await page.locator(".notice").inner_text() if await page.locator(".notice").count() else "aucune erreur affichée"
                raise AssertionError(f"Payment receipt PDF preview did not open: {notice}") from error
            await assert_pdf_rendered(page, "Payment receipt")
            await assert_generated_pdf_contract(page, "Payment receipt", 1, ("Reçu de paiement", "Espèces", "22,62 USD", "3,00 USD", "19,62 USD"))
            await assert_print_layout(page, "Payment receipt", 1)
            await page.screenshot(path=str(OUTPUT / f"payment-receipt-pdf-preview-{SHOT_SUFFIX}.png"))
            await page.get_by_role("button", name="Fermer l’aperçu").click()
            await page.evaluate("window.scrollTo(0, 0)")
            await page.wait_for_timeout(120)
            await page.screenshot(path=str(OUTPUT / f"payments-solar-{SHOT_SUFFIX}.png"))
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
