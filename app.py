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

from flask import (Flask, abort, flash, g, redirect, render_template, request,
                   send_file, session, url_for)

import db
from domain import (BillingError, calculate_flat_rate, calculate_from_readings,
                    calculate_from_settings, convert_amount, decimal, fmt, money,
                    validate_due_date)
from invoice_pdf import render_invoice
from update_client import fetch_manifest
from version import APP_VERSION
from config_service import active_customer_types, active_payment_methods, billing_policy, current_tariffs, customer_type_label, record_change

PAGE_SIZE = 50


def page_number(value):
    try:
        return max(1, int(value))
    except (TypeError, ValueError):
        return 1


def sum_money(values):
    """Additionne des montants persistés sans passer par un flottant binaire."""
    return sum((Decimal(str(value)) for value in values), Decimal('0'))


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

    @app.get('/updates/status')
    @protected
    def update_status():
        """Check availability only; never downloads or executes an installer."""
        manifest_url = os.environ.get('SEL_UPDATE_MANIFEST_URL', '').strip()
        if not manifest_url:
            return {'state': 'not_configured', 'version': APP_VERSION}
        try:
            return fetch_manifest(manifest_url, current_version=APP_VERSION)
        except (OSError, ValueError, TimeoutError):
            return {'state': 'unavailable', 'version': APP_VERSION}

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
        clients=g.db.execute('SELECT COUNT(*) AS n FROM clients').fetchone()['n']
        recent=g.db.execute('''SELECT i.*, c.name as client_name FROM invoices i
          JOIN clients c ON c.id=i.client_id ORDER BY i.id DESC LIMIT 8''').fetchall()
        paid=sum_money(row['amount'] for row in g.db.execute("SELECT p.amount FROM payments p JOIN invoices i ON i.id=p.invoice_id WHERE i.status='ISSUED' AND i.currency=?", (currency,)))
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
        if request.method=='POST':
            values={k:request.form.get(k,'').strip() for k in ('name','address','category','client_type','customer_type','billing_frequency','contract_number','meter_number','center','service_point','nif','phone','coefficient')}
            try:
                if not values['name'] or not values['address']:
                    raise BillingError('Nom et adresse sont obligatoires.')
                if values['customer_type'] and values['customer_type'] not in customer_type_codes:
                    raise BillingError('Type d’abonné invalide.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80: raise BillingError('Nom ou numéro de compteur trop long.')
                g.db.execute('BEGIN IMMEDIATE')
                next_id=g.db.execute('SELECT COALESCE(MAX(id),0)+1 FROM clients').fetchone()[0]
                code=f'SEL-C-{next_id:05d}'
                g.db.execute('''INSERT INTO clients(code,name,address,category,client_type,customer_type,billing_frequency,contract_number,meter_number,center,service_point,nif,phone,coefficient)
                     VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)''',(code,values['name'],values['address'],values['category'] or '',values['client_type'] or '',values['customer_type'],values['billing_frequency'] or 'MONTHLY',values['contract_number'],values['meter_number'],values['center'] or 'KOLWEZI',values['service_point'],values['nif'],values['phone'],values['coefficient'] or '1'))
                g.db.commit();flash('Client enregistré.','success');return redirect(url_for('clients'))
            except (BillingError,sqlite3.IntegrityError) as e:
                g.db.rollback();flash(str(e),'error')
        term=request.args.get('q','').strip()[:120]
        page=page_number(request.args.get('page','1'))
        offset=(page-1)*PAGE_SIZE
        search_args=[]
        if term:
            where='WHERE name LIKE ? OR code LIKE ? OR meter_number LIKE ? OR contract_number LIKE ?'
            search_args=['%'+term+'%']*4
        else:
            where=''
        total=g.db.execute(f'SELECT COUNT(*) FROM clients {where}',search_args).fetchone()[0]
        result=g.db.execute(f'SELECT * FROM clients {where} ORDER BY id DESC LIMIT ? OFFSET ?',search_args+[PAGE_SIZE,offset]).fetchall()
        pages=max(1,(total+PAGE_SIZE-1)//PAGE_SIZE)
        return render_template('clients.html',title='Abonnés',clients=result,query=term,page=page,pages=pages,total=total,customer_types=customer_types)

    @app.route('/clients/<int:cid>/edit',methods=['GET','POST'])
    @protected
    def edit_client(cid):
        current=g.db.execute('SELECT * FROM clients WHERE id=?',(cid,)).fetchone()
        if not current: abort(404)
        customer_type_codes = {item['code'] for item in active_customer_types(g.db)}
        if request.method=='POST':
            values={k:request.form.get(k,'').strip() for k in ('name','address','category','client_type','customer_type','billing_frequency','contract_number','meter_number','center','service_point','nif','phone','coefficient')}
            try:
                if not values['name'] or not values['address']: raise BillingError('Nom et adresse obligatoires.')
                if values['customer_type'] and values['customer_type'] not in customer_type_codes: raise BillingError('Type d’abonné invalide.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80:
                    raise BillingError('Nom ou numéro de compteur trop long.')
                g.db.execute('''UPDATE clients SET name=?,address=?,category=?,client_type=?,customer_type=?,billing_frequency=?,contract_number=?,meter_number=?,center=?,service_point=?,nif=?,phone=?,coefficient=? WHERE id=?''',(*[values[k] for k in values],cid))
                g.db.commit();flash('Fiche mise à jour. Les anciennes factures conservent leurs données.','success');return redirect(url_for('clients'))
            except BillingError as e: flash(str(e),'error')
        return render_template('client_edit.html',title='Modifier le client',client=current)

    @app.route('/settings',methods=['GET','POST'])
    @protected
    def settings_view():
        conf=db.settings(g.db)
        customer_types=active_customer_types(g.db)
        tariffs=current_tariffs(g.db)
        try:
            current_exchange_rate=db.exchange_rate_for(g.db,'USD','CDF',date.today().isoformat())['rate']
        except ValueError:
            current_exchange_rate=conf.get('exchange_rate_usd_cdf','2360')
        if request.method=='POST':
            section=request.form.get('section','tariffs')
            fields=['company_name','tagline','address','phone','email','website','rccm','nif','id_nat','vat_rate','residential_rate','commercial_rate','exchange_rate_usd_cdf']
            values={k:request.form.get(k,'').strip() for k in fields}
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
                    return redirect(url_for('settings_view'))
                if section == 'customer_type':
                    code=request.form.get('code','').strip().upper()
                    label=request.form.get('label','').strip()
                    if not re.fullmatch(r'[A-Z][A-Z0-9_]{1,31}', code):
                        raise BillingError('Le code doit contenir 2 à 32 caractères A-Z, chiffres ou underscore.')
                    if not label or len(label)>80:
                        raise BillingError('Le libellé du type d’abonné est obligatoire et limité à 80 caractères.')
                    active=request.form.get('active')=='yes'
                    initial_rate=decimal(request.form.get('initial_rate',''),'Tarif initial',allow_zero=False) if active else None
                    effective_from=date.today().replace(day=1).isoformat()
                    with g.db:
                        g.db.execute("INSERT INTO customer_types(code,label,active,sort_order) VALUES(?,?,?,?)", (code,label,1 if active else 0,100))
                        record_change(g.db,'customer_types',code,'',label,'Création depuis Paramètres')
                        if active:
                            g.db.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active) VALUES(?,?,?,?,?,?,?,1)", (f'{code}-{effective_from[:7]}',code,'KWH',str(initial_rate),'USD','16',effective_from))
                            record_change(g.db,'tariffs',code,'',str(initial_rate),'Tarif initial depuis Paramètres')
                    flash('Type d’abonné ajouté.','success')
                    return redirect(url_for('settings_view'))
                if section == 'identity' and (not values['company_name'] or not values['address']):
                    raise BillingError('Société et adresse obligatoires.')
                if section == 'identity':
                    identity_fields=('company_name','tagline','address','phone','email','website','rccm','nif','id_nat')
                    with g.db:
                        for k in identity_fields: g.db.execute('UPDATE settings SET value=? WHERE key=?',(values[k],k))
                    flash('Identité SEL mise à jour. Les anciennes factures restent inchangées.','success')
                    return redirect(url_for('settings_view'))
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
                            g.db.execute("UPDATE tariff_plans SET unit_price=?,currency='USD',vat_rate='16',active=1 WHERE code=?", (str(submitted_rates[code]),tariff_code))
                        else:
                            record_change(g.db, 'tariffs', code, '', str(submitted_rates[code]), 'Création depuis Paramètres')
                            g.db.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active) VALUES(?,?,?,?,?,?,?,1)", (tariff_code,code,'KWH',str(submitted_rates[code]),'USD','16',effective_from))
                    g.db.execute("UPDATE settings SET value='1' WHERE key='configured'")
                flash('Paramètres validés. Les anciennes factures restent inchangées.','success')
                return redirect(url_for('settings_view'))
            except (BillingError, sqlite3.IntegrityError) as e: flash(str(e),'error')
            conf.update(values)
        return render_template('settings.html',title='Paramètres SEL',settings=conf,customer_types=customer_types,tariffs=tariffs,policy=billing_policy(g.db),exchange_rate=current_exchange_rate)

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
            for payment in g.db.execute(
                f'SELECT invoice_id,amount FROM payments WHERE invoice_id IN ({marks})', ids):
                paid_by_invoice[payment['invoice_id']].append(payment['amount'])
            for item in rows:
                item['paid']=sum_money(paid_by_invoice[item['id']])
        pages=max(1,(total+PAGE_SIZE-1)//PAGE_SIZE)
        return render_template('invoices.html',title='Historique des décomptes',invoices=rows,query=term,status=status,page=page,pages=pages,total=total)

    @app.route('/invoices/new',methods=['GET','POST'])
    @protected
    def new_invoice():
        conf=db.settings(g.db)
        options=g.db.execute('SELECT * FROM clients ORDER BY name').fetchall()
        if request.method=='POST':
            session.pop('invoice_draft',None)
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
                direct_kwh=request.form.get('consumption_kwh','').strip()
                if direct_kwh:
                    customer_type=client['customer_type']
                    if not customer_type:
                        raise BillingError('Attribuez un type d’abonné avant de facturer.')
                    plan=db.tariff_plan_for(g.db,customer_type,period+'-01')
                    calc=calculate_flat_rate(consumption_kwh=direct_kwh,unit_price=plan['unit_price'],vat_rate=plan['vat_rate'],currency='USD')
                    calc.update({'customer_type':customer_type,'customer_type_label':customer_type_label(g.db, customer_type),'tariff_code':plan['code'],'tariff_effective_from':plan['effective_from'],'unit_price':plan['unit_price']})
                else:
                    calc=calculate_from_settings(request.form.get('previous',''),request.form.get('current',''),client['coefficient'],conf,request.form.get('fee','0') or '0')
                if db.active_invoice(g.db,cid,period): raise BillingError('Une facture active existe déjà pour ce client et cette période.')
                session['invoice_draft']={'client_id':cid,'billing_period':period,'previous':request.form.get('previous','').strip(),'current':request.form.get('current','').strip(),'consumption_kwh':direct_kwh,'fee':request.form.get('fee','').strip() or '0','due_date':due,'note':request.form.get('note','').strip()[:500], 'tariff_code':calc.get('tariff_code',''), 'tariff_effective_from':calc.get('tariff_effective_from',''), 'unit_price':calc.get('unit_price',''), 'vat_rate':calc.get('vat_rate',''), 'currency':calc.get('currency','USD')}
                return render_template('invoice_preview.html',title='Vérifier le décompte',draft=session['invoice_draft'],client=client,calc=calc,configured=True)
            except (BillingError,ValueError,sqlite3.IntegrityError) as e:
                g.db.rollback();flash(f'Émission interrompue : {e}','error')
        period=date.today().strftime('%Y-%m')
        return render_template('new_invoice.html',title='Nouveau décompte',clients=options,period=period,configured=conf.get('configured')=='1')

    @app.post('/invoices/new/confirm')
    @protected
    def confirm_invoice():
        draft=session.pop('invoice_draft',None)
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
            if draft.get('consumption_kwh'):
                customer_type=client['customer_type']
                if not customer_type:
                    raise BillingError('Type d’abonné manquant ou invalide.')
                plan=db.tariff_plan_for(g.db,customer_type,period+'-01')
                unit_price=draft.get('unit_price') or plan['unit_price']
                vat_rate=draft.get('vat_rate') or plan['vat_rate']
                invoice_currency=draft.get('currency') or plan['currency']
                calc=calculate_flat_rate(consumption_kwh=draft['consumption_kwh'],unit_price=unit_price,vat_rate=vat_rate,currency=invoice_currency)
                calc.update({'customer_type':customer_type,'customer_type_label':customer_type_label(g.db, customer_type),'tariff_code':draft.get('tariff_code') or plan['code'],'tariff_effective_from':draft.get('tariff_effective_from') or plan['effective_from'],'unit_price':unit_price})
            else:
                calc=calculate_from_settings(draft['previous'],draft['current'],client['coefficient'],conf,draft['fee'])
            g.db.execute('BEGIN IMMEDIATE')
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
        total=Decimal(calc['total']);paid=sum((Decimal(p['amount']) for p in payments),Decimal('0'))
        balance=money(total-paid)
        payment_status='ANNULÉ' if inv['status']=='VOID' else ('PAYÉ' if balance<=0 else ('PARTIELLEMENT PAYÉ' if paid>0 else 'IMPAYÉ'))
        return render_template('invoice_detail.html',title='Décompte '+inv['number'],invoice=inv,calc=calc,
                               client=json.loads(inv['client_snapshot']),payments=payments,paid=paid,balance=balance,payment_status=payment_status,
                               payment_methods=active_payment_methods(g.db))

    @app.post('/invoices/<int:iid>/payments')
    @protected
    def add_payment(iid):
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
            paid=sum((Decimal(p['amount']) for p in payments),Decimal('0'))
            if amount > total-paid: raise BillingError('Le paiement dépasse le solde dû.')
            g.db.execute('INSERT INTO payments(invoice_id,amount,method,reference,paid_on) VALUES(?,?,?,?,?)',(iid,str(money(amount)),method,reference,paid_on))
            g.db.commit();flash('Paiement enregistré.','success')
        except (BillingError,ValueError) as e:
            g.db.rollback();flash(str(e),'error')
        return redirect(url_for('invoice_detail',iid=iid))

    @app.post('/invoices/<int:iid>/void')
    @protected
    def void_invoice(iid):
        reason=request.form.get('reason','').strip()
        try:
            if len(reason)<6: raise BillingError('Un motif précis d’annulation (6 caractères minimum) est obligatoire.')
            g.db.execute('BEGIN IMMEDIATE')
            record=db.invoice_full(g.db,iid)
            if not record: raise BillingError('Facture introuvable.')
            invoice,payments=record
            if invoice['status']!='ISSUED': raise BillingError('Facture déjà annulée.')
            if payments: raise BillingError('Annulation bloquée : des paiements existent. Effectuer une correction supervisée.')
            g.db.execute("UPDATE invoices SET status='VOID',void_reason=? WHERE id=?",(reason,iid))
            g.db.commit();flash('Facture annulée, conservée dans l’historique.','success')
        except BillingError as e:
            g.db.rollback();flash(str(e),'error')
        return redirect(url_for('invoice_detail',iid=iid))

    @app.get('/invoices/<int:iid>/pdf')
    @protected
    def invoice_pdf(iid):
        record=db.invoice_full(g.db,iid)
        if not record: abort(404)
        inv,payments=record
        if request.args.get('currency') == 'CDF':
            if inv['currency'] != 'USD':
                raise BillingError('La variante CDF est disponible pour les factures USD.')
            rate_value=request.args.get('rate') or db.exchange_rate_for(g.db,'USD','CDF',date.today().isoformat())['rate']
            rate=decimal(rate_value,'Taux USD/CDF',allow_zero=False)
            inv=dict(inv); inv['currency']='CDF'
            calc=json.loads(inv['calc_snapshot'])
            for key in ('energy','local_tax','vat_basis','vat','fee','total'):
                calc[key]=str(money(Decimal(calc[key])*rate))
            calc['currency']='CDF'; calc['conversion_source_currency']='USD'; calc['conversion_rate']=str(rate)
            calc['lines']=[dict(row,amount=str(money(Decimal(row['amount'])*rate))) for row in calc.get('lines',[])]
            inv['calc_snapshot']=json.dumps(calc,ensure_ascii=False)
            payments=[dict(item,amount=str(money(Decimal(item['amount'])*rate))) for item in payments]
        pdf=render_invoice(inv, payments)
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
            paid=g.db.execute('SELECT amount FROM payments WHERE invoice_id=?',(inv['id'],)).fetchall()
            paid_sum=sum((Decimal(r['amount']) for r in paid),Decimal('0'))
            rows.append({'id':inv['id'],'number':inv['number'],'client_code':client['code'],
                         'client_name':client['name'],'period':inv['billing_period'],
                         'issued':inv['issue_date'],'status':inv['status'],'meter_number':client['meter_number'],
                         'previous':calc['previous'],'current':calc['current'],'coefficient':calc['coefficient'],
                         'kwh':calc['kwh'],'energy':calc['energy'],'local_tax':calc['local_tax'],
                         'vat':calc['vat'],'total':calc['total'],'currency':inv['currency'],
                         'paid':str(money(paid_sum)),'balance':str(money(Decimal(calc['total'])-paid_sum))})
        return send_csv('sel-factures.csv',rows)

    @app.get('/export/payments.csv')
    @protected
    def export_payments():
        rows=[dict(row) for row in g.db.execute('SELECT * FROM payments ORDER BY id')]
        return send_csv('sel-paiements.csv',rows)

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
        manifest={'format':'sel-facturation-portable','schema_version':1,
                  'exported_at':datetime.now().astimezone().isoformat(),
                  'source':'SEL Facturation Desktop',
                  'mapping':{'client_business_key':'clients[].code',
                             'invoice_business_key':'invoices[].number',
                             'payment_invoice_reference':'payments[].invoice_number',
                             'money':'decimal strings, same currency as document',
                             'snapshots':'immutable documents: do not recalculate'},
                  'data':{'company_settings':{k:v for k,v in db.settings(g.db).items() if k!='configured'},
                          'clients':clients,'invoices':documents,'payments':payments}}
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
