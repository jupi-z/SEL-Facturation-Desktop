"""Moteur déterministe de facturation SEL (aucune dépendance Flask)."""
from __future__ import annotations
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from typing import Any
from copy import deepcopy
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
        date.fromisoformat(issue_date)
        date.fromisoformat(due_date)
    except ValueError:
        raise BillingError("Date d’échéance invalide.") from None


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
        raise BillingError("La configuration de la TVA est incomplète.")
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


def calculate_flat_rate(*, consumption_kwh: str, unit_price: str,
                        vat_rate: str = "16", currency: str = "USD") -> dict:
    """Calculate the current SEL model: one fixed price per consumed kWh."""
    kwh = decimal(consumption_kwh, "Consommation", allow_zero=True)
    price = decimal(unit_price, "Prix unitaire", allow_zero=False)
    vat_percent = decimal(vat_rate, "Taux de TVA")
    if vat_percent > 100:
        raise BillingError("Le taux de TVA ne peut pas dépasser 100 %.")
    energy = money(kwh * price)
    vat = money(energy * vat_percent / Decimal("100"))
    total = money(energy + vat)
    return {
        "pricing_model": "FLAT_KWH",
        "previous": "",
        "current": "",
        "coefficient": "1",
        "kwh": str(kwh),
        "lines": [{"range": "Consommation", "quantity": str(kwh),
                   "unit_price": str(price), "amount": str(energy)}] if kwh else [],
        "energy": str(energy),
        "local_rate": "0",
        "local_tax": "0.00",
        "vat_rate": str(vat_percent),
        "vat_base": "ENERGY",
        "vat_basis": str(energy),
        "vat": str(vat),
        "fee": "0.00",
        "total": str(total),
        "currency": currency,
    }


def calculate_from_readings(*, previous: str, current: str, coefficient: str = "1",
                            unit_price: str, vat_rate: str = "16",
                            currency: str = "USD") -> dict:
    """Future meter mode, sharing the fixed-rate VAT rules."""
    prev = decimal(previous, "Ancien index")
    curr = decimal(current, "Nouvel index")
    coef = decimal(coefficient, "Coefficient", allow_zero=False)
    if curr < prev:
        raise BillingError("Nouvel index inférieur à l'ancien.")
    result = calculate_flat_rate(consumption_kwh=str((curr - prev) * coef),
                                 unit_price=unit_price, vat_rate=vat_rate,
                                 currency=currency)
    result["previous"] = str(prev)
    result["current"] = str(curr)
    result["coefficient"] = str(coef)
    return result


def _combined_service(service: dict, expected_code: str) -> dict:
    if not isinstance(service, dict) or service.get("code") != expected_code:
        raise BillingError(f"Le service {expected_code} est obligatoire.")
    mode = str(service.get("mode", "")).strip()
    unit = str(service.get("unit", "")).strip()
    if not mode or not unit:
        raise BillingError(f"Le mode et l’unité du service {expected_code} sont obligatoires.")
    quantity = decimal(service.get("quantity"), f"Quantité {expected_code}")
    unit_price = decimal(service.get("unit_price_excl_tax"), f"Tarif {expected_code}", allow_zero=False)
    raw_lines = service.get("lines")
    if not isinstance(raw_lines, list) or not raw_lines:
        raise BillingError(f"Le détail tarifaire du service {expected_code} est obligatoire.")

    lines = []
    subtotal = Decimal("0")
    for raw_line in raw_lines:
        if not isinstance(raw_line, dict):
            raise BillingError(f"Ligne tarifaire invalide pour le service {expected_code}.")
        line_quantity = decimal(raw_line.get("quantity"), f"Quantité {expected_code}")
        line_price = decimal(raw_line.get("unit_price"), f"Tarif {expected_code}", allow_zero=False)
        amount = money(line_quantity * line_price)
        if raw_line.get("amount") not in (None, ""):
            supplied_amount = money(decimal(raw_line["amount"], f"Montant {expected_code}"))
            if supplied_amount != amount:
                raise BillingError(f"Le montant d’une ligne {expected_code} est incohérent.")
        lines.append({
            "label": str(raw_line.get("label") or "Consommation"),
            "quantity": str(line_quantity),
            "unit_price": str(line_price),
            "amount": str(amount),
        })
        subtotal += amount

    if "taxes" not in service or not isinstance(service["taxes"], list):
        raise BillingError(f"Les taxes du service {expected_code} doivent être explicites.")
    taxes = []
    tax_total = Decimal("0")
    for raw_tax in service["taxes"]:
        if not isinstance(raw_tax, dict) or not str(raw_tax.get("code", "")).strip():
            raise BillingError(f"Taxe invalide pour le service {expected_code}.")
        rate = decimal(raw_tax.get("rate"), f"Taux de taxe {expected_code}")
        basis = money(decimal(raw_tax.get("basis"), f"Base taxable {expected_code}"))
        amount = money(basis * rate / Decimal("100"))
        if raw_tax.get("amount") not in (None, ""):
            supplied_amount = money(decimal(raw_tax["amount"], f"Montant de taxe {expected_code}"))
            if supplied_amount != amount:
                raise BillingError(f"Le montant de taxe {expected_code} est incohérent.")
        taxes.append({"code": str(raw_tax["code"]), "rate": str(rate),
                      "basis": str(basis), "amount": str(amount)})
        tax_total += amount

    return {
        "code": expected_code,
        "mode": mode,
        "quantity": str(quantity),
        "unit": unit,
        "unit_price_excl_tax": str(unit_price),
        "lines": lines,
        "subtotal_excl_tax": str(money(subtotal)),
        "taxes": taxes,
        "total_incl_tax": str(money(subtotal + tax_total)),
    }


def build_combined_snapshot(electricity: dict, water: dict, currency: str) -> dict:
    """Build an additive combined snapshot from two explicit service inputs.

    No water rule or tax is inferred here: the caller must provide the water
    mode, unit, price, lines and an explicit taxes list (which may be empty
    only when that choice is deliberate in a controlled fixture/configuration).
    """
    currency = str(currency or "").strip().upper()
    if not currency:
        raise BillingError("La devise de la facture combinée est obligatoire.")
    services = [_combined_service(electricity, "ELECTRICITY"), _combined_service(water, "WATER")]
    subtotal = sum((Decimal(item["subtotal_excl_tax"]) for item in services), Decimal("0"))
    tax_total = sum((sum((Decimal(tax["amount"]) for tax in item["taxes"]), Decimal("0")) for item in services), Decimal("0"))
    total = sum((Decimal(item["total_incl_tax"]) for item in services), Decimal("0"))
    return {
        "document_kind": "COMBINED_WATER_ELECTRICITY",
        "schema_version": 2,
        "currency": currency,
        "services": services,
        "subtotal_excl_tax": str(money(subtotal)),
        "tax_total": str(money(tax_total)),
        "total": str(money(total)),
    }


def convert_combined_snapshot(snapshot: dict, exchange_rate: str, quote_currency: str = "CDF") -> dict:
    """Return a presentation conversion without changing the source snapshot."""
    if not isinstance(snapshot, dict) or snapshot.get("document_kind") != "COMBINED_WATER_ELECTRICITY":
        raise BillingError("Snapshot combiné invalide.")
    source_currency = str(snapshot.get("currency") or "").strip().upper()
    quote_currency = str(quote_currency or "").strip().upper()
    if not source_currency or not quote_currency:
        raise BillingError("Les devises de conversion sont obligatoires.")
    rate = decimal(exchange_rate, "Taux de change", allow_zero=False)
    result = deepcopy(snapshot)
    for service in result["services"]:
        service["unit_price_excl_tax"] = str(money(Decimal(service["unit_price_excl_tax"]) * rate))
        for line in service.get("lines", []):
            line["unit_price"] = str(money(Decimal(line["unit_price"]) * rate))
            line["amount"] = str(money(Decimal(line["amount"]) * rate))
        service["subtotal_excl_tax"] = str(money(Decimal(service["subtotal_excl_tax"]) * rate))
        for tax in service.get("taxes", []):
            tax["basis"] = str(money(Decimal(tax["basis"]) * rate))
            tax["amount"] = str(money(Decimal(tax["amount"]) * rate))
        service["total_incl_tax"] = str(money(Decimal(service["total_incl_tax"]) * rate))
    result["currency"] = quote_currency
    result["conversion_source_currency"] = source_currency
    result["conversion_rate"] = str(rate)
    result["subtotal_excl_tax"] = str(money(sum((Decimal(s["subtotal_excl_tax"]) for s in result["services"]), Decimal("0"))))
    result["tax_total"] = str(money(sum((sum((Decimal(t["amount"]) for t in s.get("taxes", [])), Decimal("0")) for s in result["services"]), Decimal("0"))))
    result["total"] = str(money(sum((Decimal(s["total_incl_tax"]) for s in result["services"]), Decimal("0"))))
    return result


def convert_amount(amount_usd: str, exchange_rate: str, quote_currency: str = "CDF") -> dict:
    """Create a presentation conversion without changing the USD invoice."""
    source = money(decimal(amount_usd, "Montant USD"))
    rate = decimal(exchange_rate, "Taux de change", allow_zero=False)
    return {"source_amount": str(source), "source_currency": "USD", "rate": str(rate),
            "currency": quote_currency, "amount": str(money(source * rate))}
