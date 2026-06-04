use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

fn parse_migration_version(file_name: &str) -> Option<i32> {
    if !file_name.ends_with(".sql") {
        return None;
    }

    let bytes = file_name.as_bytes();
    if bytes.len() < 8 {
        return None;
    }

    if !bytes[0].is_ascii_digit() || !bytes[1].is_ascii_digit() || !bytes[2].is_ascii_digit() {
        return None;
    }

    if bytes[3] != b'_' {
        return None;
    }

    file_name[0..3].parse::<i32>().ok()
}

fn build_generated_migrations() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR");
    let migrations_dir = Path::new(&manifest_dir).join("sql/migrations");
    let out_dir = env::var("OUT_DIR").expect("missing OUT_DIR");
    let generated_path = Path::new(&out_dir).join("generated_migrations.rs");

    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    let entries = fs::read_dir(&migrations_dir)
        .unwrap_or_else(|error| panic!("read migrations dir failed: {error}"));

    let mut migrations = BTreeMap::<i32, String>::new();
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("read migration entry failed: {error}"));
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy().to_string();
        if let Some(version) = parse_migration_version(&file_name) {
            let previous = migrations.insert(version, file_name.clone());
            if previous.is_some() {
                panic!("duplicate migration version: {version:03}");
            }
            println!(
                "cargo:rerun-if-changed={}",
                migrations_dir.join(&file_name).display()
            );
        }
    }

    if migrations.is_empty() {
        panic!("no versioned migration file found in sql/migrations");
    }

    let mut generated = String::new();
    generated.push_str("pub const GENERATED_MIGRATIONS: &[(i32, &str)] = &[\n");
    for (version, file_name) in migrations {
        generated.push_str(&format!(
            "    ({version}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/sql/migrations/{file_name}\"))),\n"
        ));
    }
    generated.push_str("];\n");

    fs::write(&generated_path, generated)
        .unwrap_or_else(|error| panic!("write generated migrations failed: {error}"));
}

fn main() {
    build_generated_migrations();
    tauri_build::build();
}
