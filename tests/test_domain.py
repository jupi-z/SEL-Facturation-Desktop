import sys
from decimal import Decimal
from pathlib import Path
import pytest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from domain import BillingError,calculate,convert_amount,convert_calculation,parse_tiers,validate_currency,validate_due_date,validate_exchange_rate,validate_payment_reference

TIERS=parse_tiers('1-100:300\n101-200:320\n201-:350')

def amount(prev='450',cur='650',**kw):
    return calculate(previous=prev,current=cur,coefficient=kw.pop('coefficient','1'),tiers=TIERS,
                     local_rate=kw.pop('local_rate','1'),vat_rate=kw.pop('vat_rate','16'),
                     vat_base=kw.pop('vat_base','ENERGY'),fixed_fee=kw.pop('fixed_fee','0'))

def test_example_explicit_taxes():
    d=amount()
    assert d['kwh']=='200'
    assert d['energy']=='62000.00'
    assert d['local_tax']=='620.00'
    assert d['vat']=='9920.00'
    assert d['total']=='72540.00'
    assert [(row['quantity'],row['amount']) for row in d['lines']]==[('100','30000.00'),('100','32000.00')]

def test_alternative_vat_base():
    assert amount(vat_base='ENERGY_LOCAL')['total']=='72639.20'
    assert amount(vat_base='ENERGY_LOCAL_FEES',fixed_fee='100')['total']=='72755.20'

def test_zero_consumption():
    d=amount(cur='450');assert d['kwh']=='0' and d['total']=='0.00'

def test_fractional_consumption_coefficient():
    d=amount(prev='100',cur='150',coefficient='2.5',local_rate='0',vat_rate='0')
    assert d['kwh']=='125.0'
    assert d['energy']=='38000.00'

def test_no_tier_for_large_consumption():
    with pytest.raises(BillingError):
        calculate(previous='0',current='201',coefficient='1',tiers=parse_tiers('1-100:300\n101-200:320'),local_rate='1',vat_rate='16',vat_base='ENERGY')

@pytest.mark.parametrize('source', ['0-100:300','1-100:300\n100-200:320','1-100:300\n103-200:320','1-:300\n2-200:320','1-100:-2','x'])
def test_invalid_tiers(source):
    with pytest.raises(BillingError):parse_tiers(source)

@pytest.mark.parametrize('prev,cur', [('500','499'),('NaN','2'),('-1','2'),('2','Infinity')])
def test_invalid_readings(prev,cur):
    with pytest.raises(BillingError):amount(prev,cur)


def test_due_date_cannot_precede_issue_date():
    with pytest.raises(BillingError, match='échéance'):
        validate_due_date('2026-10-14', '2026-10-13')


def test_payment_reference_required_for_traceable_methods():
    with pytest.raises(BillingError, match='référence'):
        validate_payment_reference('Banque', '')
    with pytest.raises(BillingError, match='référence'):
        validate_payment_reference('Mobile Money', '   ')
    validate_payment_reference('Espèces', '')


def test_currency_must_be_from_sel_supported_catalog():
    assert validate_currency('CDF') == 'CDF'
    assert validate_currency('USD') == 'USD'
    with pytest.raises(BillingError):
        validate_currency('ABC')


def test_exchange_rate_is_positive_and_converts_money_deterministically():
    rate = validate_exchange_rate('2850')
    assert rate == Decimal('2850')
    assert convert_amount(Decimal('2850'), rate, from_currency='CDF', to_currency='USD') == Decimal('1.00')
    with pytest.raises(BillingError):
        validate_exchange_rate('0')


def test_conversion_preserves_base_snapshot_and_converts_all_displayed_amounts():
    base = amount()
    converted = convert_calculation(base, Decimal('2850'), from_currency='CDF', to_currency='USD')

    assert converted['base_currency'] == 'CDF'
    assert converted['currency'] == 'USD'
    assert converted['exchange_rate'] == '2850'
    assert converted['energy'] == '21.75'
    assert converted['total'] == '25.45'
