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
import threading
import time
from datetime import date, datetime
from decimal import Decimal
from functools import wraps
from pathlib import Path

from flask import (Flask, abort, flash, g, jsonify, redirect, render_template,
                   request, send_file, session, url_for)

import db
from domain import (BillingError, calculate_from_settings, convert_calculation,
                    decimal, fmt, money, parse_tiers, validate_currency,
                    validate_due_date, validate_exchange_rate,
                    validate_payment_reference)
from invoice_pdf import render_invoice
from updater import (DEFAULT_MANIFEST_URL, UpdateCheckResult, UpdateManifest,
                     check_for_update, create_pre_update_backup, download_update, staging_dir)
from version import APP_VERSION

PAGE_SIZE = 50


def page_number(value):
    try:
        return max(1, int(value))
    except (TypeError, ValueError):
        return 1


def sum_money(values):
    """Additionne des montants persistés sans passer par un flottant binaire."""
    return sum((Decimal(str(value)) for value in values), Decimal('0'))


def currency_context(connection, settings, issue_date):
    base = validate_currency(settings.get('base_currency', 'CDF'))
    currency = validate_currency(settings.get('currency', base))
    if currency == base:
        return {'base_currency': base, 'currency': currency, 'rate': Decimal('1'), 'rate_row': None}
    rate_row = db.exchange_rate_for(connection, base, currency, date.fromisoformat(issue_date))
    if not rate_row:
        raise BillingError('Aucun taux de change validé pour cette date et cette devise.')
    return {'base_currency': base, 'currency': currency, 'rate': validate_exchange_rate(rate_row['rate']), 'rate_row': rate_row}


def apply_currency_snapshot(calc, currency_info):
    row = currency_info.get('rate_row')
    if row:
        calc['exchange_rate_effective_on'] = row['effective_on']
        calc['exchange_rate_source'] = row['source']
    return calc


def resource(name):
    base = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parent))
    return str(base / name)


def update_result_payload(result):
    payload={'status':result.status, 'message':result.message}
    if result.manifest:
        payload['manifest']={
            'schema': result.manifest.schema,
            'version': result.manifest.version,
            'url': result.manifest.url,
            'sha256': result.manifest.sha256,
            'size': result.manifest.size,
            'published_at': result.manifest.published_at,
            'release_notes': result.manifest.release_notes,
            'source_url': result.manifest.source_url,
        }
    return payload


def start_background_update_check(application):
    def worker():
        result=check_for_update(application.config['UPDATE_MANIFEST_URL'], APP_VERSION)
        application.extensions['update_result']=update_result_payload(result)

    thread=threading.Thread(target=worker, name='sel-update-check', daemon=True)
    thread.start()
    return thread


def current_update_result(application):
    background = application.extensions.get('update_result') or {}
    if background.get('status') and background.get('status') != 'not_checked':
        return background
    return session.get('update_result') or background


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
    db.initialize()
    db.automatic_backup()
    app=Flask(__name__, template_folder=resource('templates'), static_folder=resource('static'))
    app.secret_key=read_secret()
    app.config.update(SESSION_COOKIE_HTTPONLY=True, SESSION_COOKIE_SAMESITE='Strict',
                      PERMANENT_SESSION_LIFETIME=3600*8, TRUSTED_HOSTS=['127.0.0.1','localhost'],
                      MAX_CONTENT_LENGTH=1024*1024,
                      UPDATE_MANIFEST_URL=os.environ.get('SEL_UPDATE_MANIFEST_URL', DEFAULT_MANIFEST_URL))
    app.extensions['update_result']={'status':'not_checked','message':''}
    app.start_background_update_check=lambda: start_background_update_check(app)

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

    @app.get('/updates')
    @protected
    def updates_view():
        return render_template('updates.html', title='Mises à jour',
                               update=current_update_result(app))

    @app.post('/updates/check')
    @protected
    def updates_check():
        result=check_for_update(app.config['UPDATE_MANIFEST_URL'], APP_VERSION)
        payload=update_result_payload(result)
        session['update_result']=payload
        app.extensions['update_result']=payload
        return redirect(url_for('updates_view'))

    @app.get('/updates/status')
    @protected
    def updates_status():
        result=current_update_result(app)
        return jsonify(status=result.get('status', 'not_checked'), message=result.get('message', ''))

    @app.post('/updates/download')
    @protected
    def updates_download():
        current=session.get('update_result') or {}
        raw_manifest=current.get('manifest')
        if current.get('status') != 'available' or not raw_manifest:
            session['update_result']={'status':'invalid','message':'Aucune mise à jour confirmée à télécharger.'}
            return redirect(url_for('updates_view'))
        try:
            manifest=UpdateManifest(**raw_manifest)
            package=download_update(manifest, staging_dir())
            backup=create_pre_update_backup()
            current['ready']={'filename':package.name, 'sha256':manifest.sha256, 'backup_name':backup.name}
            session['update_result']=current
            app.extensions['update_result']=current
        except Exception as exc:
            session['update_result']={'status':'invalid','message':str(exc)}
            app.extensions['update_result']=session['update_result']
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
        issued_rows=[row for row in invoice_rows if row['status']=='ISSUED' and row['currency']==currency]
        billed=sum_money(json.loads(row['calc_snapshot'])['total'] for row in issued_rows)
        clients=g.db.execute('SELECT COUNT(*) AS n FROM clients').fetchone()['n']
        recent=g.db.execute('''SELECT i.*, c.name as client_name FROM invoices i
          JOIN clients c ON c.id=i.client_id ORDER BY i.id DESC LIMIT 8''').fetchall()
        paid=sum_money(row['amount'] for row in g.db.execute("SELECT p.amount FROM payments p JOIN invoices i ON i.id=p.invoice_id WHERE i.status='ISSUED' AND i.currency=?", (currency,)))
        backups=sorted((db.home()/'sauvegardes').glob('sel-*.sqlite3'), key=lambda path: path.stat().st_mtime, reverse=True)
        stats={'n':len(issued_rows),'billed':billed,'paid':paid,'balance':money(billed-paid),
               'configured':conf.get('configured')=='1','last_backup':backups[0].name if backups else None}
        return render_template('dashboard.html',title='Tableau de bord',stats=stats,clients=clients,recent=recent)

    @app.route('/clients',methods=['GET','POST'])
    @protected
    def clients():
        if request.method=='POST':
            values={k:request.form.get(k,'').strip() for k in ('name','address','category','client_type','contract_number','meter_number','center','service_point','nif','phone','coefficient')}
            try:
                if not values['name'] or not values['address'] or not values['meter_number']:
                    raise BillingError('Nom, adresse et numéro du compteur sont obligatoires.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80: raise BillingError('Nom ou numéro de compteur trop long.')
                g.db.execute('BEGIN IMMEDIATE')
                next_id=g.db.execute('SELECT COALESCE(MAX(id),0)+1 FROM clients').fetchone()[0]
                code=f'SEL-C-{next_id:05d}'
                g.db.execute('''INSERT INTO clients(code,name,address,category,client_type,contract_number,meter_number,center,service_point,nif,phone,coefficient)
                     VALUES(?,?,?,?,?,?,?,?,?,?,?,?)''',(code,values['name'],values['address'],values['category'] or 'Résidentiel',values['client_type'] or 'Ordinaire',values['contract_number'],values['meter_number'],values['center'] or 'KOLWEZI',values['service_point'],values['nif'],values['phone'],values['coefficient'] or '1'))
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
        return render_template('clients.html',title='Abonnés et compteurs',clients=result,query=term,page=page,pages=pages,total=total)

    @app.route('/clients/<int:cid>/edit',methods=['GET','POST'])
    @protected
    def edit_client(cid):
        current=g.db.execute('SELECT * FROM clients WHERE id=?',(cid,)).fetchone()
        if not current: abort(404)
        if request.method=='POST':
            values={k:request.form.get(k,'').strip() for k in ('name','address','category','client_type','contract_number','meter_number','center','service_point','nif','phone','coefficient')}
            try:
                if not values['name'] or not values['address'] or not values['meter_number']: raise BillingError('Nom, adresse et compteur obligatoires.')
                decimal(values['coefficient'] or '1','Coefficient',allow_zero=False)
                if len(values['name'])>180 or len(values['meter_number'])>80:
                    raise BillingError('Nom ou numéro de compteur trop long.')
                g.db.execute('''UPDATE clients SET name=?,address=?,category=?,client_type=?,contract_number=?,meter_number=?,center=?,service_point=?,nif=?,phone=?,coefficient=? WHERE id=?''',(*[values[k] for k in values],cid))
                g.db.commit();flash('Fiche mise à jour. Les anciennes factures conservent leurs données.','success');return redirect(url_for('clients'))
            except (BillingError, ValueError) as e: flash(str(e),'error')
        return render_template('client_edit.html',title='Modifier le client',client=current)

    @app.route('/settings',methods=['GET','POST'])
    @protected
    def settings_view():
        conf=db.settings(g.db)
        currency_options=db.currency_catalog(g.db)
        if request.method=='POST':
            section=request.form.get('section','tariffs')
            identity_fields=('company_name','tagline','address','phone','email','website','rccm','nif','id_nat')
            tariff_fields=('tiers','local_rate','vat_rate','vat_base','currency')
            if section == 'identity':
                fields=identity_fields
            elif section == 'tariffs':
                fields=tariff_fields
            elif section == 'all':
                fields=identity_fields + tariff_fields
            else:
                fields=()
            values={k:request.form.get(k,'').strip() for k in fields}
            for key in fields:
                if key not in request.form and key in conf:
                    values[key]=conf[key]
            values['base_currency']=request.form.get('base_currency', conf.get('base_currency','CDF')).strip() or 'CDF'
            values['exchange_rate']=request.form.get('exchange_rate', conf.get('exchange_rate','')).strip()
            values['exchange_rate_effective_on']=request.form.get('exchange_rate_effective_on', conf.get('exchange_rate_effective_on','')).strip()
            values['exchange_rate_source']=request.form.get('exchange_rate_source', conf.get('exchange_rate_source','')).strip()
            try:
                if section == 'identity':
                    if not values['company_name'] or not values['address']: raise BillingError('Société et adresse obligatoires.')
                    with g.db:
                        for k in identity_fields: g.db.execute('UPDATE settings SET value=? WHERE key=?',(values[k],k))
                    flash('Identité SEL mise à jour. Les anciennes factures restent inchangées.','success')
                    return redirect(url_for('settings_view'))
                if section not in ('tariffs','all'):
                    raise BillingError('Section de paramètres invalide.')
                values['base_currency']=validate_currency(values['base_currency'])
                values['currency']=validate_currency(values['currency'])
                if values['base_currency'] != 'CDF': raise BillingError('La devise de référence SEL doit rester CDF.')
                if section == 'all' and (not values['company_name'] or not values['address']):
                    raise BillingError('Société et adresse obligatoires.')
                parse_tiers(values['tiers'])
                for k in ('local_rate','vat_rate'):
                    if decimal(values[k],k)>100: raise BillingError('Taux fiscal supérieur à 100 %.')
                if values['vat_base'] not in ('ENERGY','ENERGY_LOCAL','ENERGY_LOCAL_FEES'): raise BillingError('Assiette TVA invalide.')
                rate=Decimal('1')
                if values['currency'] != values['base_currency']:
                    rate=validate_exchange_rate(values['exchange_rate'])
                    if not values['exchange_rate_effective_on']:
                        raise BillingError('La date d’effet du taux de change est obligatoire.')
                    date.fromisoformat(values['exchange_rate_effective_on'])
                    if not values['exchange_rate_source']:
                        raise BillingError('La source du taux de change est obligatoire.')
                if request.form.get('confirmed')!='yes':
                    raise BillingError('Confirmez que les tarifs et taxes ont été validés auprès de SEL avant d’activer la facturation.')
                with g.db:
                    for k,v in values.items(): g.db.execute('INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value',(k,v))
                    g.db.execute("UPDATE settings SET value='1' WHERE key='configured'")
                    if values['currency'] != values['base_currency']:
                        db.save_exchange_rate(g.db, values['base_currency'], values['currency'], rate,
                                              values['exchange_rate_effective_on'], values['exchange_rate_source'], commit=False)
                flash('Paramètres validés. Les anciennes factures restent inchangées.','success')
                return redirect(url_for('settings_view'))
            except (BillingError, ValueError) as e: flash(f'Paramètres refusés : {e}','error')
            conf.update(values)
        return render_template('settings.html',title='Paramètres SEL',settings=conf,currency_options=currency_options)

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
                base_calc=calculate_from_settings(request.form.get('previous',''),request.form.get('current',''),client['coefficient'],conf,request.form.get('fee','0') or '0')
                currency_info=currency_context(g.db, conf, issue_date)
                calc=apply_currency_snapshot(convert_calculation(base_calc, currency_info['rate'], from_currency=currency_info['base_currency'], to_currency=currency_info['currency']), currency_info)
                if db.active_invoice(g.db,cid,period): raise BillingError('Une facture active existe déjà pour ce client et cette période.')
                session['invoice_draft']={'client_id':cid,'billing_period':period,'previous':request.form.get('previous','').strip(),'current':request.form.get('current','').strip(),'fee':request.form.get('fee','0').strip() or '0','due_date':due,'note':request.form.get('note','').strip()[:500]}
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
            base_calc=calculate_from_settings(draft['previous'],draft['current'],client['coefficient'],conf,draft['fee'])
            currency_info=currency_context(g.db, conf, issue_date)
            calc=apply_currency_snapshot(convert_calculation(base_calc, currency_info['rate'], from_currency=currency_info['base_currency'], to_currency=currency_info['currency']), currency_info)
            g.db.execute('BEGIN IMMEDIATE')
            if db.active_invoice(g.db,cid,period): raise BillingError('Cette période est déjà facturée pour ce client.')
            year=date.today().year
            row=g.db.execute('SELECT last_number FROM sequences WHERE year=?',(year,)).fetchone()
            next_n=(row['last_number']+1) if row else 1
            g.db.execute('INSERT INTO sequences(year,last_number) VALUES(?,?) ON CONFLICT(year) DO UPDATE SET last_number=excluded.last_number',(year,next_n))
            number=f'SEL-{year}-{next_n:06d}'
            s_client={k:client[k] for k in client.keys() if k!='created_at'}
            s_company={k:v for k,v in conf.items() if k not in ('configured',)}
            cur=g.db.execute('''INSERT INTO invoices(number,client_id,billing_period,issue_date,due_date,client_snapshot,company_snapshot,calc_snapshot,currency,note)
                VALUES(?,?,?,?,?,?,?,?,?,?)''',(number,cid,period,issue_date,draft['due_date'],json.dumps(s_client,ensure_ascii=False),json.dumps(s_company,ensure_ascii=False),json.dumps(calc,ensure_ascii=False),conf['currency'],draft['note']))
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
                               client=json.loads(inv['client_snapshot']),payments=payments,paid=paid,balance=balance,payment_status=payment_status)

    @app.post('/invoices/<int:iid>/payments')
    @protected
    def add_payment(iid):
        try:
            amount=decimal(request.form.get('amount',''), 'Montant du paiement',allow_zero=False)
            if money(amount)!=amount: raise BillingError('Le paiement ne peut pas avoir plus de deux décimales.')
            method=request.form.get('method','').strip()
            if method not in ('Espèces','Banque','Mobile Money','Autre'): raise BillingError('Mode de paiement invalide.')
            reference=request.form.get('reference','').strip()[:120]
            validate_payment_reference(method, reference)
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
                             'snapshots':'immutable documents: do not recalculate',
                             'currency_rates':'optional dated rates; invoice snapshots remain authoritative'},
                  'data':{'company_settings':{k:v for k,v in db.settings(g.db).items() if k!='configured'},
                          'currency_rates':[dict(row) for row in g.db.execute('SELECT base_currency,quote_currency,rate,effective_on,source,note FROM currency_rates ORDER BY effective_on,id')],
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
