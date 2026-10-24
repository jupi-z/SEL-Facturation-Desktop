"""Generate route-backed synthetic PDF evidence for the SEL release gate.

This command uses a temporary SQLite database and never opens the user's SEL data.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from contextlib import closing
from io import BytesIO
from pathlib import Path

from pypdf import PdfReader

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "Docs" / "SEL_RELEASE_GATE_V1" / "evidence"
sys.path.insert(0, str(ROOT))


def token(client, path: str) -> str:
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="sel-release-gate-") as temp:
        os.environ["SEL_DATA_DIR"] = temp
        from app import create_app
        import db

        client = create_app().test_client()
        client.post("/setup", data={"_csrf": token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
        client.post("/login", data={"_csrf": token(client, "/login"), "password": "TestPassword2026!"})
        with closing(db.connect()) as connection:
            connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
            connection.execute("UPDATE tariff_plans SET confirmed=1,source='Synthetic release-gate fixture' WHERE service_code='ELECTRICITY'")
            connection.execute("UPDATE tax_profiles SET confirmed=1,source='Synthetic release-gate fixture' WHERE service_code='ELECTRICITY'")
            connection.execute("UPDATE exchange_rates SET rate='2360',source='Synthetic release-gate fixture' WHERE base_currency='USD' AND quote_currency='CDF'")
            connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_connection_status) VALUES('SEL-C-EVID-ZERO','Synthetic zero-water client','Kolwezi','TEST-0','RESIDENTIAL','UNKNOWN')")
            connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_connection_status) VALUES('SEL-C-EVID-POS','Synthetic positive-water client','Kolwezi','TEST-2','RESIDENTIAL','CONNECTED')")
            connection.commit()

        period = __import__('datetime').date.today().strftime('%Y-%m')

        def issue(client_id: int, water: str, acknowledge_zero: bool) -> int:
            payload = {"_csrf": token(client, "/invoices/new"), "client_id": str(client_id),
                       "billing_period": period, "consumption_kwh": "100", "water_m3": water,
                       "due_date": "2026-10-31"}
            if acknowledge_zero:
                payload["water_zero_confirmed"] = "yes"
            preview = client.post("/invoices/new", data=payload)
            if "Vérifier la facture" not in preview.get_data(as_text=True):
                raise RuntimeError("Synthetic invoice preview did not pass validation")
            response = client.post("/invoices/new/confirm", data={"_csrf": token(client, "/invoices/new")})
            if response.status_code != 302:
                raise RuntimeError("Synthetic invoice confirmation did not complete")
            return int(response.location.rsplit('/', 1)[1])

        zero_id = issue(1, "0", True)
        with closing(db.connect()) as connection:
            connection.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) VALUES('EVID-WATER-TEST','RESIDENTIAL','M3','0.40','USD','0','2026-01-01',1,'WATER','Synthetic release-gate fixture',1)")
            connection.execute("INSERT INTO tax_profiles(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) VALUES('EVID-WATER-TAX','Synthetic water test profile','0','NONE','0','2026-01-01',1,'WATER','Synthetic release-gate fixture',1)")
            connection.commit()
        positive_id = issue(2, "2", False)
        token(client, f"/invoices/{positive_id}")
        client.post(f"/invoices/{positive_id}/payments", data={"_csrf": token(client, f"/invoices/{positive_id}"),
                      "amount": "1.00", "method": "Espèces", "paid_on": "2026-10-20"})
        client.post(f"/invoices/{positive_id}/payments/1/reverse", data={"_csrf": token(client, f"/invoices/{positive_id}"),
                      "amount": "0.10", "reason": "Synthetic evidence reversal", "reversed_on": "2026-10-21"})

        artifacts = []
        cases = ((zero_id, "invoice-usd-zero-water"),
                 (zero_id, "invoice-cdf-zero-water"),
                 (positive_id, "invoice-cdf-positive-water-net-payment"))
        for invoice_id, stem in cases:
            with closing(db.connect()) as connection:
                before = connection.execute("SELECT calc_snapshot FROM invoices WHERE id=?", (invoice_id,)).fetchone()[0]
            url = f"/invoices/{invoice_id}/pdf" + ("?currency=CDF" if "-cdf-" in stem else "")
            response = client.get(url)
            if response.status_code != 200 or not response.data.startswith(b"%PDF-"):
                raise RuntimeError(f"PDF route failed for {stem}: HTTP {response.status_code}")
            pdf_path = OUT / f"{stem}.pdf"
            pdf_path.write_bytes(response.data)
            with closing(db.connect()) as connection:
                after = connection.execute("SELECT calc_snapshot FROM invoices WHERE id=?", (invoice_id,)).fetchone()[0]
            if before != after:
                raise RuntimeError(f"PDF display mutated immutable invoice snapshot: {stem}")
            text = " ".join(" ".join((page.extract_text() or "") for page in PdfReader(BytesIO(response.data)).pages).split())
            if "Eau" not in text or "Électricité" not in text:
                raise RuntimeError(f"Combined service labels missing from {stem}")
            zero_currency="CDF" if "-cdf-" in stem else "USD"
            if "zero-water" in stem and ("0 M3" not in text or f"0,00 {zero_currency}" not in text or "0,40" in text):
                raise RuntimeError("Zero-water line or amount missing from generated PDF")
            if stem == "invoice-cdf-zero-water" and ("22 420,00 CDF" not in text or "26 007,20 CDF" not in text):
                raise RuntimeError("CDF zero-water totals failed extraction from actual PDF")
            if "positive-water" in stem and ("1 888,00 CDF" not in text or "27 895,20 CDF" not in text or "25 771,20 CDF" not in text):
                raise RuntimeError("CDF positive-water/payment reconciliation failed PDF extraction")
            pdf_hash = hashlib.sha256(response.data).hexdigest()
            png_path = OUT / f"{stem}.png"
            pdftoppm = shutil.which("pdftoppm") or (str(Path(os.environ["POPPLER_BIN"]) / "pdftoppm.exe") if os.environ.get("POPPLER_BIN") else None)
            if pdftoppm:
                subprocess.run([pdftoppm, "-f", "1", "-singlefile", "-png", "-r", "110", str(pdf_path), str(png_path.with_suffix(''))], check=True, capture_output=True)
            artifacts.append({"file": pdf_path.name, "sha256": pdf_hash, "bytes": len(response.data),
                              "snapshot_unchanged": True, "extracted_text_sha256": hashlib.sha256(text.encode()).hexdigest(),
                              "visual_render": png_path.name if png_path.exists() else None})

        (OUT / "manifest.json").write_text(json.dumps({"database": "temporary synthetic SQLite", "cases": artifacts}, indent=2), encoding="utf-8")
        print(json.dumps(artifacts, indent=2))


if __name__ == "__main__":
    main()
