from decimal import Decimal

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from app import sum_money


def test_sum_money_keeps_decimal_precision():
    assert sum_money(['0.10', '0.20', '1000000000000.01']) == Decimal('1000000000000.31')
