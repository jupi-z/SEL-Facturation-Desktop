use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::Path};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    match command.as_str() {
        "health" => println!("sel-cli v2 backend utilities ready"),
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
            anyhow::ensure!(
                args.next().as_deref() == Some("--dry-run"),
                "usage: sel-cli migrate-v1 --dry-run <sel-facturation-portable-v1.json>"
            );
            let path = args.next().ok_or_else(|| {
                anyhow::anyhow!(
                    "usage: sel-cli migrate-v1 --dry-run <sel-facturation-portable-v1.json>"
                )
            })?;
            let report = migration_dry_run(Path::new(&path))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "reconcile" => println!("{{\"status\":\"NO_SOURCE_SELECTED\",\"writes\":0}}"),
        _ => println!("usage: sel-cli <health|migrate-v1 --dry-run <portable-v1.json>|reconcile>"),
    }
    Ok(())
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
    let missing_payment_invoices = payments
        .iter()
        .filter(|payment| {
            payment
                .get("invoice_number")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|number| !invoice_numbers.contains(number))
        })
        .count();
    Ok(serde_json::json!({
        "dry_run": true,
        "writes": 0,
        "status": if missing_payment_invoices == 0 { "READY_FOR_APPROVED_MANIFEST" } else { "BLOCKED_MISSING_REFERENCES" },
        "source_file": path.display().to_string(),
        "source_file_sha256": source_file_sha256,
        "source_schema_version": 1,
        "counts": { "clients": clients.len(), "invoices": invoices.len(), "payments": payments.len() },
        "duplicate_client_codes": clients.len().saturating_sub(client_codes.len()),
        "duplicate_invoice_numbers": invoices.len().saturating_sub(invoice_numbers.len()),
        "missing_payment_invoice_references": missing_payment_invoices,
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
    use super::migration_dry_run;
    use std::{
        fs,
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
}
