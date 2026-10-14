"""Moteur déterministe de facturation SEL (aucune dépendance Flask)."""
from __future__ import annotations
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from typing import Any
import json
import re
from datetime import date

Q = Decimal("0.01")


class BillingError(ValueError):
    pass


def validate_due_date(issue_date: str, due_date: str) -> None:
    if not due_date:
        return
    try:
        issued = date.fromisoformat(issue_date)
        due = date.fromisoformat(due_date)
    except ValueError:
        raise BillingError("Date d’échéance invalide.") from None
    if due < issued:
        raise BillingError("La date d’échéance ne peut pas précéder la date d’émission.")


def validate_payment_reference(method: str, reference: str) -> None:
    if method in ("Banque", "Mobile Money") and not reference.strip():
        raise BillingError("Une référence est obligatoire pour ce mode de paiement.")


def decimal(value: Any, field: str, *, allow_zero=True) -> Decimal:
    if value is None or str(value).strip() == "":
        raise BillingError(f"{field} est obligatoire.")
    try:
        d = Decimal(str(value).strip().replace(",", "."))
    except InvalidOperation:
        raise BillingError(f"{field} doit être un nombre valide.") from None
    if not d.is_finite() or d < 0 or (not allow_zero and d == 0):
        raise BillingError(f"{field} doit être un nombre positif{' ou nul' if allow_zero else ''}.")
    if abs(d.as_tuple().exponent) > 6:
        raise BillingError(f"{field} ne doit pas dépasser six décimales.")
    return d


def money(value: Decimal) -> Decimal:
    return value.quantize(Q, rounding=ROUND_HALF_UP)


def fmt(value: Any) -> str:
    d = Decimal(str(value))
    return f"{d:,.2f}".replace(",", " ").replace(".", ",")


def parse_tiers(source: str):
    """Chaque ligne '1-100:300' / '101-200:320' / '201-:...' .

    Les tranches doivent former un continuum à partir de 1 kWh.
    Aucune extrapolation au-delà de la dernière tranche bornée.
    """
    items = []
    for raw in source.strip().splitlines():
        line = raw.strip()
        if not line:
            continue
        m = re.fullmatch(r"(\d+)\s*-\s*(\d*)\s*:\s*(\d+(?:[.,]\d{1,4})?)", line)
        if not m:
            raise BillingError(f"Tranche invalide : {line}. Format attendu : 1-100:300")
        lo = int(m.group(1))
        hi = int(m.group(2)) if m.group(2) else None
        price = decimal(m.group(3), "Tarif")
        if hi is not None and hi < lo:
            raise BillingError(f"Borne de tranche invalide : {line}")
        items.append({"min": lo, "max": hi, "unit_price": str(price)})
    if not items or items[0]["min"] != 1:
        raise BillingError("Les tranches doivent commencer à 1 kWh.")
    for idx, tier in enumerate(items):
        if tier["max"] is None and idx != len(items)-1:
            raise BillingError("Une tranche illimitée ne peut être que la dernière.")
        if idx and (items[idx-1]["max"] is None or tier["min"] != items[idx-1]["max"] + 1):
            raise BillingError("Les tranches doivent être consécutives sans intervalle ni chevauchement.")
    return items


def calculate(*, previous: str, current: str, coefficient: str, tiers: list,
              local_rate: str, vat_rate: str, vat_base: str, fixed_fee: str = "0") -> dict:
    prev = decimal(previous, "Ancien index")
    curr = decimal(current, "Nouvel index")
    coef = decimal(coefficient, "Coefficient", allow_zero=False)
    fee = decimal(fixed_fee, "Frais supplémentaires")
    if curr < prev:
        raise BillingError("Nouvel index inférieur à l'ancien : corriger ou traiter le changement de compteur séparément.")
    kwh = (curr - prev) * coef
    local_percent = decimal(local_rate, "Taux de taxe locale")
    vat_percent = decimal(vat_rate, "Taux de TVA")
    if local_percent > 100 or vat_percent > 100:
        raise BillingError("Un taux supérieur à 100 % doit être contrôlé.")
    if vat_base not in ("ENERGY", "ENERGY_LOCAL", "ENERGY_LOCAL_FEES"):
        raise BillingError("Assiette de TVA non configurée.")
    consumed = Decimal("0")
    rows = []
    for tr in tiers:
        lo = Decimal(tr["min"])
        hi = Decimal(tr["max"]) if tr["max"] is not None else None
        max_upto = hi if hi is not None else kwh
        used = max(Decimal("0"), min(kwh, max_upto) - (lo-1))
        if used <= 0:
            continue
        unit = decimal(tr["unit_price"], "Prix unitaire")
        amount = money(used * unit)
        rows.append({"range": f"{tr['min']} - {tr['max'] or '∞'}", "quantity": str(used),
                     "unit_price": str(unit), "amount": str(amount)})
        consumed += used
    if consumed != kwh:
        if kwh == 0: pass
        else: raise BillingError("La consommation dépasse le barème configuré. Ajoutez la tranche manquante avant de facturer.")
    energy = sum((Decimal(row["amount"]) for row in rows), Decimal("0"))
    local = money(energy * local_percent / Decimal("100"))
    if vat_base == "ENERGY":
        vat_basis = energy
    elif vat_base == "ENERGY_LOCAL":
        vat_basis = energy + local
    else:
        vat_basis = energy + local + fee
    vat = money(vat_basis * vat_percent / Decimal("100"))
    total = money(energy + local + fee + vat)
    return {"previous": str(prev), "current": str(curr), "coefficient": str(coef),
            "kwh": str(kwh), "lines": rows, "energy": str(money(energy)),
            "local_rate": str(local_percent), "local_tax": str(local),
            "vat_rate": str(vat_percent), "vat_base": vat_base, "vat_basis": str(vat_basis),
            "vat": str(vat), "fee": str(money(fee)), "total": str(total)}


def calculate_from_settings(previous: str, current: str, coefficient: str, settings: dict, fee: str = "0") -> dict:
    return calculate(previous=previous, current=current, coefficient=coefficient,
                     tiers=parse_tiers(settings['tiers']), local_rate=settings['local_rate'],
                     vat_rate=settings['vat_rate'], vat_base=settings['vat_base'], fixed_fee=fee)
