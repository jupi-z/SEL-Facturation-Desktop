"""Persistance SQLite locale, migrations minimales et sauvegarde cohérente."""
from __future__ import annotations
from contextlib import contextmanager
from datetime import date, datetime
from pathlib import Path
import os
import sqlite3
import sys
import tempfile

APP_NAME = 'SEL Facturation'


def home() -> Path:
    custom = os.environ.get('SEL_DATA_DIR')
    if custom:
        return Path(custom).resolve()
    if sys.platform == 'win32':
        root = Path(os.environ.get('LOCALAPPDATA') or Path.home() / 'AppData' / 'Local')
        return root / 'SEL Facturation'
    return Path.home() / '.local' / 'share' / 'sel-facturation'


def db_path():
    return home() / 'sel.sqlite3'


def connect() -> sqlite3.Connection:
    home().mkdir(parents=True, exist_ok=True)
    c = sqlite3.connect(db_path(), timeout=15)
    c.row_factory = sqlite3.Row
    c.execute('PRAGMA foreign_keys = ON')
    c.execute('PRAGMA busy_timeout = 15000')
    c.execute('PRAGMA journal_mode = WAL')
    return c


def initialize():
    c = connect()
    try:
      with c:
        c.executescript('''
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY, value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS admins (
          id INTEGER PRIMARY KEY CHECK (id=1), salt TEXT NOT NULL, hash TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS clients (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          code TEXT NOT NULL UNIQUE,
          name TEXT NOT NULL, address TEXT NOT NULL,
          category TEXT NOT NULL DEFAULT 'Résidentiel', client_type TEXT NOT NULL DEFAULT 'Ordinaire',
          contract_number TEXT NOT NULL DEFAULT '', meter_number TEXT NOT NULL,
          center TEXT NOT NULL DEFAULT 'KOLWEZI', service_point TEXT NOT NULL DEFAULT '',
          nif TEXT NOT NULL DEFAULT '', phone TEXT NOT NULL DEFAULT '',
          coefficient TEXT NOT NULL DEFAULT '1', created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS invoices (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          number TEXT NOT NULL UNIQUE,
          client_id INTEGER NOT NULL REFERENCES clients(id),
          billing_period TEXT NOT NULL,
          issue_date TEXT NOT NULL,
          due_date TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL DEFAULT 'ISSUED' CHECK(status IN ('ISSUED', 'VOID')),
          client_snapshot TEXT NOT NULL,
          company_snapshot TEXT NOT NULL,
          calc_snapshot TEXT NOT NULL,
          currency TEXT NOT NULL DEFAULT 'CDF',
          note TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          void_reason TEXT NOT NULL DEFAULT ''
        );
        CREATE UNIQUE INDEX IF NOT EXISTS invoices_one_active_period ON invoices(client_id, billing_period) WHERE status='ISSUED';
        CREATE TABLE IF NOT EXISTS payments (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          invoice_id INTEGER NOT NULL REFERENCES invoices(id),
          amount TEXT NOT NULL, method TEXT NOT NULL, reference TEXT NOT NULL DEFAULT '',
          paid_on TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS sequences (
          year INTEGER PRIMARY KEY, last_number INTEGER NOT NULL
        );
        CREATE TRIGGER IF NOT EXISTS immutable_invoice
          BEFORE UPDATE ON invoices
          WHEN NEW.number != OLD.number OR NEW.client_snapshot != OLD.client_snapshot
            OR NEW.calc_snapshot != OLD.calc_snapshot OR NEW.company_snapshot != OLD.company_snapshot
            OR NEW.billing_period != OLD.billing_period OR NEW.issue_date != OLD.issue_date
            OR NEW.due_date != OLD.due_date OR NEW.client_id != OLD.client_id
            OR NEW.currency != OLD.currency
          BEGIN SELECT RAISE(ABORT, 'Facture émise immuable'); END;
        ''')
        defaults = {
            'company_name': 'SOCIÉTÉ D’ÉNERGIE DU LUALABA',
            'tagline': 'Énergie solaire pour un Lualaba durable',
            'address': 'Avenue de la Province, Commune de Dilala, Kolwezi, Lualaba – RDC',
            'phone': '', 'email': '', 'website': '',
            'rccm': '', 'nif': '', 'id_nat': '',
            'tiers': '1-100:300\n101-200:320',
            'local_rate': '1', 'vat_rate': '16', 'vat_base': 'ENERGY',
            'currency': 'CDF', 'footnote': 'Document non authentifié par un dispositif fiscal DGI.',
            'configured': '0'
        }
        for k, v in defaults.items():
            c.execute('INSERT OR IGNORE INTO settings(key,value) VALUES(?,?)', (k,v))
    finally:
        c.close()


def settings(c) -> dict:
    return {r['key']: r['value'] for r in c.execute('SELECT key,value FROM settings')}


def active_invoice(c, client_id, period):
    return c.execute("SELECT id,number FROM invoices WHERE client_id=? AND billing_period=? AND status='ISSUED'", (client_id, period)).fetchone()


def invoice_full(c, invoice_id):
    inv=c.execute('SELECT * FROM invoices WHERE id=?',(invoice_id,)).fetchone()
    if not inv: return None
    payments=list(c.execute('SELECT * FROM payments WHERE invoice_id=? ORDER BY paid_on,id',(invoice_id,)))
    return inv,payments


def backup(destination: Path):
    destination=Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd,temp_name=tempfile.mkstemp(prefix=f'.{destination.name}.',suffix='.tmp',dir=destination.parent)
    os.close(fd)
    try:
        target=sqlite3.connect(temp_name)
        try:
            source = connect()
            try:
                source.backup(target)
            finally:
                source.close()
            check=target.execute('PRAGMA integrity_check').fetchone()[0]
            if check!='ok': raise sqlite3.DatabaseError(f'Backup SQLite invalide : {check}')
        finally:
            target.close()
        os.replace(temp_name,destination)
    except Exception:
        Path(temp_name).unlink(missing_ok=True)
        raise
    return destination


def automatic_backup():
    d=home() / 'sauvegardes'
    d.mkdir(parents=True, exist_ok=True)
    filename=d / f'sel-{date.today().isoformat()}.sqlite3'
    # Refresh the daily snapshot on every application start.  A backup made
    # before the first invoice of the day must not remain the only copy after
    # later writes to the same database.
    if db_path().exists():
        backup(filename)
    old=sorted(d.glob('sel-*.sqlite3'))
    for fp in old[:-14]:
        fp.unlink(missing_ok=True)
