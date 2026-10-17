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
    s['local_rate']='1'
    s['currency']='CDF'
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
    assert 'DÉCOMPTE D’ÉLECTRICITÉ' in txt
    assert 'CONDITIONS DE RÈGLEMENT' in txt
    assert 'MONTANT À RÉGLER' in txt
    with pytest.raises(Exception):
        c.execute("UPDATE invoices SET calc_snapshot='{}' WHERE id=1")
    with pytest.raises(Exception):
        c.execute('''INSERT INTO invoices(number,client_id,billing_period,issue_date,client_snapshot,company_snapshot,calc_snapshot,currency)
                     VALUES('SEL-2026-000002',1,'2026-08','2026-10-14','{}','{}','{}','CDF')''')
    c.rollback()
    db.backup(tmp_path/'check.sqlite3')
    assert (tmp_path/'check.sqlite3').exists()
    c.close()


def test_automatic_backup_refreshes_after_same_day_changes(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    db.automatic_backup()
    backup_path=tmp_path/'sauvegardes'/f"sel-{__import__('datetime').date.today().isoformat()}.sqlite3"
    with db.connect() as c:
        c.execute('INSERT INTO clients(code,name,address,meter_number,coefficient) VALUES(?,?,?,?,?)',
                  ('SEL-C-00001','Abonné sauvegardé','Kolwezi','M123','1'))
    db.automatic_backup()
    with db.sqlite3.connect(backup_path) as saved:
        assert saved.execute('SELECT name FROM clients').fetchone()[0]=='Abonné sauvegardé'


def test_automatic_backup_uses_configured_retention_count(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    folder=tmp_path/'sauvegardes'
    folder.mkdir()
    for name in ('sel-2026-01-01.sqlite3','sel-2026-01-02.sqlite3','sel-2026-01-03.sqlite3'):
        (folder/name).write_bytes(b'old')
    with db.connect() as c:
        c.execute("UPDATE billing_policy SET value='2' WHERE key='backup_retention_count'")
        c.commit()
    db.automatic_backup()
    assert len(list(folder.glob('sel-*.sqlite3'))) == 2


def test_daily_retention_preserves_preupdate_and_export_snapshots(monkeypatch, tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR', str(tmp_path))
    db.initialize()
    folder = tmp_path / 'sauvegardes'
    folder.mkdir()
    for name in ('sel-2026-01-01.sqlite3', 'sel-2026-01-02.sqlite3', 'sel-2026-01-03.sqlite3'):
        (folder / name).write_bytes(b'daily')
    preupdate = folder / 'sel-preupdate-20260101-010101-000001.sqlite3'
    export = folder / 'sel-export-20260101-010101-000001.sqlite3'
    preupdate.write_bytes(b'preupdate')
    export.write_bytes(b'export')
    with db.connect() as c:
        c.execute("UPDATE billing_policy SET value='2' WHERE key='backup_retention_count'")
        c.commit()

    db.automatic_backup()

    assert preupdate.read_bytes() == b'preupdate'
    assert export.read_bytes() == b'export'
    assert len(list(folder.glob('sel-2026-*.sqlite3'))) == 2


def test_pre_update_backup_preserves_database_before_migration(monkeypatch, tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR', str(tmp_path))
    db.initialize()
    with db.connect() as c:
        c.execute("INSERT INTO clients(code,name,address,meter_number) VALUES(?,?,?,?)", ('SEL-C-PRE', 'Avant migration', 'Kolwezi', ''))
    snapshot = db.pre_update_backup()
    with db.sqlite3.connect(snapshot) as saved:
        assert saved.execute("SELECT name FROM clients WHERE code='SEL-C-PRE'").fetchone()[0] == 'Avant migration'


def test_pre_update_backup_has_a_separate_bounded_retention(monkeypatch, tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR', str(tmp_path))
    db.initialize()
    folder = tmp_path / 'sauvegardes'
    folder.mkdir()
    for index in range(db.PRE_UPDATE_RETENTION_COUNT + 3):
        path = folder / f'sel-preupdate-20260101-010101-{index:06d}.sqlite3'
        path.write_bytes(b'old')

    db.pre_update_backup()

    snapshots = sorted(folder.glob('sel-preupdate-*.sqlite3'))
    assert len(snapshots) == db.PRE_UPDATE_RETENTION_COUNT
    assert snapshots[-1].stat().st_size > 0


def test_backup_preserves_previous_snapshot_if_refresh_fails(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    destination=tmp_path/'previous.sqlite3'
    db.backup(destination)
    previous=destination.read_bytes()
    class BrokenSource:
        def backup(self, source):
            source.execute('CREATE TABLE corrupt_marker(value TEXT)')
            source.commit()
            raise RuntimeError('simulated backup interruption')

        def __enter__(self):
            return self

        def __exit__(self,*exc):
            return False

        def close(self):
            pass

    monkeypatch.setattr(db,'connect',lambda: BrokenSource())
    with pytest.raises(RuntimeError,match='simulated backup interruption'):
        db.backup(destination)
    assert destination.read_bytes()==previous


def test_database_handles_are_released_after_initialize_and_backup(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    destination=tmp_path/'released.sqlite3'
    db.backup(destination)

    db.db_path().unlink()
    destination.unlink()


def test_payment_history_is_immutable(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    db.initialize()
    with db.connect() as c:
        c.execute("INSERT INTO clients(code,name,address,meter_number) VALUES('SEL-C-00001','A','K','M')")
        c.execute("INSERT INTO invoices(number,client_id,billing_period,issue_date,client_snapshot,company_snapshot,calc_snapshot) VALUES('SEL-2026-000001',1,'2026-10','2026-10-14','{}','{}','{\"total\":\"100.00\"}')")
        c.execute("INSERT INTO payments(invoice_id,amount,method,paid_on) VALUES(1,'10.00','Espèces','2026-10-14')")
        with pytest.raises(db.sqlite3.DatabaseError, match='Paiement enregistré immuable'):
            c.execute("UPDATE payments SET amount='20.00' WHERE id=1")
        with pytest.raises(db.sqlite3.DatabaseError, match='Paiement enregistré immuable'):
            c.execute("DELETE FROM payments WHERE id=1")


def test_backup_restore_copy_preserves_invoice_snapshot(monkeypatch,tmp_path):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path/'live'))
    db.initialize()
    with db.connect() as c:
        c.execute("INSERT INTO clients(code,name,address,meter_number) VALUES('SEL-C-00001','A','K','M')")
        c.execute("INSERT INTO invoices(number,client_id,billing_period,issue_date,client_snapshot,company_snapshot,calc_snapshot) VALUES('SEL-2026-000001',1,'2026-10','2026-10-14','{\"name\":\"A\"}','{}','{\"total\":\"100.00\"}')")
    snapshot=db.backup(tmp_path/'snapshot.sqlite3')
    restored=tmp_path/'restored.sqlite3'
    restored.write_bytes(snapshot.read_bytes())
    with db.sqlite3.connect(restored) as c:
        row=c.execute('SELECT number,calc_snapshot FROM invoices').fetchone()
        assert row == ('SEL-2026-000001','{\"total\":\"100.00\"}')
        assert c.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
