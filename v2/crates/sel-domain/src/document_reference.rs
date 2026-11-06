use chrono::{DateTime, FixedOffset, Utc};
use uuid::Uuid;

/// Printed invoice number in SEL's business timezone (Lubumbashi, UTC+2).
/// The unique invoice ID stays in its internal QR payload.
pub fn issued_invoice_reference(issued_at: DateTime<Utc>) -> String {
    let lubumbashi = FixedOffset::east_opt(2 * 60 * 60).expect("UTC+2 is a valid offset");
    format!(
        "SEL-{}",
        issued_at.with_timezone(&lubumbashi).format("%Y%m%d-%H%M%S")
    )
}

/// Stable unique reference for non-invoice documents.
pub fn issued_document_reference(issued_at: DateTime<Utc>, document_id: Uuid) -> String {
    format!(
        "{}-{}",
        issued_invoice_reference(issued_at),
        document_id.simple().to_string().to_ascii_uppercase()
    )
}

#[cfg(test)]
mod tests {
    use super::{issued_document_reference, issued_invoice_reference};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    #[test]
    fn invoice_reference_uses_lubumbashi_time_without_visible_suffix() {
        let issued_at: DateTime<Utc> = "2026-11-06T22:15:09Z".parse().unwrap();
        let reference = issued_invoice_reference(issued_at);
        assert_eq!(reference, "SEL-20261107-001509");
        assert_eq!(reference.len(), 19);
    }

    #[test]
    fn other_document_references_are_unique_within_the_same_second() {
        let issued_at: DateTime<Utc> = "2026-11-06T10:15:09Z".parse().unwrap();
        let first = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let second = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174001").unwrap();
        assert_ne!(
            issued_document_reference(issued_at, first),
            issued_document_reference(issued_at, second)
        );
        assert_eq!(issued_invoice_reference(issued_at), "SEL-20261106-121509");
    }
}
