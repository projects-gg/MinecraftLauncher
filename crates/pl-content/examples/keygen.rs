//! Generate an Ed25519 content-signing keypair.
//!
//! `cargo run -p pl-content --example keygen -- <private-key-output-path>`
//!
//! Writes the private key (base64) to the given path, which must not exist yet, and prints the
//! public key and key id. Put the public key into `pl_content::CONTENT_PUBLIC_KEYS`
//! (`src/signing.rs`, read by the launcher and Studio) and import the private key into Projects
//! Studio. Never commit the private key.

use std::io::Write;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: keygen <private-key-output-path>");
        std::process::exit(2);
    };
    let path = std::path::PathBuf::from(path);
    if path.exists() {
        eprintln!("refusing to overwrite {}", path.display());
        std::process::exit(1);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create key directory");
    }
    let signing = pl_content::generate_signing_key().expect("OS random source");
    let verifying = signing.verifying_key();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("create key file");
    file.write_all(pl_content::encode_signing_key(&signing).as_bytes())
        .expect("write key file");
    println!("private key written to {}", path.display());
    println!(
        "public key: {}",
        pl_content::encode_verifying_key(&verifying)
    );
    println!("key id:     {}", pl_content::key_id(&verifying));
}
