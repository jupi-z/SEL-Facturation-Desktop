import importlib.util
import json
import re
import sys
from pathlib import Path
import pytest

if not importlib.util.find_spec('flask'):
    pytest.skip('Flask is not installed in this offline test environment',allow_module_level=True)

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from app import create_app, page_number
from version import APP_VERSION
from db import connect

@pytest.fixture
def client(tmp_path,monkeypatch):
    monkeypatch.setenv('SEL_DATA_DIR',str(tmp_path))
    app=create_app()
    app.config['TESTING']=True
    return app.test_client()

def csrf(client,path):
    r=client.get(path)
    assert r.status_code==200, r.status_code
    match=re.search(r'name="_csrf" value="([^"]+)"',r.get_data(as_text=True))
    assert match, f'No CSRF token in {path}'
    return match.group(1)

def test_complete_flow(client):
    r=client.get('/')
    assert r.status_code==302
    token=csrf(client,'/setup')
    r=client.post('/setup',data={'_csrf':token,'password':'TestPassword2026!','confirm':'TestPassword2026!'})
    assert r.status_code==302
    token=csrf(client,'/login')
    assert client.post('/login',data={'_csrf':token,'password':'TestPassword2026!'}).status_code==302
    token=csrf(client,'/settings')
    s={'_csrf':token,'company_name':'SEL','tagline':'Test','address':'Kolwezi','phone':'','email':'','website':'','rccm':'','nif':'','id_nat':'',
       'tiers':'1-100:300\n101-200:320','local_rate':'1','vat_rate':'16','vat_base':'ENERGY','currency':'CDF','confirmed':'yes'}
    assert client.post('/settings',data=s).status_code==302
    token=csrf(client,'/clients')
    assert client.post('/clients',data={'_csrf':token,'name':'Abonné A','address':'Kolwezi','meter_number':'M123','coefficient':'1','contract_number':'C1'}).status_code==302
    token=csrf(client,'/invoices/new')
    data={'_csrf':token,'client_id':'1','billing_period':'2026-08','previous':'450','current':'650','fee':'0','due_date':'2026-10-31'}
    preview=client.post('/invoices/new',data=data)
    assert preview.status_code==200
    preview_token=re.search(r'name="_csrf" value="([^"]+)"',preview.get_data(as_text=True)).group(1)
    assert client.post('/invoices/new/confirm',data={'_csrf':preview_token}).status_code==302
    with connect() as c:
        inv=c.execute('SELECT * FROM invoices').fetchone()
        assert inv['number'].startswith('SEL-')
        iid=inv['id']
    dashboard=client.get('/')
    assert '72 540,00' in dashboard.get_data(as_text=True)
    pdf=client.get(f'/invoices/{iid}/pdf')
    assert pdf.status_code==200 and pdf.data.startswith(b'%PDF-')
    r=client.post('/invoices/new',data=data,follow_redirects=True)
    assert 'déjà facturée' in r.get_data(as_text=True) or 'déjà' in r.get_data(as_text=True)
    assert len(list(connect().execute('SELECT id FROM invoices')))==1
    token=csrf(client,f'/invoices/{iid}')
    r=client.post(f'/invoices/{iid}/payments',data={'_csrf':token,'amount':'20000','method':'Espèces','paid_on':'2026-10-14'})
    assert r.status_code==302
    portable=client.get('/export/portable.json')
    assert portable.status_code==200
    d=json.loads(portable.data)
    assert d['schema_version']==1 and len(d['data']['invoices'])==1 and len(d['data']['payments'])==1
    assert 'password' not in portable.get_data(as_text=True).lower()
    assert client.get('/backup').status_code==200


def test_health_reports_current_application_version(client):
    response=client.get('/health')
    assert response.status_code==200
    assert response.json['version']==APP_VERSION


def test_page_number_is_safe_for_list_views():
    assert page_number('2') == 2
    assert page_number('0') == 1
    assert page_number('not-a-number') == 1
