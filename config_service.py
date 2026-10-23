"""Lecture centralisée de la configuration métier SEL.

Les anciens champs de ``settings`` restent disponibles pour l’export v1 et la
compatibilité. Les nouvelles fonctions de ce module fournissent une source
typée et datée pour les écrans et le moteur de facturation.
"""
from __future__ import annotations

import db
from datetime import date


def active_customer_types(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,active,sort_order FROM customer_types "
        "WHERE active=1 ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def all_customer_types(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,active,sort_order FROM customer_types ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def customer_type_label(connection, code: str) -> str:
    row = connection.execute("SELECT label FROM customer_types WHERE code=?", (code,)).fetchone()
    return row["label"] if row else code


def active_currencies(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,decimals,active,sort_order FROM currency_catalog "
        "WHERE active=1 ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def all_currencies(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,decimals,active,sort_order FROM currency_catalog ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def active_payment_methods(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,reference_required,active,sort_order FROM payment_methods "
        "WHERE active=1 ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def all_payment_methods(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT code,label,reference_required,active,sort_order FROM payment_methods ORDER BY sort_order, code"
    ).fetchall()
    return [dict(row) for row in rows]


def all_tax_profiles(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT * FROM tax_profiles ORDER BY effective_from DESC, code"
    ).fetchall()
    return [dict(row) for row in rows]


def tariff_history(connection) -> list[dict]:
    rows = connection.execute(
        """SELECT t.*, c.label AS customer_type_label
           FROM tariff_plans t LEFT JOIN customer_types c ON c.code=t.customer_type
           ORDER BY t.effective_from DESC, t.customer_type, t.id DESC"""
    ).fetchall()
    return [dict(row) for row in rows]


def exchange_rate_history(connection) -> list[dict]:
    rows = connection.execute(
        "SELECT * FROM exchange_rates ORDER BY effective_on DESC, id DESC"
    ).fetchall()
    return [dict(row) for row in rows]


def configuration_history(connection, limit: int = 200) -> list[dict]:
    rows = connection.execute(
        "SELECT * FROM configuration_history ORDER BY id DESC LIMIT ?", (limit,)
    ).fetchall()
    return [dict(row) for row in rows]


def tax_profile_for(
    connection,
    on_date: str,
    code: str | None = "SEL_STANDARD",
    service_code: str = "ELECTRICITY",
    require_confirmed: bool = False,
) -> dict:
    if service_code not in {"ELECTRICITY", "WATER"}:
        raise ValueError(f"Service inconnu : {service_code}.")
    clauses = ["service_code=?", "active=1", "effective_from<=?"]
    params = [service_code, on_date]
    if code:
        clauses.insert(0, "code=?")
        params.insert(0, code)
    if require_confirmed:
        clauses.append("confirmed=1")
    row = connection.execute(
        "SELECT * FROM tax_profiles WHERE " + " AND ".join(clauses)
        + " ORDER BY effective_from DESC, code LIMIT 1",
        params,
    ).fetchone()
    if not row:
        qualification = "confirmé " if require_confirmed else ""
        raise ValueError(
            f"Aucun profil fiscal {qualification}applicable au service {service_code} "
            f"à la date {on_date}."
        )
    return dict(row)


def billing_policy(connection) -> dict[str, str]:
    return {
        row["key"]: row["value"]
        for row in connection.execute("SELECT key,value FROM billing_policy")
    }


def billing_configuration(connection, on_date: str) -> dict:
    settings = db.settings(connection)
    primary = settings.get("currency") or "USD"
    currency = connection.execute(
        "SELECT * FROM currency_catalog WHERE code=? AND active=1", (primary,)
    ).fetchone()
    if not currency:
        currency = connection.execute(
            "SELECT * FROM currency_catalog WHERE code='USD' AND active=1"
        ).fetchone()
    if not currency:
        raise ValueError("Aucune devise principale active.")
    return {
        "currency": dict(currency),
        "tax": tax_profile_for(connection, on_date),
        "exchange_rate": dict(db.exchange_rate_for(connection, "USD", "CDF", on_date)),
        "policy": billing_policy(connection),
        "customer_types": active_customer_types(connection),
    }


def current_tariffs(connection, on_date: str | None = None) -> list[dict]:
    """Return one editable tariff row per active customer type."""
    effective = on_date or date.today().replace(day=1).isoformat()
    rows = []
    for customer_type in active_customer_types(connection):
        plan = connection.execute(
            "SELECT * FROM tariff_plans WHERE service_code='ELECTRICITY' "
            "AND customer_type=? AND effective_from<=? "
            "ORDER BY effective_from DESC, id DESC LIMIT 1",
            (customer_type["code"], effective),
        ).fetchone()
        item = dict(customer_type)
        item["unit_price"] = plan["unit_price"] if plan else ""
        item["currency"] = plan["currency"] if plan else "USD"
        item["effective_from"] = plan["effective_from"] if plan else effective
        item["tariff_code"] = plan["code"] if plan else ""
        rows.append(item)
    return rows


def record_change(connection, section: str, key: str, old_value: str, new_value: str, reason: str = "") -> None:
    if old_value == new_value:
        return
    connection.execute(
        "INSERT INTO configuration_history(section,config_key,old_value,new_value,reason) "
        "VALUES(?,?,?,?,?)",
        (section, key, old_value, new_value, reason),
    )
