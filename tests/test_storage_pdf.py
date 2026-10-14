import json
import os
import sys
from io import BytesIO
from pathlib import Path
from decimal import Decimal
import pytest
from pypdf import PdfReader
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import db
from domain import calculate_from_settings
from invoice_pdf import render_invoice


def test_initialize_backup_immutability_and_pdf(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    c=db.connect()
    s=db.settings(c)
    assert s['configured']=='0'
    assert db.db_path().exists()
    c.execute('INSERT INTO clients(code,name,address,meter_number,coefficient) VALUES(?,?,?,?,?)',('SEL-C-00001','Abonné Test','Kolwezi','M123','1'))
    calc=calculate_from_settings('450','650','1',s)
    c.execute('''INSERT INTO invoices(number,client_id,billing_period,issue_date,due_date,client_snapshot,company_snapshot,calc_snapshot,currency)
                  VALUES(?,?,?,?,?,?,?,?,?)''',('SEL-2026-000001',1,'2026-08','2026-10-14','2026-10-30',
                  json.dumps(dict(c.execute('SELECT * FROM clients').fetchone())),json.dumps(s),json.dumps(calc),'CDF'))
    c.commit()
    row=c.execute('SELECT * FROM invoices').fetchone()
    pdf=render_invoice(row,[])
    assert pdf[:5]==b'%PDF-'
    rdr=PdfReader(BytesIO(pdf)); txt='\n'.join(p.extract_text() for p in rdr.pages)
    assert 'SEL-2026-000001' in txt
    assert 'NON AUTHENTIFI' not in txt
    assert 'NON FISCALIS' not in txt
    assert 'Service clientèle' in txt
    assert '72 540,00' in txt
    with pytest.raises(Exception):
        c.execute("UPDATE invoices SET calc_snapshot='{}' WHERE id=1")
    with pytest.raises(Exception):
        c.execute('''INSERT INTO invoices(number,client_id,billing_period,issue_date,client_snapshot,company_snapshot,calc_snapshot,currency)
                     VALUES('SEL-2026-000002',1,'2026-08','2026-10-14','{}','{}','{}','CDF')''')
    c.rollback()
    db.backup(tmp_path/'check.sqlite3')
    assert (tmp_path/'check.sqlite3').exists()
    c.close()
