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
          coefficient TEXT NOT NULL DEFAULT '1', customer_type TEXT NOT NULL DEFAULT '',
          billing_frequency TEXT NOT NULL DEFAULT 'MONTHLY', created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
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
        CREATE INDEX IF NOT EXISTS clients_lookup ON clients(name, code, meter_number, contract_number);
        CREATE INDEX IF NOT EXISTS invoices_history ON invoices(issue_date, billing_period, client_id);
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
        CREATE TABLE IF NOT EXISTS tariff_plans (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          code TEXT NOT NULL UNIQUE,
          customer_type TEXT NOT NULL,
          unit TEXT NOT NULL DEFAULT 'KWH',
          unit_price TEXT NOT NULL,
          currency TEXT NOT NULL DEFAULT 'USD',
          vat_rate TEXT NOT NULL DEFAULT '16',
          effective_from TEXT NOT NULL,
          active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1))
        );
        CREATE INDEX IF NOT EXISTS tariff_plans_lookup
          ON tariff_plans(customer_type, effective_from, active);
        CREATE TABLE IF NOT EXISTS exchange_rates (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          base_currency TEXT NOT NULL,
          quote_currency TEXT NOT NULL,
          rate TEXT NOT NULL,
          effective_on TEXT NOT NULL,
          source TEXT NOT NULL DEFAULT '',
          UNIQUE(base_currency, quote_currency, effective_on)
        );
        CREATE TRIGGER IF NOT EXISTS immutable_invoice
          BEFORE UPDATE ON invoices
          WHEN NEW.number != OLD.number OR NEW.client_snapshot != OLD.client_snapshot
            OR NEW.calc_snapshot != OLD.calc_snapshot OR NEW.company_snapshot != OLD.company_snapshot
            OR NEW.billing_period != OLD.billing_period OR NEW.issue_date != OLD.issue_date
            OR NEW.due_date != OLD.due_date OR NEW.client_id != OLD.client_id
            OR NEW.currency != OLD.currency
          BEGIN SELECT RAISE(ABORT, 'Facture émise immuable'); END;
        CREATE TRIGGER IF NOT EXISTS immutable_payment_update
          BEFORE UPDATE ON payments
          BEGIN SELECT RAISE(ABORT, 'Paiement enregistré immuable'); END;
        CREATE TRIGGER IF NOT EXISTS immutable_payment_delete
          BEFORE DELETE ON payments
          BEGIN SELECT RAISE(ABORT, 'Paiement enregistré immuable'); END;
        ''')
        columns = {row['name'] for row in c.execute('PRAGMA table_info(clients)')}
        if 'customer_type' not in columns:
            c.execute("ALTER TABLE clients ADD COLUMN customer_type TEXT NOT NULL DEFAULT ''")
        if 'billing_frequency' not in columns:
            c.execute("ALTER TABLE clients ADD COLUMN billing_frequency TEXT NOT NULL DEFAULT 'MONTHLY'")
        defaults = {
            'company_name': 'SOCIÉTÉ D’ÉNERGIE DU LUALABA',
            'tagline': 'Énergie solaire pour un Lualaba durable',
            'address': 'Avenue de la Province, Commune de Dilala, Kolwezi, Lualaba – RDC',
            'phone': '', 'email': '', 'website': '',
            'rccm': '', 'nif': '', 'id_nat': '',
            'tiers': '1-100:300\n101-200:320',
            'local_rate': '0', 'vat_rate': '16', 'vat_base': 'ENERGY',
            'currency': 'USD', 'exchange_rate_usd_cdf': '2360',
            'exchange_rate_date': date.today().isoformat(),
            'footnote': '',
            'configured': '0'
        }
        for k, v in defaults.items():
            c.execute('INSERT OR IGNORE INTO settings(key,value) VALUES(?,?)', (k,v))
        c.execute('''INSERT OR IGNORE INTO tariff_plans
            (code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active)
            VALUES ('RESIDENTIAL-2026','RESIDENTIAL','KWH','0.095','USD','16','2026-01-01',1),
                   ('COMMERCIAL-2026','COMMERCIAL','KWH','0.12','USD','16','2026-01-01',1)''')
        c.execute('''INSERT OR IGNORE INTO exchange_rates
            (base_currency,quote_currency,rate,effective_on,source)
            VALUES ('USD','CDF','2360',?,?)''', (date.today().isoformat(), 'SEL'))
    finally:
        c.close()


def settings(c) -> dict:
    return {r['key']: r['value'] for r in c.execute('SELECT key,value FROM settings')}


def tariff_plan_for(c, customer_type: str, on_date: str):
    if customer_type not in ('RESIDENTIAL', 'COMMERCIAL'):
        raise ValueError('Type d’abonné invalide.')
    row = c.execute('''SELECT * FROM tariff_plans
        WHERE customer_type=? AND active=1 AND effective_from<=?
        ORDER BY effective_from DESC, id DESC LIMIT 1''', (customer_type, on_date)).fetchone()
    if not row:
        raise ValueError('Aucun tarif applicable pour ce type et cette date.')
    return row


def exchange_rate_for(c, base_currency: str, quote_currency: str, on_date: str):
    row = c.execute('''SELECT * FROM exchange_rates
        WHERE base_currency=? AND quote_currency=? AND effective_on<=?
        ORDER BY effective_on DESC, id DESC LIMIT 1''',
                    (base_currency, quote_currency, on_date)).fetchone()
    if not row:
        raise ValueError('Aucun taux de change applicable.')
    return row


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


def pre_update_backup():
    """Create a timestamped snapshot before schema migration or replacement."""
    if not db_path().exists():
        return None
    folder = home() / 'sauvegardes'
    folder.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now().strftime('%Y%m%d-%H%M%S-%f')
    return backup(folder / f'sel-preupdate-{stamp}.sqlite3')
