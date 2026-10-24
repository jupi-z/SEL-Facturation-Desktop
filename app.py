"""Application locale SEL Facturation. Accessible exclusivement via 127.0.0.1."""
from __future__ import annotations
import csv
import hashlib
import hmac
import io
import json
import os
import re
import secrets
import sqlite3
import sys
import time
from datetime import date, datetime
from decimal import Decimal
from functools import wraps
from pathlib import Path
from urllib.error import HTTPError, URLError

from flask import (Flask, abort, flash, g, redirect, render_template, request,
                   send_file, session, url_for)

import db
from domain import (BillingError, calculate_flat_rate, calculate_from_readings,
                    calculate_from_settings, convert_amount, decimal, fmt, money,
                    calculate_service, combine_services, validate_due_date)
from invoice_pdf import render_invoice
from safeupdate_coordinator import SafeUpdateCoordinator
from version import APP_VERSION
from config_service import (
    active_customer_types, active_payment_methods, all_currencies,
    all_customer_types, all_payment_methods, all_tax_profiles,
    billing_policy, configuration_history, current_tariffs,
    customer_type_label, exchange_rate_history, record_change,
    tariff_history, tax_profile_for,
)

PAGE_SIZE = 50
# The repository-hosted manifest is the currently published channel. An
# operator can point this to the SEL update server with SEL_UPDATE_MANIFEST_URL
# once that server exposes the same signed-manifest contract.
DEFAULT_UPDATE_MANIFEST_URL = 'https://raw.githubusercontent.com/jupi-z/SEL-Facturation-Desktop/refs/heads/main/updates/latest.json'

DEVELOPER_LEGAL = {
    'name': 'MonarQue Group S.A.S.',
    'role': 'Pôle informatique LTEX',
    'legal_form': 'Société par Actions Simplifiée de droit congolais',
    'rccm': 'CD/KWZ/RCCM/26-B-00523',
    'national_id': '14-14300-N00001Y',
    'address': 'N° 900, Avenue Bunkeya, Quartier Mutoshi, Commune de Manika, Ville de Kolwezi, Province du Lualaba, République Démocratique du Congo',
    'email': 'contact@monarquegrp.com',
    'phones': '+243 840 855 547 et +243 971 666 441',
    'website': 'https://monarquegrp.com/',
}


def page_number(value):
    try:
        return max(1, int(value))
    except (TypeError, ValueError):
        return 1


def sum_money(values):
    """Additionne des montants persistés sans passer par un flottant binaire."""
    return sum((Decimal(str(value)) for value in values), Decimal('0'))


def combine_invoice_calculation(connection, client, electricity, billing_period, water_m3):
    """Add independently configured services while preserving legacy root keys."""
    currency=electricity.get('currency') or 'USD'
    electric_lines=[]
    for row in electricity.get('lines',[]):
        electric_lines.append({
            'label':row.get('range') or "Consommation d’électricité",
            'quantity':row.get('quantity','0'), 'unit':'KWH',
            'unit_price':row.get('unit_price',''), 'amount':row.get('amount','0.00'),
        })
    electric_service={
        'service_code':'ELECTRICITY','quantity':electricity.get('kwh','0'),
        'unit':'KWH','unit_price':electricity.get('unit_price',''),
        'currency':currency,'lines':electric_lines,
        'subtotal':electricity.get('energy','0.00'),
        'local_rate':electricity.get('local_rate','0'),
        'local_tax':electricity.get('local_tax','0.00'),
        'vat_rate':electricity.get('vat_rate','0'),
        'vat_base':electricity.get('vat_base','ENERGY'),
        'vat_basis':electricity.get('vat_basis',electricity.get('energy','0.00')),
        'vat':electricity.get('vat','0.00'),
        'fee':electricity.get('fee','0.00'),
        'total':electricity.get('total','0.00'),
        'tariff_code':electricity.get('tariff_code',''),
        'tax_profile_code':electricity.get('tax_profile_code',''),
    }
    services=[electric_service]
    water_value=(water_m3 or '').strip()
    if not water_value:
        raise BillingError('Saisissez la consommation d’eau en m³, y compris 0 si elle est nulle.')
    water_quantity=decimal(water_value, 'Consommation d’eau')
    if water_quantity > 0 and client.get('water_connection_status') != 'CONNECTED':
        if client.get('water_connection_status') == 'DISCONNECTED':
            raise BillingError('La consommation d’eau positive est impossible : le raccordement est déclaré déconnecté.')
        raise BillingError('Confirmez le raccordement physique à l’eau sur la fiche abonné avant de saisir une consommation positive.')
    customer_type=client.get('customer_type') or ''
    try:
        plan=db.tariff_plan_for(connection,customer_type,billing_period+'-01',
                                service_code='WATER',require_confirmed=True)
        profile=tax_profile_for(connection,billing_period+'-01',code=None,
                                 service_code='WATER',require_confirmed=True)
    except ValueError:
        if water_quantity != 0:
            raise
        plan=profile=None

    if plan and profile and plan['currency'] == currency:
        water=calculate_service(
            service_code='WATER',quantity=water_value,unit=plan['unit'],
            unit_price=plan['unit_price'],currency=currency,
            taxes={'vat_rate':profile['vat_rate'],'vat_base':profile['vat_base'],
                   'local_rate':profile['local_rate']},
        )
        water.update({'tariff_code':plan['code'],'tariff_effective_from':plan['effective_from'],
                      'tariff_source':plan['source'],'tax_profile_code':profile['code'],
                      'tax_source':profile['source']})
    elif water_quantity == 0:
        # A zero reading is still recorded on every invoice. With no approved
        # water price/tax profile, record zero volume and zero amount without
        # fabricating a unit price, tax rate, exemption, or source.
        zero='0.00'
        water={
            'service_code':'WATER','quantity':'0','unit':'M3','unit_price':'',
            'currency':currency,'lines':[{'label':'Consommation d’eau','quantity':'0',
                'unit':'M3','unit_price':'','amount':zero}],
            'subtotal':zero,'local_rate':'0','local_tax':zero,'vat_basis':zero,
            'vat':'0.00','total':zero,
        }
    else:
        if plan and plan['currency'] != currency:
            raise BillingError('Les tarifs d’eau et d’électricité doivent utiliser la même devise pour une facture combinée.')
        raise BillingError('Configurez un tarif et un profil fiscal d’eau confirmés par SEL avant de facturer une consommation positive.')
    services.append(water)

    combined=combine_services(services,currency)
    result=dict(electricity)
    result.update({
        'document_schema_version':2,'document_template_version':3,
        'services':combined['services'],'subtotal':combined['subtotal'],
        'service_subtotal_total':combined['subtotal'],
        'local_tax':combined['local_tax'],'vat':combined['vat'],
        'total':combined['total'],'currency':currency,
    })
    return result


def calculate_invoice_draft(connection, client, draft, conf):
    """Rebuild a billable preview from current, validated configuration."""
    period=draft['billing_period']
    if draft.get('consumption_kwh'):
        customer_type=client.get('customer_type') or ''
        if not customer_type:
            raise BillingError('Attribuez un type d’abonné avant de facturer.')
        plan=db.tariff_plan_for(connection,customer_type,period+'-01',require_confirmed=True)
        currency=plan['currency']
        active_currency=connection.execute("SELECT 1 FROM currency_catalog WHERE code=? AND active=1",(currency,)).fetchone()
        if not active_currency:
            raise BillingError('La devise du dernier tarif est inactive.')
        tax=tax_profile_for(connection,period+'-01',code=None,service_code='ELECTRICITY',require_confirmed=True)
        if Decimal(str(tax['vat_rate'])) != Decimal(str(plan['vat_rate'])):
            raise BillingError('Le tarif et le profil fiscal électricité ne sont pas cohérents; vérifiez les paramètres validés.')
        base_map={'ENERGY':'SUBTOTAL','ENERGY_LOCAL':'SUBTOTAL_LOCAL','ENERGY_LOCAL_FEE':'SUBTOTAL_LOCAL'}
        service=calculate_service(service_code='ELECTRICITY',quantity=draft['consumption_kwh'],unit='KWH',
                                  unit_price=plan['unit_price'],currency=currency,
                                  taxes={'vat_rate':tax['vat_rate'],'vat_base':base_map.get(tax['vat_base'],tax['vat_base']),
                                         'local_rate':tax['local_rate']})
        calc=calculate_flat_rate(consumption_kwh=draft['consumption_kwh'],unit_price=plan['unit_price'],vat_rate=tax['vat_rate'],currency=currency)
        calc.update({'energy':service['subtotal'],'local_rate':service['local_rate'],
                     'local_tax':service['local_tax'],'vat_base':tax['vat_base'],
                     'vat_basis':service['vat_basis'],'vat':service['vat'],'total':service['total'],
                     'lines':[{'range':'Consommation','quantity':service['quantity'],'unit_price':service['unit_price'],'amount':service['subtotal']}]})
        calc.update({'customer_type':customer_type,'customer_type_label':customer_type_label(connection,customer_type),
                     'tariff_code':plan['code'],'tariff_effective_from':plan['effective_from'],
                     'tariff_source':plan['source'],'unit_price':plan['unit_price'],
                     'tax_profile_code':tax['code'],'tax_source':tax['source']})
    else:
        calc=calculate_from_settings(draft['previous'],draft['current'],client['coefficient'],conf,draft.get('fee','0'))
    return combine_invoice_calculation(connection,client,calc,period,draft.get('water_m3',''))


def converted_invoice_copy(invoice, payments, rate_row, rate_value):
    """Build a CDF-only display copy; source snapshots and SQLite stay untouched."""
    rate=decimal(rate_value,'Taux USD/CDF',allow_zero=False)
    inv=dict(invoice)
    calc=json.loads(inv['calc_snapshot'])
    def convert(value):
        return str(money(Decimal(str(value))*rate))
    for key in ('unit_price','energy','subtotal','service_subtotal_total','local_tax','vat_basis','vat','fee','total'):
        if calc.get(key) not in (None,''):
            calc[key]=convert(calc[key])
    for line in calc.get('lines',[]):
        if line.get('amount') not in (None,''):
            line['amount']=convert(line['amount'])
        if line.get('unit_price') not in (None,''):
            line['unit_price']=convert(line['unit_price'])
    services=calc.get('services',[])
    for service in services:
        for key in ('unit_price','subtotal','local_tax','vat_basis','vat','fee','total'):
            if service.get(key) not in (None,''):
                service[key]=convert(service[key])
        for line in service.get('lines',[]):
            for key in ('unit_price','amount'):
                if line.get(key) not in (None,''):
                    line[key]=convert(line[key])
        service['currency']='CDF'
    if services:
        for root_key, service_key in (('subtotal','subtotal'),('service_subtotal_total','subtotal'),
                                      ('local_tax','local_tax'),('vat_basis','vat_basis'),
                                      ('vat','vat'),('fee','fee')):
            values=[Decimal(str(item.get(service_key,'0') or '0')) for item in services]
            calc[root_key]=str(money(sum(values,Decimal('0'))))
        calc['total']=str(money(sum((Decimal(str(item.get('total','0') or '0')) for item in services),Decimal('0'))))
    calc.update({'currency':'CDF','conversion_source_currency':'USD','conversion_rate':str(rate),
                 'conversion_date':rate_row.get('effective_on','') if rate_row else '',
                 'conversion_source':rate_row.get('source','') if rate_row else 'Taux saisi pour cette impression'})
    inv['currency']='CDF'
    inv['calc_snapshot']=json.dumps(calc,ensure_ascii=False)
    converted_payments=[]
    for payment in payments:
        item=dict(payment)
        amount=Decimal(convert(item['amount']))
        reversed_amount=Decimal(convert(item['reversed_amount']))
        item.update({'amount':str(amount),'reversed_amount':str(reversed_amount),
                     'net_amount':str(money(amount-reversed_amount))})
        converted_payments.append(item)
    return inv,converted_payments


def resource(name):
    base = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parent))
    return str(base / name)


def password_hash(password, salt):
    return hashlib.pbkdf2_hmac('sha256', password.encode('utf-8'), bytes.fromhex(salt), 600000).hex()


def read_secret():
    base=db.home()
    base.mkdir(parents=True, exist_ok=True)
    path=base/'session.key'
    if not path.exists():
        try:
            fd=os.open(str(path), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            with os.fdopen(fd, 'w') as f: f.write(secrets.token_hex(48))
        except FileExistsError: pass
    return path.read_text(encoding='utf-8').strip()


def create_app():
    if db.db_path().exists():
        db.pre_update_backup()
    db.initialize()
    db.automatic_backup()
    app=Flask(__name__, template_folder=resource('templates'), static_folder=resource('static'))
    app.secret_key=read_secret()
    app.config.update(SESSION_COOKIE_HTTPONLY=True, SESSION_COOKIE_SAMESITE='Strict',
                      PERMANENT_SESSION_LIFETIME=3600*8, TRUSTED_HOSTS=['127.0.0.1','localhost'],
                      MAX_CONTENT_LENGTH=1024*1024)
    manifest_url = os.environ.get('SEL_UPDATE_MANIFEST_URL', '').strip() or DEFAULT_UPDATE_MANIFEST_URL
    updater = SafeUpdateCoordinator(manifest_url=manifest_url, current_version=APP_VERSION)
    app.extensions['safeupdate'] = updater

    @app.before_request
    def open_connection_and_authorize():
        g.db=db.connect()
        if '_csrf' not in session:
            session['_csrf']=secrets.token_urlsafe(32)
        if request.method=='POST' and not hmac.compare_digest(request.form.get('_csrf',''), session['_csrf']):
            abort(400, 'Session expirée. Rechargez la page avant de soumettre le formulaire.')

    @app.after_request
    def headers(response):
        response.headers['X-Content-Type-Options']='nosniff'
        response.headers['X-Frame-Options']='DENY'
        response.headers['Referrer-Policy']='no-referrer'
        response.headers['Cache-Control']='no-store'
        response.headers['Content-Security-Policy']=(
           "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; "
           "font-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'")
        return response

    @app.errorhandler(400)
    def bad_request(error):
        return render_template('error.html', title='Requête impossible', status_code=400,
                               message='La demande n’a pas pu être traitée. Rechargez la page puis réessayez.'), 400

    @app.errorhandler(404)
    def not_found(error):
        return render_template('error.html', title='Page introuvable', status_code=404,
                               message='Cette page n’existe pas ou n’est plus disponible.'), 404

    @app.errorhandler(500)
    def server_error(error):
        return render_template('error.html', title='Erreur du logiciel', status_code=500,
                               message='Une erreur inattendue est survenue. Vos données existantes n’ont pas été modifiées.'), 500

    @app.teardown_request
    def close_connection(exc):
        c=g.pop('db',None)
        if c: c.close()

    @app.context_processor
    def global_template():
        return {'csrf':lambda: session.get('_csrf',''), 'company':db.settings(g.db),
                'fmt':fmt, 'today':date.today(), 'app_version':APP_VERSION,
                'is_logged':bool(session.get('authenticated'))}

    def protected(fn):
        @wraps(fn)
        def wrapper(*a,**kw):
            if not g.db.execute('SELECT id FROM admins WHERE id=1').fetchone():
                return redirect(url_for('setup'))
            if not session.get('authenticated'):
                return redirect(url_for('login', next=request.path))
            return fn(*a,**kw)
        return wrapper

    @app.get('/health')
    def health():
        return {'app':'SEL-Facturation-Local','status':'ok','version':APP_VERSION}

    @app.get('/about')
    @protected
    def about():
        return render_template('about.html', title='À propos et mentions légales', developer=DEVELOPER_LEGAL)

    @app.get('/updates/status')
    @protected
    def update_status():
        """Return the asynchronous updater state without blocking the UI."""
        if updater.snapshot().get('state') == 'idle':
            updater.start_background_check()
        return updater.snapshot()

    @app.get('/updates')
    @protected
    def updates_view():
        if updater.snapshot().get('state') == 'idle':
            updater.start_background_check()
        return render_template('updates.html', title='Mises à jour', update=updater.snapshot())

    @app.post('/updates/check')
    @protected
    def updates_check():
        updater.start_check()
        return redirect(url_for('updates_view'))

    @app.post('/updates/download')
    @protected
    def updates_download():
        if updater.snapshot().get('state') != 'available':
            flash('Aucune mise à jour prête à être téléchargée.','error')
            return redirect(url_for('updates_view'))
        if not updater.start_download():
            flash('Une préparation de mise à jour est déjà en cours.','error')
        return redirect(url_for('updates_view'))

    @app.post('/updates/cancel')
    @protected
    def updates_cancel():
        updater.cancel()
        return redirect(url_for('updates_view'))

    @app.route('/setup',methods=['GET','POST'])
    def setup():
        if g.db.execute('SELECT id FROM admins WHERE id=1').fetchone():
            return redirect(url_for('login'))
        if request.method=='POST':
            password=request.form.get('password','')
            confirm=request.form.get('confirm','')
            if len(password)<10 or password!=confirm:
                flash('Choisissez un mot de passe d’au moins 10 caractères et confirmez-le.','error')
            else:
                salt=secrets.token_hex(16)
                try:
                    g.db.execute('INSERT INTO admins(id,salt,hash) VALUES(1,?,?)',(salt,password_hash(password,salt)))
                    g.db.commit()
                except sqlite3.IntegrityError:
                    return redirect(url_for('login'))
                session.clear()
                flash('Compte administrateur créé. Connectez-vous.','success')
                return redirect(url_for('login'))
        return render_template('setup.html',title='Première configuration')

    @app.route('/login',methods=['GET','POST'])
    def login():
        if not g.db.execute('SELECT id FROM admins WHERE id=1').fetchone():
            return redirect(url_for('setup'))
        if request.method=='POST':
            admin=g.db.execute('SELECT salt,hash FROM admins WHERE id=1').fetchone()
            if hmac.compare_digest(password_hash(request.form.get('password',''),admin['salt']),admin['hash']):
                session.clear();session['authenticated']=True;session['_csrf']=secrets.token_urlsafe(32)
                session.permanent=True
                return redirect(url_for('dashboard'))
            time.sleep(0.5)
            flash('Mot de passe incorrect.','error')
        return render_template('login.html',title='Connexion')

    @app.post('/logout')
    @protected
    def logout():
        session.clear()
        return redirect(url_for('login'))

    @app.get('/')
    @protected
    def dashboard():
        invoice_rows=g.db.execute('SELECT status,currency,calc_snapshot FROM invoices').fetchall()
        conf=db.settings(g.db)
        currency=conf.get('currency')
        issued_all=[row for row in invoice_rows if row['status']=='ISSUED']
        issued_rows=[row for row in issued_all if row['currency']==currency]
        # An older installation may still have CDF in settings while the
        # current SEL tariff model emits USD invoices.  Do not hide those
        # invoices merely because the legacy display preference survived an
        # additive migration.  We only fall back when the data has one
        # unambiguous currency; mixed-currency history remains explicit.
        currencies={row['currency'] for row in issued_all}
        if not issued_rows and len(currencies)==1:
            currency=next(iter(currencies))
            issued_rows=list(issued_all)
        billed=sum_money(json.loads(row['calc_snapshot'])['total'] for row in issued_rows)
        clients=g.db.execute("SELECT COUNT(*) AS n FROM clients WHERE status='ACTIVE'").fetchone()['n']
        recent=g.db.execute('''SELECT i.*, c.name as client_name FROM invoices i
          JOIN clients c ON c.id=i.client_id ORDER BY i.id DESC LIMIT 8''').fetchall()
        paid=sum_money(row['amount'] for row in g.db.execute('''SELECT p.amount FROM payments p JOIN invoices i ON i.id=p.invoice_id
          WHERE i.status='ISSUED' AND i.currency=?''', (currency,)))
        reversed_total=sum_money(row['amount'] for row in g.db.execute('''SELECT r.amount FROM payment_reversals r
          JOIN payments p ON p.id=r.payment_id JOIN invoices i ON i.id=p.invoice_id
          WHERE i.status='ISSUED' AND i.currency=?''',(currency,)))
        paid=money(paid-reversed_total)
        backups=sorted((db.home()/'sauvegardes').glob('sel-*.sqlite3'), key=lambda path: path.stat().st_mtime, reverse=True)
        stats={'n':len(issued_rows),'billed':billed,'paid':paid,'balance':money(billed-paid),
               'currency':currency,
               'configured':conf.get('configured')=='1','last_backup':backups[0].name if backups else None}
        return render_template('dashboard.html',title='Tableau de bord',stats=stats,clients=clients,recent=recent)

    @app.route('/clients',methods=['GET','POST'])
    @protected
    def clients():
        customer_types = active_customer_types(g.db)
        customer_type_codes = {item['code'] for item in customer_types}
        client_form=session.get('client_form',{})
        if request.method=='POST':
            values={k:request.form.get(k,'').strip() for k in ('name','address','category','client_type','customer_type','billing_frequency','contract_number','meter_number','center','service_point','nif','phone','coefficient','water_connection_status')}
            values['water_connection_status']=values['water_connection_status'] or 'UNKNOWN'
            try:
                if not values['name'] or not values['address']:
                    raise BillingError('Nom et adresse sont obligatoires.')
                if values['customer_type'] and values['customer_type'] not in customer_type_codes:
                    raise BillingError('Type d’abonné invalide.')
                if values['water_connection_status'] not in {'UNKNOWN','CONNECTED','DISCONNECTED'}:
                    raise BillingError('Sélectionnez un état de raccordement à l’eau valide.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80: raise BillingError('Nom ou numéro de compteur trop long.')
                g.db.execute('BEGIN IMMEDIATE')
                next_id=g.db.execute('SELECT COALESCE(MAX(id),0)+1 FROM clients').fetchone()[0]
                code=f'SEL-C-{next_id:05d}'
                g.db.execute('''INSERT INTO clients(code,name,address,category,client_type,customer_type,billing_frequency,contract_number,meter_number,center,service_point,nif,phone,coefficient,water_enabled,water_connection_status)
                     VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)''',(code,values['name'],values['address'],values['category'] or '',values['client_type'] or '',values['customer_type'],values['billing_frequency'] or 'MONTHLY',values['contract_number'],values['meter_number'],values['center'] or 'KOLWEZI',values['service_point'],values['nif'],values['phone'],values['coefficient'] or '1',int(values['water_connection_status']=='CONNECTED'),values['water_connection_status']))
                g.db.commit();session.pop('client_form',None);flash('Client enregistré.','success');return redirect(url_for('clients'))
            except (BillingError,sqlite3.IntegrityError) as e:
                g.db.rollback();session['client_form']=values;client_form=values;flash(str(e),'error')
        term=request.args.get('q','').strip()[:120]
        status=request.args.get('status','ACTIVE').strip().upper()
        if status not in ('ACTIVE','ARCHIVED','ALL'):
            status='ACTIVE'
        page=page_number(request.args.get('page','1'))
        offset=(page-1)*PAGE_SIZE
        search_args=[]
        status_clause='' if status == 'ALL' else 'status=?'
        if status_clause:
            search_args.append(status)
        if term:
            search_clause='(name LIKE ? OR code LIKE ? OR meter_number LIKE ? OR contract_number LIKE ?)'
            search_args=['%'+term+'%']*4
            if status_clause:
                search_args.insert(0,status)
                where=f'WHERE {status_clause} AND {search_clause}'
            else:
                where=f'WHERE {search_clause}'
        else:
            where=f'WHERE {status_clause}' if status_clause else ''
        total=g.db.execute(f'SELECT COUNT(*) FROM clients {where}',search_args).fetchone()[0]
        result=g.db.execute(f'SELECT * FROM clients {where} ORDER BY id DESC LIMIT ? OFFSET ?',search_args+[PAGE_SIZE,offset]).fetchall()
        pages=max(1,(total+PAGE_SIZE-1)//PAGE_SIZE)
        return render_template('clients.html',title='Abonnés',clients=result,query=term,status=status,page=page,pages=pages,total=total,customer_types=customer_types,client_form=client_form)

    @app.route('/clients/<int:cid>/edit',methods=['GET','POST'])
    @protected
    def edit_client(cid):
        current=g.db.execute('SELECT * FROM clients WHERE id=?',(cid,)).fetchone()
        if not current: abort(404)
        draft_key=f'client_edit_{cid}'
        form_client=dict(current)
        form_client.update(session.get(draft_key,{}))
        all_types = all_customer_types(g.db)
        customer_type_codes = {item['code'] for item in all_types if item['active'] or item['code'] == current['customer_type']}
        if request.method=='POST':
            editable=('name','address','category','client_type','customer_type','billing_frequency','contract_number','meter_number','center','service_point','nif','phone','coefficient')
            values={k:request.form.get(k, current[k] or '').strip() for k in editable}
            values['water_connection_status']=request.form.get('water_connection_status',current['water_connection_status']).strip()
            try:
                if not values['name'] or not values['address']: raise BillingError('Nom et adresse obligatoires.')
                if values['customer_type'] and values['customer_type'] not in customer_type_codes: raise BillingError('Type d’abonné invalide.')
                if values['water_connection_status'] not in {'UNKNOWN','CONNECTED','DISCONNECTED'}: raise BillingError('Sélectionnez un état de raccordement à l’eau valide.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80:
                    raise BillingError('Nom ou numéro de compteur trop long.')
                legacy_water_enabled=(int(values['water_connection_status']=='CONNECTED')
                                      if values['water_connection_status']!='UNKNOWN'
                                      else int(current['water_enabled']))
                g.db.execute('''UPDATE clients SET name=?,address=?,category=?,client_type=?,customer_type=?,billing_frequency=?,contract_number=?,meter_number=?,center=?,service_point=?,nif=?,phone=?,coefficient=?,water_enabled=?,water_connection_status=? WHERE id=?''',(*[values[k] for k in editable],legacy_water_enabled,values['water_connection_status'],cid))
                g.db.commit();session.pop(draft_key,None);flash('Fiche mise à jour. Les anciennes factures conservent leurs données.','success');return redirect(url_for('clients'))
            except BillingError as e:
                session[draft_key]=values;form_client=dict(current);form_client.update(values);flash(str(e),'error')
        return render_template('client_edit.html',title='Modifier le client',client=form_client,customer_types=all_types,client_draft_active=bool(session.get(draft_key)))

    @app.post('/clients/<int:cid>/archive')
    @protected
    def archive_client(cid):
        reason=request.form.get('reason','').strip()[:240]
        if len(reason)<3:
            flash('Un motif d’archivage est obligatoire.','error')
            return redirect(url_for('edit_client',cid=cid))
        with g.db:
            row=g.db.execute('SELECT status FROM clients WHERE id=?',(cid,)).fetchone()
            if not row: abort(404)
            if row['status']=='ARCHIVED':
                flash('Cet abonné est déjà archivé.','error')
            else:
                g.db.execute("UPDATE clients SET status='ARCHIVED',archived_at=?,archive_reason=? WHERE id=?",(date.today().isoformat(),reason,cid))
                flash('Abonné archivé. Il ne sera plus proposé pour une nouvelle facturation.','success')
        return redirect(url_for('clients',status='ARCHIVED'))

    @app.post('/clients/<int:cid>/restore')
    @protected
    def restore_client(cid):
        with g.db:
            row=g.db.execute('SELECT customer_type,status FROM clients WHERE id=?',(cid,)).fetchone()
            if not row: abort(404)
            if row['customer_type'] and not g.db.execute('SELECT 1 FROM customer_types WHERE code=? AND active=1',(row['customer_type'],)).fetchone():
                flash('Impossible de restaurer : le type d’abonné est inactif. Réactivez-le d’abord.','error')
            else:
                g.db.execute("UPDATE clients SET status='ACTIVE',archived_at='',archive_reason='' WHERE id=?",(cid,))
                flash('Abonné restauré.','success')
        return redirect(url_for('clients'))

    @app.post('/clients/<int:cid>/delete')
    @protected
    def delete_client(cid):
        with g.db:
            row=g.db.execute('SELECT name FROM clients WHERE id=?',(cid,)).fetchone()
            if not row: abort(404)
            dependency=g.db.execute('SELECT 1 FROM invoices WHERE client_id=? LIMIT 1',(cid,)).fetchone()
            if dependency:
                flash('Suppression refusée : cet abonné possède un historique de décomptes. Archivez-le à la place.','error')
                return redirect(url_for('edit_client',cid=cid))
            g.db.execute('DELETE FROM clients WHERE id=?',(cid,))
            flash('Abonné supprimé définitivement car aucun document historique ne le référence.','success')
        return redirect(url_for('clients'))

    @app.route('/settings',methods=['GET','POST'])
    @protected
    def settings_view():
        conf=db.settings(g.db)
        customer_types=all_customer_types(g.db)
        tariffs=current_tariffs(g.db)
        settings_tabs={'overview','identity','tariffs','customer-types','catalogs','operations','history'}
        section_tabs={'identity':'identity','tariffs':'tariffs','customer_type':'customer-types',
                      'customer_type_edit':'customer-types','currency':'catalogs',
                      'payment_method':'catalogs','tax_profile':'catalogs',
                      'tariff':'catalogs','exchange_rate':'catalogs','operations':'operations'}
        settings_drafts=dict(session.get('settings_drafts',{}))
        requested_tab=request.args.get('tab','').strip()
        if requested_tab in settings_tabs:
            active_tab=requested_tab
        else:
            active_tab=next((section_tabs.get(section,'overview') for section in settings_drafts if section_tabs.get(section) in settings_tabs),'overview')

        def settings_redirect(tab):
            selected=tab if tab in settings_tabs else 'overview'
            return redirect(url_for('settings_view', tab=selected) + f'#{selected}')

        def clear_settings_draft(section):
            drafts=dict(session.get('settings_drafts',{}))
            drafts.pop(section,None)
            if drafts:
                session['settings_drafts']=drafts
            else:
                session.pop('settings_drafts',None)

        try:
            current_exchange_rate=db.exchange_rate_for(g.db,'USD','CDF',date.today().isoformat())['rate']
        except ValueError:
            current_exchange_rate=conf.get('exchange_rate_usd_cdf','2360')
        currencies=all_currencies(g.db)
        payment_methods=all_payment_methods(g.db)
        tax_profiles=all_tax_profiles(g.db)
        tariffs_history=tariff_history(g.db)
        exchange_rates=exchange_rate_history(g.db)
        changes=configuration_history(g.db)
        if request.method=='POST':
            section=request.form.get('section','tariffs')
            requested_return_tab=request.form.get('return_tab','').strip()
            active_tab=requested_return_tab if requested_return_tab in settings_tabs else section_tabs.get(section,'overview')
            fields=['company_name','tagline','address','phone','email','website','rccm','nif','id_nat','vat_rate','residential_rate','commercial_rate','exchange_rate_usd_cdf']
            values={k:request.form.get(k,'').strip() for k in fields}
            section_values={k:request.form.get(k,'').strip() for k in request.form.keys() if k not in ('_csrf','section','return_tab')}
            if section == 'customer_type':
                section_values['active']='yes' if request.form.get('active')=='yes' else ''
            try:
                if section == 'operations':
                    prefix=request.form.get('invoice_prefix','').strip().upper()
                    if not re.fullmatch(r'[A-Z0-9][A-Z0-9_-]{0,15}', prefix):
                        raise BillingError('Le préfixe doit contenir 1 à 16 caractères A-Z, chiffres, tiret ou underscore.')
                    try:
                        width=max(1,min(12,int(request.form.get('invoice_sequence_width','6'))))
                        retention=max(1,min(365,int(request.form.get('backup_retention_count','14'))))
                    except ValueError:
                        raise BillingError('La largeur de numérotation et la rétention doivent être des nombres entiers.')
                    with g.db:
                        for key,value in (('invoice_prefix',prefix),('invoice_sequence_width',str(width)),('backup_retention_count',str(retention))):
                            old=g.db.execute('SELECT value FROM billing_policy WHERE key=?',(key,)).fetchone()
                            record_change(g.db,'operations',key,old['value'] if old else '',value,'Modification depuis Paramètres')
                            g.db.execute('INSERT INTO billing_policy(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value',(key,value))
                    flash('Politique opérationnelle mise à jour.','success')
                    clear_settings_draft(section)
                    return settings_redirect(active_tab)
                if section == 'customer_type':
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    if not re.fullmatch(r'[A-Z][A-Z0-9_]{1,31}', code):
                        raise BillingError('Le code doit contenir 2 à 32 caractères A-Z, chiffres ou underscore.')
                    if not label or len(label)>80:
                        raise BillingError('Le libellé du type d’abonné est obligatoire et limité à 80 caractères.')
                    active=request.form.get('active')=='yes'
                    initial_rate=decimal(request.form.get('initial_rate',''),'Tarif initial',allow_zero=False) if active else None
                    confirmed=request.form.get('confirmed')=='yes'
                    if active and not confirmed:
                        raise BillingError('Confirmez que le tarif initial est validé par SEL avant d’activer ce type.')
                    effective_from=date.today().replace(day=1).isoformat()
                    with g.db:
                        g.db.execute("INSERT INTO customer_types(code,label,active,sort_order) VALUES(?,?,?,?)", (code,label,1 if active else 0,100))
                        record_change(g.db,'customer_types',code,'',label,'Création depuis Paramètres')
                        if active:
                            g.db.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,source,confirmed) VALUES(?,?,?,?,?,?,?,1,'Configuration SEL',1)", (f'{code}-{effective_from[:7]}',code,'KWH',str(initial_rate),'USD','16',effective_from))
                            record_change(g.db,'tariffs',code,'',str(initial_rate),'Tarif initial depuis Paramètres')
                    flash('Type d’abonné ajouté.','success')
                    clear_settings_draft(section)
                    return settings_redirect(active_tab)
                if section == 'customer_type_edit':
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    active=1 if request.form.get('active')=='yes' else 0
                    try:
                        sort_order=int(request.form.get('sort_order','100'))
                    except ValueError:
                        raise BillingError('L’ordre d’affichage doit être un nombre entier.')
                    if not re.fullmatch(r'[A-Z][A-Z0-9_]{1,31}', code):
                        raise BillingError('Code de type d’abonné invalide.')
                    if not label or len(label)>80:
                        raise BillingError('Le libellé du type d’abonné est obligatoire et limité à 80 caractères.')
                    existing=g.db.execute('SELECT label,active,sort_order FROM customer_types WHERE code=?',(code,)).fetchone()
                    if not existing: raise BillingError('Type d’abonné introuvable.')
                    with g.db:
                        record_change(g.db,'customer_types',code,str(existing['label']),label,'Modification depuis Paramètres')
                        g.db.execute('UPDATE customer_types SET label=?,active=?,sort_order=? WHERE code=?',(label,active,sort_order,code))
                    flash('Type d’abonné mis à jour.','success')
                    clear_settings_draft(section)
                    return settings_redirect(active_tab)
                if section == 'currency':
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    active=1 if request.form.get('active')=='yes' else 0
                    try:
                        decimals=int(request.form.get('decimals','2'))
                        sort_order=int(request.form.get('sort_order','100'))
                    except ValueError:
                        raise BillingError('Les décimales et l’ordre doivent être des nombres entiers.')
                    if not re.fullmatch(r'[A-Z][A-Z0-9]{2,9}', code):
                        raise BillingError('Code devise invalide.')
                    if not label or not 0 <= decimals <= 6:
                        raise BillingError('Libellé ou nombre de décimales invalide.')
                    configured_currency=conf.get('currency','USD')
                    if code==configured_currency and not active:
                        raise BillingError('La devise principale configurée ne peut pas être désactivée.')
                    old=g.db.execute('SELECT label,active,decimals,sort_order FROM currency_catalog WHERE code=?',(code,)).fetchone()
                    with g.db:
                        if old:
                            record_change(g.db,'currencies',code,str(old['label']),label,'Modification depuis Paramètres')
                            g.db.execute('UPDATE currency_catalog SET label=?,decimals=?,active=?,sort_order=? WHERE code=?',(label,decimals,active,sort_order,code))
                        else:
                            g.db.execute('INSERT INTO currency_catalog(code,label,decimals,active,sort_order) VALUES(?,?,?,?,?)',(code,label,decimals,active,sort_order))
                            record_change(g.db,'currencies',code,'',label,'Création depuis Paramètres')
                    flash('Devise enregistrée.','success')
                    clear_settings_draft(section)
                    return settings_redirect('catalogs')
                if section == 'payment_method':
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    active=1 if request.form.get('active')=='yes' else 0
                    required=1 if request.form.get('reference_required')=='yes' else 0
                    try:
                        sort_order=int(request.form.get('sort_order','100'))
                    except ValueError:
                        raise BillingError('L’ordre d’affichage doit être un nombre entier.')
                    if not re.fullmatch(r'[A-Z][A-Z0-9_]{1,31}', code) or not label:
                        raise BillingError('Code et libellé de mode de paiement obligatoires.')
                    old=g.db.execute('SELECT label FROM payment_methods WHERE code=?',(code,)).fetchone()
                    with g.db:
                        if old:
                            record_change(g.db,'payment_methods',code,str(old['label']),label,'Modification depuis Paramètres')
                            g.db.execute('UPDATE payment_methods SET label=?,reference_required=?,active=?,sort_order=? WHERE code=?',(label,required,active,sort_order,code))
                        else:
                            g.db.execute('INSERT INTO payment_methods(code,label,reference_required,active,sort_order) VALUES(?,?,?,?,?)',(code,label,required,active,sort_order))
                            record_change(g.db,'payment_methods',code,'',label,'Création depuis Paramètres')
                    flash('Mode de paiement enregistré.','success')
                    clear_settings_draft(section)
                    return settings_redirect('catalogs')
                if section == 'tariff':
                    service_code=request.form.get('service_code','ELECTRICITY').strip().upper()
                    customer_type=request.form.get('customer_type','').strip().upper()
                    unit_price=decimal(request.form.get('unit_price',''),'Tarif',allow_zero=False)
                    currency=request.form.get('currency','USD').strip().upper()
                    vat_rate=decimal(request.form.get('vat_rate','16'),'TVA')
                    unit=request.form.get('unit','KWH').strip().upper()
                    effective_from=request.form.get('effective_from','').strip()
                    source=request.form.get('source','').strip()[:160]
                    confirmed=request.form.get('confirmed')=='yes'
                    try:
                        date.fromisoformat(effective_from)
                    except ValueError:
                        raise BillingError('Date d’application du tarif invalide.')
                    expected_unit={'ELECTRICITY':'KWH','WATER':'M3'}.get(service_code)
                    if expected_unit is None or unit != expected_unit:
                        raise BillingError('Service ou unité de tarif invalide.')
                    if service_code == 'WATER' and (not confirmed or len(source)<5):
                        raise BillingError('Pour l’eau, indiquez la source SEL et confirmez le tarif avant son activation.')
                    if service_code == 'ELECTRICITY':
                        source=source or 'Configuration SEL'
                        confirmed=True
                    if not g.db.execute('SELECT 1 FROM customer_types WHERE code=?',(customer_type,)).fetchone():
                        raise BillingError('Type d’abonné introuvable.')
                    if not g.db.execute('SELECT 1 FROM currency_catalog WHERE code=? AND active=1',(currency,)).fetchone():
                        raise BillingError('Devise inactive ou inconnue.')
                    existing=g.db.execute('SELECT 1 FROM tariff_plans WHERE service_code=? AND customer_type=? AND effective_from=?',(service_code,customer_type,effective_from)).fetchone()
                    if existing:
                        raise BillingError('Un tarif existe déjà pour ce type et cette date. Créez une nouvelle date d’application.')
                    prefix=f'{service_code}-{customer_type}-{effective_from[:7].replace("-","")}'
                    count=g.db.execute('SELECT COUNT(*) FROM tariff_plans WHERE code LIKE ?',(prefix+'%',)).fetchone()[0]+1
                    code=f'{prefix}-{count}'
                    with g.db:
                        g.db.execute('INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) VALUES(?,?,?,?,?,?,?,1,?,?,?)',(code,customer_type,unit,str(unit_price),currency,str(vat_rate),effective_from,service_code,source,1 if confirmed else 0))
                        record_change(g.db,'tariffs',code,'',str(unit_price),f'Création datée depuis Paramètres · {service_code} · {source}')
                    flash('Tarif daté enregistré.','success')
                    clear_settings_draft(section)
                    return settings_redirect('catalogs')
                if section == 'exchange_rate':
                    base=request.form.get('base_currency','').strip().upper()
                    quote=request.form.get('quote_currency','').strip().upper()
                    rate=decimal(request.form.get('rate',''),'Taux de change',allow_zero=False)
                    effective_on=request.form.get('effective_on','').strip()
                    source=request.form.get('source','').strip()[:120]
                    try:
                        date.fromisoformat(effective_on)
                    except ValueError:
                        raise BillingError('Date d’application du taux invalide.')
                    if base==quote or not source:
                        raise BillingError('Paire de devises ou source invalide.')
                    if not g.db.execute('SELECT 1 FROM currency_catalog WHERE code=? AND active=1',(base,)).fetchone() or not g.db.execute('SELECT 1 FROM currency_catalog WHERE code=? AND active=1',(quote,)).fetchone():
                        raise BillingError('Les deux devises doivent exister et être actives dans le référentiel.')
                    old=g.db.execute('SELECT rate FROM exchange_rates WHERE base_currency=? AND quote_currency=? AND effective_on=?',(base,quote,effective_on)).fetchone()
                    with g.db:
                        g.db.execute('''INSERT INTO exchange_rates(base_currency,quote_currency,rate,effective_on,source) VALUES(?,?,?,?,?)
                                        ON CONFLICT(base_currency,quote_currency,effective_on) DO UPDATE SET rate=excluded.rate,source=excluded.source''',(base,quote,str(rate),effective_on,source))
                        record_change(g.db,'exchange_rates',f'{base}/{quote}/{effective_on}',old['rate'] if old else '',str(rate),'Enregistrement depuis Paramètres')
                    flash('Taux de change enregistré.','success')
                    clear_settings_draft(section)
                    return settings_redirect('catalogs')
                if section == 'tax_profile':
                    service_code=request.form.get('service_code','ELECTRICITY').strip().upper()
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    vat_base=request.form.get('service_vat_base',request.form.get('vat_base','ENERGY')).strip().upper()
                    effective_from=request.form.get('effective_from','').strip()
                    active=1 if request.form.get('active')=='yes' else 0
                    source=request.form.get('source','').strip()[:160]
                    confirmed=request.form.get('confirmed')=='yes'
                    vat_rate=decimal(request.form.get('vat_rate',''),'TVA')
                    local_rate=decimal(request.form.get('service_local_rate',request.form.get('local_rate','0')),'Taxe locale')
                    try:
                        date.fromisoformat(effective_from)
                    except ValueError:
                        raise BillingError('Date d’application du profil fiscal invalide.')
                    if not re.fullmatch(r'[A-Z][A-Z0-9_]{1,31}', code):
                        raise BillingError('Code de profil fiscal invalide.')
                    if not label or len(label)>100:
                        raise BillingError('Le libellé du profil fiscal est obligatoire et limité à 100 caractères.')
                    allowed_bases={'ELECTRICITY':('ENERGY','ENERGY_LOCAL','ENERGY_LOCAL_FEES'),
                                   'WATER':('NONE','SUBTOTAL','SUBTOTAL_LOCAL')}
                    if service_code not in allowed_bases or vat_base not in allowed_bases[service_code]:
                        raise BillingError('Base de TVA invalide.')
                    if service_code == 'WATER' and (not confirmed or len(source)<5):
                        raise BillingError('Pour l’eau, indiquez la source SEL et confirmez la règle fiscale avant son activation.')
                    if service_code == 'ELECTRICITY':
                        source=source or 'Configuration SEL'
                        confirmed=True
                    if vat_rate>100 or local_rate>100:
                        raise BillingError('Les taux fiscaux doivent être compris entre 0 et 100 %.')
                    old=g.db.execute('SELECT * FROM tax_profiles WHERE code=?',(code,)).fetchone()
                    with g.db:
                        if old:
                            old_value=f"{old['label']}|{old['vat_rate']}|{old['vat_base']}|{old['local_rate']}|{old['effective_from']}|{old['active']}|{old['service_code']}"
                            new_value=f"{label}|{vat_rate}|{vat_base}|{local_rate}|{effective_from}|{active}|{service_code}"
                            record_change(g.db,'tax_profiles',code,old_value,new_value,'Modification depuis Paramètres')
                            g.db.execute('''UPDATE tax_profiles SET label=?,vat_rate=?,vat_base=?,local_rate=?,effective_from=?,active=?,service_code=?,source=?,confirmed=? WHERE code=?''',
                                         (label,str(vat_rate),vat_base,str(local_rate),effective_from,active,service_code,source,1 if confirmed else 0,code))
                        else:
                            g.db.execute('''INSERT INTO tax_profiles(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed)
                                            VALUES(?,?,?,?,?,?,?,?,?,?)''',
                                         (code,label,str(vat_rate),vat_base,str(local_rate),effective_from,active,service_code,source,1 if confirmed else 0))
                            record_change(g.db,'tax_profiles',code,'',label,'Création depuis Paramètres')
                    flash('Profil fiscal enregistré.','success')
                    clear_settings_draft(section)
                    return settings_redirect('catalogs')
                if section == 'identity' and (not values['company_name'] or not values['address']):
                    raise BillingError('Société et adresse obligatoires.')
                if section == 'identity':
                    identity_fields=('company_name','tagline','address','phone','email','website','rccm','nif','id_nat')
                    with g.db:
                        for k in identity_fields: g.db.execute('UPDATE settings SET value=? WHERE key=?',(values[k],k))
                    flash('Identité SEL mise à jour. Les anciennes factures restent inchangées.','success')
                    clear_settings_draft(section)
                    return settings_redirect(active_tab)
                if section != 'tariffs':
                    raise BillingError('Section de paramètres invalide.')
                legacy_rates={'RESIDENTIAL': 'residential_rate', 'COMMERCIAL': 'commercial_rate'}
                submitted_rates={}
                for customer_type in customer_types:
                    code=customer_type['code']
                    raw=request.form.get(f'tariff_rate_{code}','').strip()
                    if not raw and code in legacy_rates:
                        raw=values[legacy_rates[code]]
                    submitted_rates[code]=decimal(raw, f"Tarif {customer_type['label']}", allow_zero=False)
                rate=decimal(values['exchange_rate_usd_cdf'],'Taux USD/CDF',allow_zero=False)
                if values['vat_rate'] != '16':
                    raise BillingError('La TVA SEL doit être fixée à 16 %.')
                if rate <= 0: raise BillingError('Le taux USD/CDF doit être positif.')
                if request.form.get('confirmed')!='yes':
                    raise BillingError('Confirmez les tarifs officiels SEL avant d’activer la facturation.')
                with g.db:
                    g.db.execute("UPDATE settings SET value='0' WHERE key='local_rate'")
                    g.db.execute("UPDATE settings SET value='16' WHERE key='vat_rate'")
                    g.db.execute("UPDATE settings SET value='ENERGY' WHERE key='vat_base'")
                    g.db.execute("UPDATE settings SET value='USD' WHERE key='currency'")
                    g.db.execute("UPDATE settings SET value=? WHERE key='exchange_rate_usd_cdf'", (str(rate),))
                    g.db.execute("UPDATE settings SET value=? WHERE key='exchange_rate_date'", (date.today().isoformat(),))
                    previous_rate=g.db.execute("SELECT rate FROM exchange_rates WHERE base_currency='USD' AND quote_currency='CDF' AND effective_on=?", (date.today().isoformat(),)).fetchone()
                    record_change(g.db, 'exchange_rates', 'USD/CDF', previous_rate['rate'] if previous_rate else '', str(rate), 'Modification depuis Paramètres')
                    g.db.execute("INSERT INTO exchange_rates(base_currency,quote_currency,rate,effective_on,source) VALUES('USD','CDF',?,?,?) ON CONFLICT(base_currency,quote_currency,effective_on) DO UPDATE SET rate=excluded.rate,source=excluded.source", (str(rate),date.today().isoformat(),'SEL'))
                    effective_from = date.today().replace(day=1).isoformat()
                    for customer_type in customer_types:
                        code=customer_type['code']
                        tariff_code=f"{code}-{effective_from[:7]}"
                        previous=g.db.execute("SELECT unit_price FROM tariff_plans WHERE code=?", (tariff_code,)).fetchone()
                        if previous:
                            record_change(g.db, 'tariffs', code, previous['unit_price'], str(submitted_rates[code]), 'Modification depuis Paramètres')
                            g.db.execute("UPDATE tariff_plans SET unit_price=?,currency='USD',vat_rate='16',active=1,source='Configuration SEL',confirmed=1 WHERE code=?", (str(submitted_rates[code]),tariff_code))
                        else:
                            record_change(g.db, 'tariffs', code, '', str(submitted_rates[code]), 'Création depuis Paramètres')
                            g.db.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,source,confirmed) VALUES(?,?,?,?,?,?,?,1,'Configuration SEL',1)", (tariff_code,code,'KWH',str(submitted_rates[code]),'USD','16',effective_from))
                    g.db.execute("UPDATE settings SET value='1' WHERE key='configured'")
                flash('Paramètres validés. Les anciennes factures restent inchangées.','success')
                clear_settings_draft(section)
                return settings_redirect(active_tab)
            except (BillingError, sqlite3.IntegrityError) as e:
                settings_drafts=dict(session.get('settings_drafts',{}));settings_drafts[section]=section_values;session['settings_drafts']=settings_drafts
                flash(str(e),'error')
            conf.update(values)
        return render_template('settings.html',title='Paramètres SEL',settings=conf,customer_types=customer_types,tariffs=tariffs,policy=billing_policy(g.db),exchange_rate=current_exchange_rate,active_tab=active_tab,settings_drafts=settings_drafts if request.method=='POST' else dict(session.get('settings_drafts',{})),currencies=currencies,payment_methods=payment_methods,tax_profiles=tax_profiles,tariffs_history=tariffs_history,exchange_rates=exchange_rates,changes=changes)

    @app.post('/settings/catalogs/<kind>/<key>/delete')
    @protected
    def delete_settings_catalog(kind, key):
        """Hard-delete an unused reference only after transactional dependency checks."""
        allowed={'currency','payment_method','customer_type','tax_profile','tariff'}
        if kind not in allowed:
            abort(404)
        try:
            with g.db:
                table={'currency':'currency_catalog','payment_method':'payment_methods',
                       'customer_type':'customer_types','tax_profile':'tax_profiles',
                       'tariff':'tariff_plans'}[kind]
                field='code'
                row=g.db.execute(f'SELECT * FROM {table} WHERE {field}=?',(key,)).fetchone()
                if not row:
                    flash('Ce référentiel n’existe plus.','error')
                    return redirect(url_for('settings_view',tab='catalogs'))
                if kind=='currency':
                    references=[]
                    if db.settings(g.db).get('currency')==key:
                        references.append('devise principale')
                    if g.db.execute('SELECT 1 FROM billing_policy WHERE value=? LIMIT 1',(key,)).fetchone():
                        references.append('politique de facturation')
                    if g.db.execute('SELECT 1 FROM tariff_plans WHERE currency=? LIMIT 1',(key,)).fetchone():
                        references.append('barème')
                    if g.db.execute('SELECT 1 FROM exchange_rates WHERE base_currency=? OR quote_currency=? LIMIT 1',(key,key)).fetchone():
                        references.append('taux de change')
                    if g.db.execute('SELECT 1 FROM invoices WHERE currency=? LIMIT 1',(key,)).fetchone():
                        references.append('facture historique')
                    if references:
                        raise BillingError('Suppression refusée : cette devise est liée à ' + ', '.join(references) + '. Désactivez-la si elle ne doit plus être proposée.')
                elif kind=='payment_method':
                    if g.db.execute('SELECT 1 FROM payments WHERE method IN (?,?) LIMIT 1',(key,row['label'])).fetchone():
                        raise BillingError('Suppression refusée : un paiement historique utilise ce mode. Désactivez-le à la place.')
                elif kind=='customer_type':
                    if key in {'RESIDENTIAL','COMMERCIAL'}:
                        raise BillingError('Les types SEL de base ne peuvent pas être supprimés; vous pouvez les désactiver.')
                    if g.db.execute('SELECT 1 FROM clients WHERE customer_type=? LIMIT 1',(key,)).fetchone():
                        raise BillingError('Suppression refusée : des fiches abonnés utilisent ce type. Désactivez-le à la place.')
                    if g.db.execute('SELECT 1 FROM invoices i JOIN clients c ON c.id=i.client_id WHERE c.customer_type=? LIMIT 1',(key,)).fetchone():
                        raise BillingError('Suppression refusée : des documents historiques utilisent ce type.')
                    g.db.execute('DELETE FROM tariff_plans WHERE customer_type=?',(key,))
                elif kind=='tax_profile':
                    for invoice in g.db.execute('SELECT calc_snapshot FROM invoices'):
                        if key in invoice['calc_snapshot']:
                            raise BillingError('Suppression refusée : un snapshot de facture référence ce profil fiscal.')
                elif kind=='tariff':
                    customer_type=row['customer_type']
                    if g.db.execute('SELECT 1 FROM invoices i JOIN clients c ON c.id=i.client_id WHERE c.customer_type=? AND substr(i.billing_period,1,7)>=substr(?,1,7) LIMIT 1',(customer_type,row['effective_from'])).fetchone():
                        raise BillingError('Suppression refusée : une facture peut dépendre de ce tarif. Désactivez-le au lieu de l’effacer.')
                g.db.execute(f'DELETE FROM {table} WHERE {field}=?',(key,))
            flash('Référentiel supprimé; aucun document historique ne le référence.','success')
        except BillingError as error:
            flash(str(error),'error')
        return redirect(url_for('settings_view',tab='catalogs'))

    @app.get('/invoices')
    @protected
    def invoices():
        term=request.args.get('q','').strip()[:100]
        status=request.args.get('status','')
        page=page_number(request.args.get('page','1'))
        offset=(page-1)*PAGE_SIZE
        clause=[];args=[]
        if term:
            clause.append('(i.number LIKE ? OR c.name LIKE ? OR i.billing_period LIKE ?)')
            args.extend(['%'+term+'%']*3)
        if status in ('ISSUED','VOID'):
            clause.append('i.status=?');args.append(status)
        where='WHERE '+' AND '.join(clause) if clause else ''
        total=g.db.execute(f'''SELECT COUNT(*) FROM invoices i JOIN clients c ON c.id=i.client_id {where}''',args).fetchone()[0]
        rows=[]
        for row in g.db.execute(f'''SELECT i.*,c.name client_name
            FROM invoices i JOIN clients c ON c.id=i.client_id {where} ORDER BY i.id DESC LIMIT ? OFFSET ?''',args+[PAGE_SIZE,offset]):
            item=dict(row)
            rows.append(item)
        if rows:
            ids=[row['id'] for row in rows]
            marks=','.join('?' for _ in ids)
            paid_by_invoice={invoice_id: [] for invoice_id in ids}
            reversals_by_payment={}
            for reversal in g.db.execute(f'''SELECT p.invoice_id,r.payment_id,r.amount FROM payment_reversals r
                JOIN payments p ON p.id=r.payment_id WHERE p.invoice_id IN ({marks})''',ids):
                reversals_by_payment.setdefault(reversal['payment_id'],[]).append(reversal['amount'])
            for payment in g.db.execute(f'SELECT id,invoice_id,amount FROM payments WHERE invoice_id IN ({marks})',ids):
                reversed_amount=sum((Decimal(value) for value in reversals_by_payment.get(payment['id'],[])),Decimal('0'))
                paid_by_invoice[payment['invoice_id']].append(str(Decimal(payment['amount'])-reversed_amount))
            for item in rows:
                item['paid']=sum_money(paid_by_invoice[item['id']])
        pages=max(1,(total+PAGE_SIZE-1)//PAGE_SIZE)
        return render_template('invoices.html',title='Historique des décomptes',invoices=rows,query=term,status=status,page=page,pages=pages,total=total)

    @app.route('/invoices/new',methods=['GET','POST'])
    @protected
    def new_invoice():
        conf=db.settings(g.db)
        options=g.db.execute("SELECT * FROM clients WHERE status='ACTIVE' ORDER BY name").fetchall()
        if request.method=='POST':
            # Une nouvelle tentative invalide ne doit jamais pouvoir confirmer
            # un aperçu plus ancien resté en session.
            session.pop('invoice_draft',None)
            submitted_form={key:request.form.get(key,'').strip() for key in ('client_id','billing_period','consumption_kwh','water_m3','water_zero_confirmed','due_date','note','previous','current','fee')}
            session['invoice_form']=submitted_form
            try:
                if conf.get('configured')!='1': raise BillingError('Validez d’abord le barème et les taxes dans Paramètres.')
                cid=int(request.form.get('client_id','0'))
                period=request.form.get('billing_period','')
                if not re.fullmatch(r'\d{4}-(0[1-9]|1[0-2])',period): raise BillingError('Période invalide (AAAA-MM).')
                issue_date=date.today().isoformat()
                due=request.form.get('due_date','').strip()
                validate_due_date(issue_date, due)
                client=g.db.execute('SELECT * FROM clients WHERE id=?',(cid,)).fetchone()
                if not client: raise BillingError('Client inexistant.')
                client=dict(client)
                water_value=request.form.get('water_m3','').strip()
                if water_value and decimal(water_value,'Consommation d’eau') == 0 and request.form.get('water_zero_confirmed') != 'yes':
                    raise BillingError('Confirmez explicitement que le relevé d’eau est bien de 0 m³.')
                direct_kwh=request.form.get('consumption_kwh','').strip()
                calc=calculate_invoice_draft(g.db,client,{
                    'billing_period':period,'consumption_kwh':direct_kwh,
                    'water_m3':water_value,'previous':request.form.get('previous','').strip(),
                    'current':request.form.get('current','').strip(),'fee':request.form.get('fee','0') or '0'},conf)
                if db.active_invoice(g.db,cid,period): raise BillingError('Une facture active existe déjà pour ce client et cette période.')
                session['invoice_draft']={'client_id':cid,'billing_period':period,'previous':request.form.get('previous','').strip(),'current':request.form.get('current','').strip(),'consumption_kwh':direct_kwh,'water_m3':water_value,'water_zero_confirmed':request.form.get('water_zero_confirmed') == 'yes','fee':request.form.get('fee','').strip() or '0','due_date':due,'note':request.form.get('note','').strip()[:500], 'tariff_code':calc.get('tariff_code',''), 'tariff_effective_from':calc.get('tariff_effective_from',''), 'unit_price':calc.get('unit_price',''), 'vat_rate':calc.get('vat_rate',''), 'currency':calc.get('currency','USD'), 'calculation':calc}
                session.pop('invoice_form',None)
                return render_template('invoice_preview.html',title='Vérifier le décompte',draft=session['invoice_draft'],client=client,calc=calc,configured=True)
            except (BillingError,ValueError,sqlite3.IntegrityError) as e:
                g.db.rollback();flash(f'Émission interrompue : {e}','error')
        form_draft=session.get('invoice_form') or session.get('invoice_draft') or {}
        period=form_draft.get('billing_period') or date.today().strftime('%Y-%m')
        return render_template('new_invoice.html',title='Nouveau décompte',clients=options,period=period,configured=conf.get('configured')=='1',draft=form_draft)

    @app.post('/invoices/new/confirm')
    @protected
    def confirm_invoice():
        draft=session.get('invoice_draft')
        if not draft:
            flash('Aucun décompte en attente de confirmation.','error')
            return redirect(url_for('new_invoice'))
        conf=db.settings(g.db)
        try:
            if conf.get('configured')!='1': raise BillingError('Validez d’abord le barème et les taxes dans Paramètres.')
            cid=int(draft['client_id']); period=draft['billing_period']; issue_date=date.today().isoformat()
            validate_due_date(issue_date,draft['due_date'])
            client=g.db.execute('SELECT * FROM clients WHERE id=?',(cid,)).fetchone()
            if not client: raise BillingError('Client inexistant.')
            client=dict(client)
            water_value=draft.get('water_m3','')
            if water_value and decimal(water_value,'Consommation d’eau') == 0 and not draft.get('water_zero_confirmed'):
                raise BillingError('Le relevé d’eau à 0 m³ doit être confirmé depuis une nouvelle prévisualisation.')
            g.db.execute('BEGIN IMMEDIATE')
            fresh_calc=calculate_invoice_draft(g.db,client,draft,conf)
            if not draft.get('calculation') or json.dumps(fresh_calc,sort_keys=True,separators=(',',':')) != json.dumps(draft['calculation'],sort_keys=True,separators=(',',':')):
                raise BillingError('Les tarifs ou paramètres ont changé depuis l’aperçu. Prévisualisez de nouveau la facture.')
            calc=fresh_calc
            service_codes=[item.get('service_code') for item in calc.get('services',[])]
            if sorted(service_codes)!=['ELECTRICITY','WATER']:
                raise BillingError('Une facture doit contenir l’électricité et l’eau. Recommencez la prévisualisation.')
            if db.active_invoice(g.db,cid,period): raise BillingError('Cette période est déjà facturée pour ce client.')
            year=date.today().year
            row=g.db.execute('SELECT last_number FROM sequences WHERE year=?',(year,)).fetchone()
            next_n=(row['last_number']+1) if row else 1
            g.db.execute('INSERT INTO sequences(year,last_number) VALUES(?,?) ON CONFLICT(year) DO UPDATE SET last_number=excluded.last_number',(year,next_n))
            policy=billing_policy(g.db)
            prefix=policy.get('invoice_prefix','SEL').strip() or 'SEL'
            if not re.fullmatch(r'[A-Z0-9][A-Z0-9_-]{0,15}', prefix):
                raise BillingError('Préfixe de numérotation invalide.')
            try:
                width=max(1,min(12,int(policy.get('invoice_sequence_width','6'))))
            except ValueError:
                width=6
            number=f'{prefix}-{year}-{next_n:0{width}d}'
            s_client={k:client[k] for k in client.keys() if k!='created_at'}
            s_company={k:v for k,v in conf.items() if k not in ('configured',)}
            invoice_currency=calc.get('currency', conf['currency'])
            cur=g.db.execute('''INSERT INTO invoices(number,client_id,billing_period,issue_date,due_date,client_snapshot,company_snapshot,calc_snapshot,currency,note)
                VALUES(?,?,?,?,?,?,?,?,?,?)''',(number,cid,period,issue_date,draft['due_date'],json.dumps(s_client,ensure_ascii=False),json.dumps(s_company,ensure_ascii=False),json.dumps(calc,ensure_ascii=False),invoice_currency,draft['note']))
            inv_id=cur.lastrowid
            g.db.commit();flash('Décompte enregistré. Vérifiez le PDF avant impression.','success')
            session.pop('invoice_draft',None)
            return redirect(url_for('invoice_detail',iid=inv_id))
        except (BillingError,ValueError,sqlite3.IntegrityError) as e:
            g.db.rollback();flash(f'Émission interrompue : {e}','error')
            return redirect(url_for('new_invoice'))

    @app.get('/invoices/<int:iid>')
    @protected
    def invoice_detail(iid):
        record=db.invoice_full(g.db,iid)
        if not record: abort(404)
        inv,payments=record
        calc=json.loads(inv['calc_snapshot'])
        total=Decimal(calc['total']);paid=sum((Decimal(p['net_amount']) for p in payments),Decimal('0'))
        balance=money(total-paid)
        payment_status='ANNULÉ' if inv['status']=='VOID' else ('PAYÉ' if balance<=0 else ('PARTIELLEMENT PAYÉ' if paid>0 else 'IMPAYÉ'))
        return render_template('invoice_detail.html',title='Décompte '+inv['number'],invoice=inv,calc=calc,
                               client=json.loads(inv['client_snapshot']),payments=payments,paid=paid,balance=balance,payment_status=payment_status,
                               payment_methods=active_payment_methods(g.db),payment_form=session.get(f'payment_form_{iid}',{}),void_form=session.get(f'void_form_{iid}',{}))

    @app.post('/invoices/<int:iid>/payments')
    @protected
    def add_payment(iid):
        draft_key=f'payment_form_{iid}'
        submitted={key:request.form.get(key,'').strip() for key in ('amount','method','paid_on','reference')}
        try:
            amount=decimal(request.form.get('amount',''), 'Montant du paiement',allow_zero=False)
            if money(amount)!=amount: raise BillingError('Le paiement ne peut pas avoir plus de deux décimales.')
            method=request.form.get('method','').strip()
            payment_methods={item['label']: item for item in active_payment_methods(g.db)}
            if method not in payment_methods: raise BillingError('Mode de paiement invalide.')
            reference=request.form.get('reference','').strip()[:120]
            if payment_methods[method]['reference_required'] and not reference:
                raise BillingError('Une référence est obligatoire pour ce mode de paiement.')
            paid_on=request.form.get('paid_on',date.today().isoformat())
            date.fromisoformat(paid_on)
            g.db.execute('BEGIN IMMEDIATE')
            record=db.invoice_full(g.db,iid)
            if not record: raise BillingError('Facture inexistante.')
            inv,payments=record
            if inv['status']!='ISSUED': raise BillingError('Paiements interdits sur une facture annulée.')
            total=Decimal(json.loads(inv['calc_snapshot'])['total'])
            paid=sum((Decimal(p['net_amount']) for p in payments),Decimal('0'))
            if amount > total-paid: raise BillingError('Le paiement dépasse le solde dû.')
            g.db.execute('INSERT INTO payments(invoice_id,amount,method,reference,paid_on) VALUES(?,?,?,?,?)',(iid,str(money(amount)),method,reference,paid_on))
            g.db.commit();session.pop(draft_key,None);flash('Paiement enregistré.','success')
        except (BillingError,ValueError) as e:
            g.db.rollback();session[draft_key]=submitted;flash(str(e),'error')
        return redirect(url_for('invoice_detail',iid=iid))

    @app.post('/invoices/<int:iid>/payments/<int:payment_id>/reverse')
    @protected
    def reverse_payment(iid, payment_id):
        reason=request.form.get('reason','').strip()[:500]
        try:
            amount=decimal(request.form.get('amount',''),'Montant à corriger',allow_zero=False)
            if money(amount)!=amount: raise BillingError('La correction ne peut pas avoir plus de deux décimales.')
            if len(reason)<6: raise BillingError('Un motif précis (6 caractères minimum) est obligatoire.')
            reversed_on=request.form.get('reversed_on',date.today().isoformat()).strip()
            date.fromisoformat(reversed_on)
            g.db.execute('BEGIN IMMEDIATE')
            record=db.invoice_full(g.db,iid)
            if not record: raise BillingError('Facture introuvable.')
            invoice,payments=record
            if invoice['status']!='ISSUED': raise BillingError('Correction interdite sur une facture annulée.')
            payment=next((item for item in payments if item['id']==payment_id),None)
            if not payment: raise BillingError('Paiement introuvable pour cette facture.')
            outstanding=Decimal(payment['amount'])-Decimal(payment['reversed_amount'])
            if amount>outstanding: raise BillingError('La correction dépasse le montant encore imputé à ce paiement.')
            g.db.execute('INSERT INTO payment_reversals(payment_id,amount,reason,reversed_on) VALUES(?,?,?,?)',
                         (payment_id,str(money(amount)),reason,reversed_on))
            g.db.commit();flash('Correction de paiement enregistrée et conservée dans la traçabilité.','success')
        except (BillingError,ValueError,sqlite3.IntegrityError) as error:
            g.db.rollback();flash(str(error),'error')
        return redirect(url_for('invoice_detail',iid=iid))

    @app.post('/invoices/<int:iid>/void')
    @protected
    def void_invoice(iid):
        reason=request.form.get('reason','').strip()
        draft_key=f'void_form_{iid}'
        try:
            if len(reason)<6: raise BillingError('Un motif précis d’annulation (6 caractères minimum) est obligatoire.')
            g.db.execute('BEGIN IMMEDIATE')
            record=db.invoice_full(g.db,iid)
            if not record: raise BillingError('Facture introuvable.')
            invoice,payments=record
            if invoice['status']!='ISSUED': raise BillingError('Facture déjà annulée.')
            net_paid=sum((Decimal(payment['net_amount']) for payment in payments),Decimal('0'))
            if net_paid>0: raise BillingError('Annulation bloquée : corrigez intégralement les paiements avant d’annuler cette facture.')
            g.db.execute("UPDATE invoices SET status='VOID',void_reason=? WHERE id=?",(reason,iid))
            g.db.commit();session.pop(draft_key,None);flash('Facture annulée, conservée dans l’historique.','success')
        except BillingError as e:
            g.db.rollback();session[draft_key]={'reason':reason};flash(str(e),'error')
        return redirect(url_for('invoice_detail',iid=iid))

    @app.get('/invoices/<int:iid>/pdf')
    @protected
    def invoice_pdf(iid):
        record=db.invoice_full(g.db,iid)
        if not record: abort(404)
        inv,payments=record
        original_inv=dict(inv)
        original_calc=json.loads(inv['calc_snapshot'])
        if request.args.get('currency') == 'CDF':
            if inv['currency'] != 'USD':
                raise BillingError('La variante CDF est disponible pour les factures USD.')
            if request.args.get('rate'):
                return render_template('error.html',title='Taux de conversion refusé',status_code=400,
                                       message='Le taux d’impression ne peut pas être remplacé par un paramètre libre; utilisez le taux SEL enregistré.'),400
            rate_row=dict(db.exchange_rate_for(g.db,'USD','CDF',inv['issue_date']))
            inv,payments=converted_invoice_copy(inv,payments,rate_row,rate_row['rate'])
        pdf=render_invoice(inv, payments, qr_reference=(original_inv, original_calc))
        return send_file(io.BytesIO(pdf),mimetype='application/pdf',download_name=f'{inv["number"]}.pdf',as_attachment=request.args.get('download')=='1')

    @app.get('/export/clients.csv')
    @protected
    def export_clients():
        rows=g.db.execute('SELECT * FROM clients ORDER BY id').fetchall()
        return send_csv('sel-clients.csv',[dict(x) for x in rows])

    @app.get('/export/invoices.csv')
    @protected
    def export_invoices():
        rows=[]
        for inv in g.db.execute('SELECT * FROM invoices ORDER BY id'):
            client=json.loads(inv['client_snapshot']);calc=json.loads(inv['calc_snapshot'])
            payment_rows=g.db.execute('SELECT id,amount FROM payments WHERE invoice_id=?',(inv['id'],)).fetchall()
            paid_sum=Decimal('0')
            for payment in payment_rows:
                reversed_rows=g.db.execute('SELECT amount FROM payment_reversals WHERE payment_id=?',(payment['id'],)).fetchall()
                paid_sum+=Decimal(payment['amount'])-sum((Decimal(item['amount']) for item in reversed_rows),Decimal('0'))
            services={item.get('service_code'):item for item in calc.get('services',[]) if isinstance(item,dict)}
            water=services.get('WATER',{});electricity=services.get('ELECTRICITY',{})
            rows.append({'id':inv['id'],'number':inv['number'],'client_code':client['code'],
                         'client_name':client['name'],'period':inv['billing_period'],
                         'issued':inv['issue_date'],'status':inv['status'],'meter_number':client['meter_number'],
                         'previous':calc['previous'],'current':calc['current'],'coefficient':calc['coefficient'],
                         'kwh':calc['kwh'],'energy':calc['energy'],'local_tax':calc['local_tax'],
                         'vat':calc['vat'],'total':calc['total'],'currency':inv['currency'],
                         'electricity_total':electricity.get('total',calc['total']),'water_m3':water.get('quantity',''),
                         'water_subtotal':water.get('subtotal',''),'water_vat':water.get('vat',''),
                         'water_total':water.get('total',''),'paid':str(money(paid_sum)),
                         'balance':str(money(Decimal(calc['total'])-paid_sum)),
                         'services_json':json.dumps(calc.get('services',[]),ensure_ascii=False,separators=(',',':'))})
        return send_csv('sel-factures.csv',rows)

    @app.get('/export/payments.csv')
    @protected
    def export_payments():
        rows=[]
        for row in g.db.execute('SELECT * FROM payments ORDER BY id'):
            item=dict(row)
            reversals=g.db.execute('SELECT amount,reason,reversed_on FROM payment_reversals WHERE payment_id=? ORDER BY id',(row['id'],)).fetchall()
            reversed_amount=sum((Decimal(reversal['amount']) for reversal in reversals),Decimal('0'))
            item['reversed_amount']=str(reversed_amount)
            item['net_amount']=str(Decimal(item['amount'])-reversed_amount)
            item['corrections_json']=json.dumps([dict(reversal) for reversal in reversals],ensure_ascii=False,separators=(',',':'))
            rows.append(item)
        return send_csv('sel-paiements.csv',rows)

    @app.get('/export/payment-reversals.csv')
    @protected
    def export_payment_reversals():
        rows=[dict(row) for row in g.db.execute('''SELECT r.id,r.payment_id,p.invoice_id,i.number AS invoice_number,
            r.amount,r.reason,r.reversed_on,r.created_at FROM payment_reversals r
            JOIN payments p ON p.id=r.payment_id JOIN invoices i ON i.id=p.invoice_id ORDER BY r.id''')]
        return send_csv('sel-corrections-paiements.csv',rows)

    @app.get('/export/portable.json')
    @protected
    def export_portable():
        """Export portable v1 (sans hash de mot de passe ni clé de session)."""
        clients=[dict(row) for row in g.db.execute('SELECT * FROM clients ORDER BY id')]
        documents=[]
        for row in g.db.execute('SELECT * FROM invoices ORDER BY id'):
            raw=dict(row)
            raw['client_code']=json.loads(raw['client_snapshot'])['code']
            for key in ('client_snapshot','company_snapshot','calc_snapshot'):
                raw[key]=json.loads(raw[key])
            documents.append(raw)
        payments=[dict(row) for row in g.db.execute('''SELECT p.*, i.number AS invoice_number FROM payments p
                             JOIN invoices i ON i.id=p.invoice_id ORDER BY p.id''')]
        reversals=[dict(row) for row in g.db.execute('''SELECT r.*,p.invoice_id,i.number AS invoice_number FROM payment_reversals r
                             JOIN payments p ON p.id=r.payment_id JOIN invoices i ON i.id=p.invoice_id ORDER BY r.id''')]
        reversal_totals={}
        for reversal in reversals:
            reversal_totals[reversal['payment_id']]=reversal_totals.get(reversal['payment_id'],Decimal('0'))+Decimal(reversal['amount'])
        for payment in payments:
            reversed_amount=reversal_totals.get(payment['id'],Decimal('0'))
            payment['reversed_amount']=str(reversed_amount)
            payment['net_amount']=str(Decimal(payment['amount'])-reversed_amount)
        manifest={'format':'sel-facturation-portable','schema_version':1,
                  'exported_at':datetime.now().astimezone().isoformat(),
                  'source':'SEL Facturation Desktop',
                  'mapping':{'client_business_key':'clients[].code',
                             'invoice_business_key':'invoices[].number',
                             'payment_invoice_reference':'payments[].invoice_number',
                             'money':'decimal strings, same currency as document',
                             'snapshots':'immutable documents: do not recalculate'},
                  'data':{'company_settings':{k:v for k,v in db.settings(g.db).items() if k!='configured'},
                          'clients':clients,'invoices':documents,'payments':payments,'payment_reversals':reversals}}
        payload=json.dumps(manifest,ensure_ascii=False,indent=2).encode('utf-8')
        return send_file(io.BytesIO(payload),mimetype='application/json',as_attachment=True,
                         download_name=f'sel-export-{date.today().isoformat()}-v1.json')

    @app.get('/backup')
    @protected
    def download_backup():
        folder=db.home()/'sauvegardes'
        stamp=datetime.now().strftime('%Y%m%d-%H%M%S-%f')
        path=db.backup(folder/f'sel-export-{stamp}.sqlite3')
        return send_file(path,as_attachment=True,download_name=f'SEL-sauvegarde-{stamp}.sqlite3',mimetype='application/octet-stream')

    return app


def send_csv(filename, rows):
    buff=io.StringIO(newline='')
    if rows:
        cols=list(rows[0])
        writer=csv.DictWriter(buff,fieldnames=cols,delimiter=';')
        writer.writeheader()
        for row in rows:
            writer.writerow({k:('\''+str(v) if isinstance(v,str) and v[:1] in ('=','+','-','@','\t','\r') else v) for k,v in row.items()})
    payload=b'\xef\xbb\xbf'+buff.getvalue().encode('utf-8')
    return send_file(io.BytesIO(payload),mimetype='text/csv',as_attachment=True,download_name=filename)


if __name__=='__main__':
    print('Utilisez desktop.py pour démarrer SEL Facturation localement.')
