use std::{fs, path::PathBuf};

#[test]
fn postgres_migrations_use_lf_bytes_on_every_platform() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let mut files = fs::read_dir(&directory)
        .expect("PostgreSQL migration directory must be readable")
        .map(|entry| entry.expect("migration directory entry must be readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .collect::<Vec<_>>();
    files.sort();

    assert!(!files.is_empty(), "PostgreSQL migrations must be present");
    for path in files {
        let bytes = fs::read(&path).expect("migration must be readable");
        assert!(
            !bytes.windows(2).any(|window| window == b"\r\n"),
            "{} must use the LF line endings whose bytes SQLx hashes",
            path.display()
        );
        assert!(
            bytes.contains(&b'\n'),
            "{} must contain LF line endings",
            path.display()
        );
    }
}
