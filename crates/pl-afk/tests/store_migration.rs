//! Opening a real legacy `accounts.json` (written by Newtonsoft.Json and put through two legacy
//! load/save cycles, so its kick messages are tripled) and migrating its plain-text login commands
//! into the secret store.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pl_afk::{
    AccountStore, AfkPaths, LoadReport, MemorySecretStore, SecretStore, VersionCatalog,
    build_config, default_kick_messages_in, is_default_kick_messages, login_commands_key,
};
use pl_lang::Lang;

const PLAIN_ID: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";
const PASSWORD: &str = "çokGizli123";

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

fn legacy_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        golden("legacy-accounts.json"),
        dir.path().join("accounts.json"),
    )
    .unwrap();
    std::fs::copy(
        golden("legacy-settings.json"),
        dir.path().join("settings.json"),
    )
    .unwrap();
    dir
}

fn open(root: &Path, secrets: &Arc<MemorySecretStore>) -> (AccountStore, LoadReport) {
    let secrets: Arc<dyn SecretStore> = secrets.clone();
    AccountStore::open(AfkPaths::new(root), secrets).unwrap()
}

#[test]
fn legacy_file_loads_unchanged_and_secrets_move_to_the_keychain() {
    let root = legacy_root();
    let original = std::fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(
        original.contains(PASSWORD),
        "fixture carries a plain-text password"
    );

    let secrets = Arc::new(MemorySecretStore::new());
    let (store, report) = open(root.path(), &secrets);

    assert_eq!(report.corrupt_backup, None);
    assert_eq!(report.migrated_login_commands, 1);
    assert_eq!(report.pending_migrations, 0);
    assert_eq!(
        report.regenerated_ids, 1,
        "the account with an empty Id gets one"
    );
    assert_eq!(
        report.removed_duplicate_kick_messages, 24,
        "3 accounts x 8 duplicates appended by Newtonsoft"
    );

    let accounts = store.accounts();
    assert_eq!(accounts.len(), 3);

    let plain = &accounts[0];
    assert_eq!(plain.id, PLAIN_ID);
    assert_eq!(plain.nickname, "Ana_Hesap");
    assert!(plain.login_commands.enabled);
    // Loaded as written: the legacy English list word for word, still known as the default one.
    assert_eq!(
        plain.auto_relog.kick_messages,
        default_kick_messages_in(Lang::En)
    );
    assert!(is_default_kick_messages(&plain.auto_relog.kick_messages));

    let regenerated = &accounts[1];
    assert_eq!(regenerated.nickname, "Kimliksiz");
    assert_eq!(regenerated.id.len(), 32);
    assert_eq!(regenerated.server_host, "oyun.sunucu.net");
    assert_eq!(regenerated.server_port, 25566);

    let custom = &accounts[2];
    assert!(custom.auto_start && custom.allow_resource_packs);
    assert_eq!(custom.minecraft_version, "auto");
    assert!(!custom.auto_relog.ignore_kick_message);
    assert_eq!(custom.auto_relog.retries, 5);

    // The secret is in the keychain only.
    let stored = secrets.entries();
    assert_eq!(
        stored
            .get(&login_commands_key(PLAIN_ID))
            .map(String::as_str),
        Some(r#"["/login çokGizli123","/kit"]"#)
    );
    assert_eq!(
        store.login_commands(PLAIN_ID).unwrap(),
        vec!["/login çokGizli123", "/kit"]
    );
    let rewritten = std::fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(
        !rewritten.contains(PASSWORD),
        "accounts.json must not keep the password"
    );
    assert!(rewritten.contains("\"Commands\": []"));
    assert!(rewritten.contains("\"EffectiveHost\": \"oyun.sunucu.net\""));

    assert!(
        store.afk_settings().show_script_commands,
        "legacy settings.json is read"
    );

    // A second open has nothing left to repair and keeps everything.
    let (again, report) = open(root.path(), &secrets);
    assert_eq!(report, LoadReport::default());
    assert_eq!(again.accounts(), accounts);
    assert_eq!(again.login_commands(PLAIN_ID).unwrap().len(), 2);

    // The migrated account still produces the legacy config.
    let from_store = build_config(
        plain,
        &again.login_commands(PLAIN_ID).unwrap(),
        "Hazar",
        &VersionCatalog::built_in(),
    );
    assert!(
        from_store
            .login_script
            .contains("send /login çokGizli123\r\n")
    );
    assert!(
        from_store
            .ini
            .contains("Action = \"script giris-komutlari.txt\"")
    );
}

#[test]
fn unavailable_keychain_keeps_the_commands_where_they_were() {
    let root = legacy_root();
    let secrets = Arc::new(MemorySecretStore::new());
    secrets.set_failing(true);

    let (store, report) = open(root.path(), &secrets);
    assert_eq!(report.migrated_login_commands, 0);
    assert_eq!(report.pending_migrations, 1);
    // Still usable this run, and still on disk so nothing is lost.
    assert_eq!(store.login_commands(PLAIN_ID).unwrap().len(), 2);
    let file = std::fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(file.contains(PASSWORD));
    // Other repairs were saved meanwhile (ids, duplicates) and survive the next open.
    assert!(!file.contains("\"Id\": \"\""));

    // Once the keychain works the migration completes.
    secrets.set_failing(false);
    let (store, report) = open(root.path(), &secrets);
    assert_eq!(report.migrated_login_commands, 1);
    assert_eq!(report.pending_migrations, 0);
    assert_eq!(store.login_commands(PLAIN_ID).unwrap().len(), 2);
    let file = std::fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(!file.contains(PASSWORD));
}

#[test]
fn locked_account_file_is_reported_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    // A directory where the file should be: reading fails with an I/O error, not NotFound.
    std::fs::create_dir_all(root.path().join("accounts.json")).unwrap();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::new());
    assert!(AccountStore::open(AfkPaths::new(root.path()), secrets).is_err());
}
