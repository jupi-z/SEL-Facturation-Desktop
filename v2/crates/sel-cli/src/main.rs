use anyhow::Result;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sel_contracts::{utility_snapshot_sha256, InvoiceLineResponse, UtilityBillingResponse};
use sel_storage::{migrate_postgres, CentralStore};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    match command.as_str() {
        "health" => println!("sel-cli v2 backend utilities ready"),
        "bootstrap-admin" => {
            let identity_ref = args.next().ok_or_else(|| {
                anyhow::anyhow!("usage: sel-cli bootstrap-admin <identity-ref>")
            })?;
            anyhow::ensure!(
                args.next().is_none(),
                "usage: sel-cli bootstrap-admin <identity-ref>"
            );
            bootstrap_initial_admin(&identity_ref).await?;
        }
        "migration-dry-run" => {
            let path = args.next().ok_or_else(|| {
                anyhow::anyhow!(
                    "usage: sel-cli migration-dry-run <sel-facturation-portable-v1.json>"
                )
            })?;
            let report = migration_dry_run(Path::new(&path))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "migrate-v1" => {
            let mode = args.next().unwrap_or_default();
            let approved_flag = if mode == "--apply" {
                args.next()
            } else {
                None
            };
            let path = migration_source_path(&mode, approved_flag.as_deref(), args.next())?;
            let report = if mode == "--dry-run" {
                migration_dry_run(Path::new(&path))?
            } else {
                migration_apply(Path::new(&path)).await?
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "backup-postgres" => {
            let path = args.next().ok_or_else(|| {
                anyhow::anyhow!("usage: sel-cli backup-postgres <backup.dump>")
            })?;
            let database_url = std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for backup-postgres"))?;
            let path = Path::new(&path);
            create_pg_backup(&database_url, path)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "BACKUP_CREATED",
                    "path": path.display().to_string(),
                    "sha256": file_sha256(path)?
                }))?
            );
        }
        "restore-postgres" => {
            let path = args.next().ok_or_else(|| {
                anyhow::anyhow!("usage: sel-cli restore-postgres <backup.dump>")
            })?;
            require_restore_approval(std::env::var("SEL_RESTORE_APPROVED").ok().as_deref())?;
            let database_url = std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for restore-postgres"))?;
            let path = Path::new(&path);
            let pre_restore_backup = std::env::var("SEL_RESTORE_PRE_BACKUP_PATH")
                .map(PathBuf::from)
                .map_err(|_| anyhow::anyhow!("SEL_RESTORE_PRE_BACKUP_PATH is required for restore-postgres"))?;
            anyhow::ensure!(
                pre_restore_backup != path,
                "pre-restore backup path must differ from the restore source"
            );
            create_pg_backup(&database_url, &pre_restore_backup)?;
            restore_pg_backup(&database_url, path)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "RESTORE_COMPLETED",
                    "path": path.display().to_string(),
                    "pre_restore_backup": pre_restore_backup.display().to_string(),
                    "sha256": file_sha256(path)?
                }))?
            );
        }
        "reconcile" => {
            let path = args.next().ok_or_else(|| {
                anyhow::anyhow!("usage: sel-cli reconcile <portable-v1.json>")
            })?;
            let report = migration_reconcile(Path::new(&path)).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => println!("usage: sel-cli <health|bootstrap-admin <identity-ref>|migrate-v1 --dry-run <portable-v1.json>|migrate-v1 --apply --approved-manifest <portable-v1.json>|backup-postgres <backup.dump>|restore-postgres <backup.dump>|reconcile <portable-v1.json>"),
    }
    Ok(())
}

async fn bootstrap_initial_admin(identity_ref: &str) -> Result<()> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for bootstrap-admin"))?;
    let organization_id = std::env::var("SEL_ORGANIZATION_ID")
        .map_err(|_| anyhow::anyhow!("SEL_ORGANIZATION_ID is required for bootstrap-admin"))?
        .parse::<Uuid>()?;
    anyhow::ensure!(
        !organization_id.is_nil(),
        "SEL_ORGANIZATION_ID must not be nil"
    );
    let store = CentralStore::connect(&database_url, organization_id).await?;

    let password = rpassword::prompt_password("Mot de passe administrateur (saisie masquée) : ")?;
    let confirmation = rpassword::prompt_password("Confirmez le mot de passe (saisie masquée) : ")?;
    anyhow::ensure!(
        password == confirmation,
        "password confirmation does not match"
    );

    let admin_id = store
        .bootstrap_initial_admin(identity_ref, &password)
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "INITIAL_ADMIN_CREATED",
            "user_id": admin_id,
            "identity_ref": identity_ref.trim(),
            "role": "ADMIN"
        }))?
    );
    Ok(())
}

fn migration_source_path(
    mode: &str,
    approved_flag: Option<&str>,
    path: Option<String>,
) -> Result<String> {
    anyhow::ensure!(
        matches!(mode, "--dry-run" | "--apply"),
        "usage: sel-cli migrate-v1 --dry-run <file> | --apply --approved-manifest <file>"
    );
    if mode == "--apply" {
        anyhow::ensure!(
            approved_flag == Some("--approved-manifest"),
            "usage: sel-cli migrate-v1 --dry-run <file> | --apply --approved-manifest <file>"
        );
    }
    path.ok_or_else(|| {
        anyhow::anyhow!(
            "usage: sel-cli migrate-v1 --dry-run <file> | --apply --approved-manifest <file>"
        )
    })
}

async fn migration_apply(path: &Path) -> Result<serde_json::Value> {
    let bytes = fs::read(path)?;
    let mut digest = Sha256::new();
    digest.update(&bytes);
    let source_file_sha256 = hex::encode(digest.finalize());
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        manifest.get("format").and_then(serde_json::Value::as_str)
            == Some("sel-facturation-portable"),
        "unsupported source format"
    );
    anyhow::ensure!(
        manifest
            .get("schema_version")
            .and_then(serde_json::Value::as_i64)
            == Some(1),
        "unsupported portable schema version"
    );
    anyhow::ensure!(
        manifest
            .get("migration_approval")
            .and_then(|value| value.get("approved"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        "approved manifest must contain migration_approval.approved=true"
    );
    anyhow::ensure!(
        std::env::var("SEL_MIGRATION_APPROVED").ok().as_deref() == Some("I_UNDERSTAND"),
        "set SEL_MIGRATION_APPROVED=I_UNDERSTAND for an explicit import"
    );
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for --apply"))?;
    let organization_id = std::env::var("SEL_ORGANIZATION_ID")
        .map_err(|_| anyhow::anyhow!("SEL_ORGANIZATION_ID is required for --apply"))?
        .parse::<Uuid>()?;
    let backup_path = std::env::var("SEL_MIGRATION_BACKUP_PATH")
        .map(PathBuf::from)
        .map_err(|_| anyhow::anyhow!("SEL_MIGRATION_BACKUP_PATH is required for --apply"))?;
    create_pg_backup(&database_url, &backup_path)?;

    let store = CentralStore::connect(&database_url, organization_id).await?;
    migrate_postgres(&store.pool).await?;
    store
        .ensure_organization("SEL V1 imported organization")
        .await?;
    let data = manifest
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("portable manifest has no data object"))?;
    let clients = array(data, "clients")?;
    let invoices = array(data, "invoices")?;
    let payments = array(data, "payments")?;
    let mut tx = store.pool.begin().await?;
    let batch_id = Uuid::new_v4();
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM import_batches
         WHERE organization_id = $1 AND source_system = 'SEL_FACTURATION_V1'
           AND source_file_sha256 = $2",
    )
    .bind(organization_id)
    .bind(&source_file_sha256)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(existing) = existing {
        tx.rollback().await?;
        return Ok(serde_json::json!({
            "dry_run": false,
            "writes": 0,
            "status": "ALREADY_APPLIED",
            "import_batch_id": existing,
            "source_file_sha256": source_file_sha256,
            "backup": backup_path.display().to_string()
        }));
    }
    sqlx::query(
        "INSERT INTO import_batches
         (id, organization_id, source_system, source_schema_version, source_file_sha256, manifest_json, status)
         VALUES ($1, $2, 'SEL_FACTURATION_V1', 1, $3, $4, 'STARTED')",
    )
    .bind(batch_id)
    .bind(organization_id)
    .bind(&source_file_sha256)
    .bind(&manifest)
    .execute(&mut *tx)
    .await?;

    let mut customer_ids = HashMap::new();
    for client in clients {
        let code = required_string(client, "code")?;
        let customer_id = stable_legacy_uuid("CLIENT", code);
        let legal_name = required_string(client, "name")?;
        let contact_fields = serde_json::json!({
            "address": client.get("address").and_then(serde_json::Value::as_str),
            "phone": client.get("phone").and_then(serde_json::Value::as_str),
            "legacy_snapshot": client
        });
        sqlx::query(
            "INSERT INTO customers
             (id, organization_id, customer_code, legal_name, contact_fields, status, revision)
             VALUES ($1, $2, $3, $4, $5, 'ACTIVE', 1)
             ON CONFLICT (organization_id, customer_code) DO NOTHING",
        )
        .bind(customer_id)
        .bind(organization_id)
        .bind(code)
        .bind(legal_name)
        .bind(contact_fields)
        .execute(&mut *tx)
        .await?;
        insert_legacy_map(&mut tx, batch_id, "CLIENT", code, customer_id).await?;
        customer_ids.insert(code.to_owned(), customer_id);
    }

    let mut invoice_ids = HashMap::new();
    for invoice in invoices {
        let number = required_string(invoice, "number")?;
        let client_code = invoice
            .get("client_code")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                invoice
                    .get("client_snapshot")
                    .and_then(|value| value.get("code"))
                    .and_then(serde_json::Value::as_str)
            })
            .ok_or_else(|| anyhow::anyhow!("invoice {number} has no client_code"))?;
        let customer_id = *customer_ids.get(client_code).ok_or_else(|| {
            anyhow::anyhow!("invoice {number} references unknown client {client_code}")
        })?;
        let invoice_id = stable_legacy_uuid("INVOICE", number);
        let receivable_id = stable_legacy_uuid("RECEIVABLE", number);
        let calc_snapshot = invoice
            .get("calc_snapshot")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let response = legacy_invoice_response(invoice, &calc_snapshot, invoice_id, receivable_id)?;
        let document_snapshot = serde_json::json!({
            "legacy_v1": invoice,
            "v2_document": response
        });
        let currency = invoice
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("USD");
        let total = decimal_field(&calc_snapshot, "total")?;
        let period = invoice
            .get("billing_period")
            .and_then(serde_json::Value::as_str);
        let issue_date = invoice
            .get("issue_date")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("1970-01-01");
        let status = if invoice.get("status").and_then(serde_json::Value::as_str) == Some("VOID") {
            "VOID"
        } else {
            "CENTRAL_ACCEPTED"
        };
        sqlx::query(
            "INSERT INTO invoices
             (id, customer_id, period, invoice_number, issued_at, currency, total, template_version, immutable_snapshot, status)
             VALUES ($1, $2, $3, $4, $5::date, $6, $7, 'v1-import', $8, $9)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(invoice_id)
        .bind(customer_id)
        .bind(period)
        .bind(number)
        .bind(issue_date)
        .bind(currency)
        .bind(total)
        .bind(document_snapshot)
        .bind(status)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO invoice_lines
             (id, invoice_id, service_code, quantity, unit, unit_price, subtotal, tax_snapshot, total)
             VALUES ($1, $2, 'ELECTRICITY', $3, 'KWH', $4, $5, $6, $7)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(stable_legacy_uuid("INVOICE_LINE", number))
        .bind(invoice_id)
        .bind(response.lines[0].quantity)
        .bind(response.lines[0].unit_price)
        .bind(response.lines[0].subtotal)
        .bind(serde_json::json!({"tax": response.lines[0].tax, "source": "V1_SNAPSHOT"}))
        .bind(response.lines[0].total)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'INVOICE', $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(receivable_id)
        .bind(invoice_id)
        .bind(customer_id)
        .bind(currency)
        .bind(total)
        .bind(if status == "VOID" { "VOID" } else { "OPEN" })
        .execute(&mut *tx)
        .await?;
        insert_legacy_map(&mut tx, batch_id, "INVOICE", number, invoice_id).await?;
        insert_legacy_map(&mut tx, batch_id, "RECEIVABLE", number, receivable_id).await?;
        invoice_ids.insert(
            number.to_owned(),
            (invoice_id, receivable_id, currency.to_owned()),
        );
    }

    for payment in payments {
        let invoice_number = required_string(payment, "invoice_number")?;
        let (_invoice_id, receivable_id, invoice_currency) =
            invoice_ids.get(invoice_number).ok_or_else(|| {
                anyhow::anyhow!("payment references unknown invoice {invoice_number}")
            })?;
        let legacy_id = payment
            .get("id")
            .map(|value| value.to_string())
            .unwrap_or_else(|| invoice_number.to_owned());
        let payment_id = stable_legacy_uuid("PAYMENT", &format!("{invoice_number}:{legacy_id}"));
        let amount = payment
            .get("amount")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("payment {legacy_id} has no amount"))?
            .parse::<Decimal>()?;
        let currency = payment
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(invoice_currency);
        anyhow::ensure!(
            currency == invoice_currency,
            "payment {legacy_id} currency {currency} does not match invoice {invoice_number} currency {invoice_currency}; an approved conversion snapshot is required"
        );
        let paid_at = payment
            .get("paid_on")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("1970-01-01T00:00:00Z");
        sqlx::query(
            "INSERT INTO payments
             (id, customer_id, amount_received, currency, method, paid_at, status)
             SELECT $1, customer_id, $2, $3, $4, $5::timestamptz, 'CENTRAL_ACCEPTED'
             FROM receivables WHERE id = $6
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(payment_id)
        .bind(amount)
        .bind(currency)
        .bind(
            payment
                .get("method")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("V1_IMPORT"),
        )
        .bind(paid_at)
        .bind(receivable_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO payment_allocations
             (id, payment_id, receivable_id, allocated_amount, currency)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(stable_legacy_uuid(
            "PAYMENT_ALLOCATION",
            &format!("{invoice_number}:{legacy_id}"),
        ))
        .bind(payment_id)
        .bind(receivable_id)
        .bind(amount)
        .bind(currency)
        .execute(&mut *tx)
        .await?;
        insert_legacy_map(
            &mut tx,
            batch_id,
            "PAYMENT",
            &format!("{invoice_number}:{legacy_id}"),
            payment_id,
        )
        .await?;
    }
    sqlx::query("UPDATE import_batches SET status = 'APPLIED', applied_at = now() WHERE id = $1")
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(serde_json::json!({
        "dry_run": false,
        "writes": clients.len() + invoices.len() + payments.len(),
        "status": "APPLIED",
        "import_batch_id": batch_id,
        "source_file_sha256": source_file_sha256,
        "backup": backup_path.display().to_string(),
        "counts": {"clients": clients.len(), "invoices": invoices.len(), "payments": payments.len()}
    }))
}

async fn migration_reconcile(path: &Path) -> Result<serde_json::Value> {
    let bytes = fs::read(path)?;
    let mut digest = Sha256::new();
    digest.update(&bytes);
    let source_file_sha256 = hex::encode(digest.finalize());
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        manifest.get("format").and_then(serde_json::Value::as_str)
            == Some("sel-facturation-portable"),
        "unsupported source format"
    );
    anyhow::ensure!(
        manifest
            .get("schema_version")
            .and_then(serde_json::Value::as_i64)
            == Some(1),
        "unsupported portable schema version"
    );
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for reconcile"))?;
    let organization_id = std::env::var("SEL_ORGANIZATION_ID")
        .map_err(|_| anyhow::anyhow!("SEL_ORGANIZATION_ID is required for reconcile"))?
        .parse::<Uuid>()?;
    let store = CentralStore::connect(&database_url, organization_id).await?;
    let data = manifest
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("portable manifest has no data object"))?;
    let clients = array(data, "clients")?;
    let invoices = array(data, "invoices")?;
    let payments = array(data, "payments")?;

    let mut missing_clients = 0usize;
    for client in clients {
        let code = required_string(client, "code")?;
        let customer_id = stable_legacy_uuid("CLIENT", code);
        let found: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
                .bind(customer_id)
                .bind(organization_id)
                .fetch_optional(&store.pool)
                .await?;
        if found.is_none() {
            missing_clients += 1;
        }
    }

    let mut missing_invoices = 0usize;
    let mut missing_receivables = 0usize;
    let mut source_invoice_totals = BTreeMap::new();
    let mut stored_invoice_totals = BTreeMap::new();
    let mut source_outstanding_totals = BTreeMap::new();
    let mut stored_outstanding_totals = BTreeMap::new();
    let mut invoice_currencies = HashMap::new();
    for invoice in invoices {
        let number = required_string(invoice, "number")?;
        let currency = invoice
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("USD");
        let total = decimal_field(
            invoice
                .get("calc_snapshot")
                .ok_or_else(|| anyhow::anyhow!("invoice {number} has no calc_snapshot"))?,
            "total",
        )?;
        add_currency_total(&mut source_invoice_totals, currency, total);
        add_currency_total(&mut source_outstanding_totals, currency, total);
        invoice_currencies.insert(number, currency.to_owned());
        let invoice_id = stable_legacy_uuid("INVOICE", number);
        let found: Option<(String, Decimal)> = sqlx::query_as(
            "SELECT currency, total FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(invoice_id)
        .bind(organization_id)
        .fetch_optional(&store.pool)
        .await?;
        match found {
            Some((stored_currency, stored_total)) => {
                add_currency_total(&mut stored_invoice_totals, &stored_currency, stored_total);
            }
            None => missing_invoices += 1,
        }

        let receivable_id = stable_legacy_uuid("RECEIVABLE", number);
        let stored_balance: Option<(String, Decimal)> = sqlx::query_as(
            "SELECT r.currency,
                    r.principal_total - COALESCE(SUM(a.allocated_amount), 0)
             FROM receivables r
             LEFT JOIN payment_allocations a ON a.receivable_id = r.id
             WHERE r.id = $1 AND r.customer_id IN
               (SELECT id FROM customers WHERE organization_id = $2)
             GROUP BY r.id, r.currency, r.principal_total",
        )
        .bind(receivable_id)
        .bind(organization_id)
        .fetch_optional(&store.pool)
        .await?;
        match stored_balance {
            Some((stored_currency, balance)) => {
                add_currency_total(&mut stored_outstanding_totals, &stored_currency, balance)
            }
            None => missing_receivables += 1,
        }
    }

    let mut missing_payments = 0usize;
    let mut currency_mismatches = 0usize;
    let mut source_payment_totals = BTreeMap::new();
    let mut stored_payment_totals = BTreeMap::new();
    for payment in payments {
        let invoice_number = required_string(payment, "invoice_number")?;
        let legacy_id = payment
            .get("id")
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| invoice_number.to_owned());
        let payment_id = stable_legacy_uuid("PAYMENT", &format!("{invoice_number}:{legacy_id}"));
        let amount = payment
            .get("amount")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("payment {legacy_id} has no amount"))?
            .parse::<Decimal>()?;
        let invoice_currency = invoice_currencies
            .get(invoice_number)
            .map(String::as_str)
            .unwrap_or("USD");
        let currency = payment
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(invoice_currency);
        add_currency_total(&mut source_payment_totals, currency, amount);
        if currency != invoice_currency {
            currency_mismatches += 1;
        } else if let Some(balance) = source_outstanding_totals.get_mut(currency) {
            *balance -= amount;
        }
        let found: Option<(String, Decimal)> = sqlx::query_as(
            "SELECT currency, amount_received FROM payments WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(payment_id)
        .bind(organization_id)
        .fetch_optional(&store.pool)
        .await?;
        match found {
            Some((stored_currency, stored_amount)) => {
                add_currency_total(&mut stored_payment_totals, &stored_currency, stored_amount)
            }
            None => missing_payments += 1,
        }
    }

    let invoices_match = missing_invoices == 0 && source_invoice_totals == stored_invoice_totals;
    let payments_match = missing_payments == 0 && source_payment_totals == stored_payment_totals;
    let balances_match =
        missing_receivables == 0 && source_outstanding_totals == stored_outstanding_totals;
    let status = if missing_clients == 0
        && invoices_match
        && payments_match
        && balances_match
        && currency_mismatches == 0
    {
        "MATCH"
    } else {
        "MISMATCH"
    };
    Ok(serde_json::json!({
        "read_only": true,
        "writes": 0,
        "status": status,
        "source_file": path.display().to_string(),
        "source_file_sha256": source_file_sha256,
        "organization_id": organization_id,
        "counts": {
            "clients": clients.len(),
            "invoices": invoices.len(),
            "payments": payments.len(),
            "missing_clients": missing_clients,
            "missing_invoices": missing_invoices,
            "missing_payments": missing_payments,
            "missing_receivables": missing_receivables,
            "currency_mismatches": currency_mismatches
        },
        "totals": {
            "source_invoice_by_currency": normalized_currency_totals(&source_invoice_totals),
            "stored_invoice_by_currency": normalized_currency_totals(&stored_invoice_totals),
            "source_payment_by_currency": normalized_currency_totals(&source_payment_totals),
            "stored_payment_by_currency": normalized_currency_totals(&stored_payment_totals),
            "source_outstanding_by_currency": normalized_currency_totals(&source_outstanding_totals),
            "stored_outstanding_by_currency": normalized_currency_totals(&stored_outstanding_totals)
        }
    }))
}

fn add_currency_total(totals: &mut BTreeMap<String, Decimal>, currency: &str, amount: Decimal) {
    *totals.entry(currency.to_owned()).or_default() += amount;
}

fn normalized_currency_totals(totals: &BTreeMap<String, Decimal>) -> BTreeMap<String, String> {
    totals
        .iter()
        .map(|(currency, amount)| (currency.clone(), amount.normalize().to_string()))
        .collect()
}

fn create_pg_backup(database_url: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let status = Command::new("pg_dump")
        .args(["--format=custom", "--no-owner", "--no-acl", "--file"])
        .arg(path)
        .arg(database_url)
        .status()
        .map_err(|error| anyhow::anyhow!("pg_dump is required before import: {error}"))?;
    anyhow::ensure!(status.success(), "pg_dump failed with status {status}");
    Ok(())
}

fn restore_pg_backup(database_url: &str, path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .map_err(|error| anyhow::anyhow!("backup file cannot be read: {error}"))?;
    anyhow::ensure!(metadata.is_file(), "backup path is not a regular file");
    anyhow::ensure!(metadata.len() > 0, "backup file is empty");
    let status = Command::new("pg_restore")
        .args([
            "--exit-on-error",
            "--single-transaction",
            "--clean",
            "--if-exists",
            "--no-owner",
            "--no-acl",
            "--dbname",
        ])
        .arg(database_url)
        .arg(path)
        .status()
        .map_err(|error| anyhow::anyhow!("pg_restore is required for restore-postgres: {error}"))?;
    anyhow::ensure!(status.success(), "pg_restore failed with status {status}");
    Ok(())
}

fn require_restore_approval(value: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        value == Some("I_UNDERSTAND"),
        "set SEL_RESTORE_APPROVED=I_UNDERSTAND for an explicit PostgreSQL restore"
    );
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

async fn insert_legacy_map(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch_id: Uuid,
    entity: &str,
    legacy_key: &str,
    target_uuid: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO legacy_id_map (source_system, entity, legacy_key, target_uuid, import_batch_id)
         VALUES ('SEL_FACTURATION_V1', $1, $2, $3, $4)
         ON CONFLICT (source_system, entity, legacy_key) DO NOTHING",
    )
    .bind(entity)
    .bind(legacy_key)
    .bind(target_uuid)
    .bind(batch_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn stable_legacy_uuid(entity: &str, key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("SEL_FACTURATION_V1:{entity}:{key}").as_bytes(),
    )
}

fn required_string<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("portable item is missing string field {key}"))
}

fn decimal_field(value: &serde_json::Value, key: &str) -> Result<Decimal> {
    let raw = value
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("legacy snapshot is missing decimal field {key}"))?;
    match raw {
        serde_json::Value::String(value) => Ok(value.parse()?),
        serde_json::Value::Number(value) => Ok(value.to_string().parse()?),
        _ => Err(anyhow::anyhow!("legacy field {key} is not a decimal")),
    }
}

fn legacy_invoice_response(
    invoice: &serde_json::Value,
    calc: &serde_json::Value,
    invoice_id: Uuid,
    receivable_id: Uuid,
) -> Result<UtilityBillingResponse> {
    let quantity = decimal_field(calc, "kwh").unwrap_or(Decimal::ZERO);
    let subtotal = decimal_field(calc, "energy")
        .unwrap_or_else(|_| decimal_field(calc, "subtotal").unwrap_or(Decimal::ZERO));
    let tax = decimal_field(calc, "vat").unwrap_or(Decimal::ZERO);
    let total = decimal_field(calc, "total")?;
    let unit_price = calc
        .get("unit_price")
        .map(|_| decimal_field(calc, "unit_price"))
        .transpose()?;
    let mut response = UtilityBillingResponse {
        invoice_id,
        receivable_id: Some(receivable_id),
        invoice_number: invoice
            .get("number")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        status: "CENTRAL_ACCEPTED".into(),
        billing_period: invoice
            .get("period")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        due_date: invoice
            .get("due_date")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()),
        currency: invoice
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("USD")
            .into(),
        lines: vec![InvoiceLineResponse {
            service_code: "ELECTRICITY".into(),
            quantity,
            unit: "KWH".into(),
            unit_price,
            subtotal,
            tax,
            total,
        }],
        subtotal,
        tax,
        total,
        snapshot_sha256: String::new(),
    };
    response.snapshot_sha256 = utility_snapshot_sha256(&response)?;
    Ok(response)
}

fn migration_dry_run(path: &Path) -> Result<serde_json::Value> {
    let bytes = fs::read(path)?;
    let mut digest = Sha256::new();
    digest.update(&bytes);
    let source_file_sha256 = hex::encode(digest.finalize());
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        manifest.get("format").and_then(serde_json::Value::as_str)
            == Some("sel-facturation-portable"),
        "unsupported source format"
    );
    anyhow::ensure!(
        manifest
            .get("schema_version")
            .and_then(serde_json::Value::as_i64)
            == Some(1),
        "unsupported portable schema version"
    );
    let data = manifest
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("portable manifest has no data object"))?;
    let clients = array(data, "clients")?;
    let invoices = array(data, "invoices")?;
    let payments = array(data, "payments")?;
    let client_codes = unique_values(clients, "code")?;
    let invoice_numbers = unique_values(invoices, "number")?;
    let mut currency_mismatches = 0usize;
    let missing_payment_invoices = payments
        .iter()
        .filter(|payment| {
            payment
                .get("invoice_number")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|number| !invoice_numbers.contains(number))
        })
        .count();
    for payment in payments {
        let Some(invoice_number) = payment
            .get("invoice_number")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(invoice) = invoices.iter().find(|invoice| {
            invoice.get("number").and_then(serde_json::Value::as_str) == Some(invoice_number)
        }) else {
            continue;
        };
        let invoice_currency = invoice
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("USD");
        let payment_currency = payment
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(invoice_currency);
        if payment_currency != invoice_currency {
            currency_mismatches += 1;
        }
    }
    let blocked = missing_payment_invoices > 0
        || currency_mismatches > 0
        || clients.len() != client_codes.len()
        || invoices.len() != invoice_numbers.len();
    Ok(serde_json::json!({
        "dry_run": true,
        "writes": 0,
        "status": if currency_mismatches > 0 { "BLOCKED_CURRENCY_MISMATCH" } else if blocked { "BLOCKED_VALIDATION" } else { "READY_FOR_APPROVED_MANIFEST" },
        "source_file": path.display().to_string(),
        "source_file_sha256": source_file_sha256,
        "source_schema_version": 1,
        "counts": { "clients": clients.len(), "invoices": invoices.len(), "payments": payments.len() },
        "duplicate_client_codes": clients.len().saturating_sub(client_codes.len()),
        "duplicate_invoice_numbers": invoices.len().saturating_sub(invoice_numbers.len()),
        "missing_payment_invoice_references": missing_payment_invoices,
        "currency_mismatches": currency_mismatches,
        "operator_approval_required": true,
        "source_untouched": true
    }))
}

fn array<'a>(data: &'a serde_json::Value, key: &str) -> Result<&'a [serde_json::Value]> {
    data.get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow::anyhow!("portable data.{key} must be an array"))
}

fn unique_values(items: &[serde_json::Value], key: &str) -> Result<HashSet<String>> {
    let mut values = HashSet::new();
    for item in items {
        let value = item
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("portable item is missing string field {key}"))?;
        values.insert(value.to_owned());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::{
        create_pg_backup, migration_apply, migration_dry_run, migration_reconcile,
        migration_source_path, require_restore_approval, restore_pg_backup,
    };
    use serde_json::json;
    use std::{
        fs,
        str::FromStr,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn migration_dry_run_is_read_only_and_counts_references() {
        let path = std::env::temp_dir().join(format!(
            "sel-v1-fixture-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"{"format":"sel-facturation-portable","schema_version":1,"data":{"clients":[{"code":"C-1"}],"invoices":[{"number":"INV-1"}],"payments":[{"invoice_number":"INV-1"}]}}"#,
        )
        .expect("write fixture");
        let before = fs::read(&path).expect("read fixture");
        let report = migration_dry_run(&path).expect("dry run");
        assert_eq!(report["dry_run"], true);
        assert_eq!(report["writes"], 0);
        assert_eq!(report["missing_payment_invoice_references"], 0);
        assert_eq!(fs::read(&path).expect("read fixture"), before);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn migration_dry_run_blocks_payment_currency_mismatch() {
        let path = std::env::temp_dir().join(format!(
            "sel-v1-currency-fixture-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"{"format":"sel-facturation-portable","schema_version":1,"data":{"clients":[{"code":"C-1"}],"invoices":[{"number":"INV-1","currency":"USD"}],"payments":[{"invoice_number":"INV-1","amount":"5.00","currency":"CDF"}]}}"#,
        )
        .expect("write fixture");

        let report = migration_dry_run(&path).expect("dry run should return a blocking report");

        assert_eq!(report["status"], "BLOCKED_CURRENCY_MISMATCH");
        assert_eq!(report["currency_mismatches"], 1);
        assert_eq!(report["writes"], 0);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn postgres_restore_is_fail_closed_without_explicit_approval() {
        assert!(require_restore_approval(None).is_err());
        assert!(require_restore_approval(Some("yes")).is_err());
        assert!(require_restore_approval(Some("I_UNDERSTAND")).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and PostgreSQL client tools"]
    async fn postgres_backup_restore_round_trip_uses_isolated_database() {
        use sqlx::{postgres::PgConnectOptions, Connection, Executor, PgConnection};

        let test_database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be configured for PostgreSQL restore integration");
        let database_name = format!("sel_restore_{}", uuid::Uuid::new_v4().simple());
        let path_query = test_database_url
            .find('?')
            .unwrap_or(test_database_url.len());
        let database_separator = test_database_url[..path_query]
            .rfind('/')
            .expect("PostgreSQL URL must include a database path");
        let isolated_database_url = format!(
            "{}{database_name}{}",
            &test_database_url[..=database_separator],
            &test_database_url[path_query..]
        );
        let backup_path = std::env::temp_dir().join(format!("{database_name}.dump"));
        let admin_options = PgConnectOptions::from_str(&test_database_url)
            .expect("parse test PostgreSQL URL")
            .database("postgres");
        // A database name is generated only from a fixed prefix and a UUID.
        let mut admin = PgConnection::connect_with(&admin_options)
            .await
            .expect("connect to PostgreSQL test server");
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&mut admin)
            .await
            .expect("create isolated restore database");

        let result: anyhow::Result<()> = async {
            let mut isolated = PgConnection::connect(&isolated_database_url).await?;
            isolated
                .execute(
                    "CREATE TABLE restore_probe (id integer PRIMARY KEY, value text NOT NULL);
                     INSERT INTO restore_probe (id, value) VALUES (1, 'before-backup');",
                )
                .await?;
            isolated.close().await?;

            create_pg_backup(&isolated_database_url, &backup_path)?;
            anyhow::ensure!(fs::metadata(&backup_path)?.len() > 0, "backup is empty");

            let mut isolated = PgConnection::connect(&isolated_database_url).await?;
            sqlx::query("UPDATE restore_probe SET value = 'after-backup' WHERE id = 1")
                .execute(&mut isolated)
                .await?;
            isolated.close().await?;

            restore_pg_backup(&isolated_database_url, &backup_path)?;

            let mut restored = PgConnection::connect(&isolated_database_url).await?;
            let value: String = sqlx::query_scalar("SELECT value FROM restore_probe WHERE id = 1")
                .fetch_one(&mut restored)
                .await?;
            anyhow::ensure!(
                value == "before-backup",
                "restore did not recover the backup state"
            );
            restored.close().await?;
            Ok(())
        }
        .await;

        let drop_result = sqlx::query(&format!("DROP DATABASE IF EXISTS {database_name}"))
            .execute(&mut admin)
            .await;
        admin
            .close()
            .await
            .expect("close PostgreSQL admin connection");
        let _ = fs::remove_file(&backup_path);
        drop_result.expect("drop isolated restore database");
        result.expect("PostgreSQL backup/restore round trip");
    }

    #[test]
    fn migration_apply_arguments_keep_the_manifest_path() {
        assert_eq!(
            migration_source_path(
                "--apply",
                Some("--approved-manifest"),
                Some("fixture.json".into())
            )
            .expect("valid apply arguments"),
            "fixture.json"
        );
        assert!(migration_source_path("--apply", Some("fixture.json"), None).is_err());
        assert!(migration_source_path("--dry-run", None, None).is_err());
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and pg_dump for a real migration fixture"]
    async fn migration_apply_is_transactional_and_idempotent_on_postgres() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for the migration integration fixture");
        let organization_id = uuid::Uuid::new_v4();
        let suffix = organization_id.to_string();
        let source_code = format!("MIG-{suffix}");
        let invoice_number = format!("V1-{suffix}");
        let path = std::env::temp_dir().join(format!("sel-v1-migration-{suffix}.json"));
        let backup = std::env::temp_dir().join(format!("sel-v1-migration-{suffix}.dump"));
        let manifest = json!({
            "format": "sel-facturation-portable",
            "schema_version": 1,
            "migration_approval": {"approved": true},
            "data": {
                "clients": [{"code": source_code, "name": "Migration fixture"}],
                "invoices": [{
                    "number": invoice_number,
                    "client_code": source_code,
                    "currency": "USD",
                    "billing_period": "2026-11",
                    "issue_date": "2026-11-03",
                    "calc_snapshot": {
                        "kwh": "10.00",
                        "energy": "10.00",
                        "unit_price": "1.00",
                        "vat": "1.60",
                        "total": "11.60"
                    }
                }],
                "payments": [{
                    "id": "payment-1",
                    "invoice_number": invoice_number,
                    "amount": "5.00",
                    "currency": "USD",
                    "method": "CASH",
                    "paid_on": "2026-11-03T10:00:00Z"
                }]
            }
        });
        fs::write(
            &path,
            serde_json::to_vec(&manifest).expect("serialize fixture"),
        )
        .expect("write migration fixture");
        let prior_database_url = std::env::var_os("DATABASE_URL");
        let prior_org = std::env::var_os("SEL_ORGANIZATION_ID");
        let prior_approval = std::env::var_os("SEL_MIGRATION_APPROVED");
        let prior_backup = std::env::var_os("SEL_MIGRATION_BACKUP_PATH");
        std::env::set_var("DATABASE_URL", &database_url);
        std::env::set_var("SEL_ORGANIZATION_ID", organization_id.to_string());
        std::env::set_var("SEL_MIGRATION_APPROVED", "I_UNDERSTAND");
        std::env::set_var("SEL_MIGRATION_BACKUP_PATH", &backup);

        let first = migration_apply(&path)
            .await
            .expect("apply migration fixture");
        assert_eq!(first["status"], "APPLIED");
        assert_eq!(
            first["counts"],
            json!({"clients": 1, "invoices": 1, "payments": 1})
        );
        let second = migration_apply(&path)
            .await
            .expect("replay migration fixture");
        assert_eq!(second["status"], "ALREADY_APPLIED");
        assert_eq!(second["writes"], 0);
        let reconciliation = migration_reconcile(&path)
            .await
            .expect("reconcile migrated invoices, payments, and receivable balances");
        assert_eq!(reconciliation["status"], "MATCH");
        assert_eq!(
            reconciliation["totals"]["source_outstanding_by_currency"]["USD"],
            "6.6"
        );
        assert_eq!(
            reconciliation["totals"]["stored_outstanding_by_currency"]["USD"],
            "6.6"
        );

        let store = sel_storage::CentralStore::connect(&database_url, organization_id)
            .await
            .expect("connect imported organization");
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT count(*) FROM customers WHERE organization_id = $1),
               (SELECT count(*) FROM invoices WHERE customer_id IN (SELECT id FROM customers WHERE organization_id = $1)),
               (SELECT count(*) FROM payments WHERE customer_id IN (SELECT id FROM customers WHERE organization_id = $1))",
        )
        .bind(organization_id)
        .fetch_one(&store.pool)
        .await
        .expect("read imported counts");
        assert_eq!(counts, (1, 1, 1));
        assert!(backup.is_file(), "migration backup should be created");

        let failed_organization_id = uuid::Uuid::new_v4();
        let failed_suffix = failed_organization_id.to_string();
        let failed_source_code = format!("MIG-FAIL-{failed_suffix}");
        let failed_path =
            std::env::temp_dir().join(format!("sel-v1-migration-fail-{failed_suffix}.json"));
        let failed_backup =
            std::env::temp_dir().join(format!("sel-v1-migration-fail-{failed_suffix}.dump"));
        let failed_manifest = json!({
            "format": "sel-facturation-portable",
            "schema_version": 1,
            "migration_approval": {"approved": true},
            "data": {
                "clients": [{"code": failed_source_code, "name": "Rollback fixture"}],
                "invoices": [
                    {
                        "number": format!("V1-FAIL-OK-{failed_suffix}"),
                        "client_code": failed_source_code,
                        "currency": "USD",
                        "billing_period": "2026-11",
                        "issue_date": "2026-11-03",
                        "calc_snapshot": {"kwh": "1.00", "energy": "1.00", "unit_price": "1.00", "vat": "0.16", "total": "1.16"}
                    },
                    {
                        "number": format!("V1-FAIL-BAD-{failed_suffix}"),
                        "client_code": failed_source_code,
                        "currency": "USD",
                        "billing_period": "2026-12",
                        "issue_date": "not-a-date",
                        "calc_snapshot": {"kwh": "2.00", "energy": "2.00", "unit_price": "1.00", "vat": "0.32", "total": "2.32"}
                    }
                ],
                "payments": []
            }
        });
        fs::write(
            &failed_path,
            serde_json::to_vec(&failed_manifest).expect("serialize rollback fixture"),
        )
        .expect("write rollback fixture");
        std::env::set_var("SEL_ORGANIZATION_ID", failed_organization_id.to_string());
        std::env::set_var("SEL_MIGRATION_BACKUP_PATH", &failed_backup);
        assert!(
            migration_apply(&failed_path).await.is_err(),
            "a malformed later invoice must abort the complete import transaction"
        );
        let failed_store =
            sel_storage::CentralStore::connect(&database_url, failed_organization_id)
                .await
                .expect("connect rollback organization");
        let failed_counts: (i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT count(*) FROM customers WHERE organization_id = $1),
               (SELECT count(*) FROM invoices WHERE customer_id IN (SELECT id FROM customers WHERE organization_id = $1))",
        )
        .bind(failed_organization_id)
        .fetch_one(&failed_store.pool)
        .await
        .expect("read rollback counts");
        assert_eq!(failed_counts, (0, 0));

        for (key, value) in [
            ("DATABASE_URL", prior_database_url),
            ("SEL_ORGANIZATION_ID", prior_org),
            ("SEL_MIGRATION_APPROVED", prior_approval),
            ("SEL_MIGRATION_BACKUP_PATH", prior_backup),
        ] {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(backup);
        let _ = fs::remove_file(failed_path);
        let _ = fs::remove_file(failed_backup);
    }
}
