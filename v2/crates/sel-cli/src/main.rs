use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    match command.as_str() {
        "health" => println!("sel-cli v2 backend utilities ready"),
        "migration-dry-run" => {
            println!("{{\"dry_run\":true,\"writes\":0,\"status\":\"READY_FOR_APPROVED_MANIFEST\"}}")
        }
        "reconcile" => println!("{{\"status\":\"NO_SOURCE_SELECTED\",\"writes\":0}}"),
        _ => println!("usage: sel-cli <health|migration-dry-run|reconcile>"),
    }
    Ok(())
}
