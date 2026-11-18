//! Explicit live check for a disposable CLI session approved through the real TUI.
use super::*;

#[test]
#[ignore = "requires explicit live-account opt-in and a disposable TUI-approved private account credential"]
fn live_saved_account_refresh_and_revoke() {
    assert_eq!(
        std::env::var("OSMOGREP_LIVE_ACCOUNT_TEST").as_deref(),
        Ok("1")
    );
    let fixture = std::env::var_os("OSMOGREP_LIVE_ACCOUNT_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap();
    assert!(fixture
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("osmogrep-account-live-"));
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.join("credential-marker.json")).unwrap())
            .unwrap();
    assert_eq!(marker["started_signed_out"], true);
    let raw = crate::credentials::get("oauth/osmogrep")
        .unwrap()
        .expect("Disposable TUI credential required");
    assert!(
        crate::file_guard::digest(raw.as_bytes()) == marker["credential_sha256"].as_str().unwrap(),
        "Refuse to modify a credential that does not match the approved disposable test"
    );
    assert_eq!(
        auth::account_endpoint("/api/cli/account").unwrap(),
        "https://app.osmogrep.com/api/cli/account"
    );
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Err(error) = auth::logout("osmogrep") {
                eprintln!("Disposable account cleanup needs attention: {error}");
            }
        }
    }
    let _cleanup = Cleanup;
    let before = load()
        .expect("Live account summary must be readable")
        .expect("TUI must approve this disposable connection first");
    assert!(!before.user.id.is_empty());
    let mut saved: serde_json::Value = serde_json::from_str(
        &crate::credentials::get("oauth/osmogrep")
            .unwrap()
            .expect("TUI must save the credential"),
    )
    .expect("Stored test credential must be valid JSON");
    let old_access = saved["access_token"]
        .as_str()
        .expect("Access token required")
        .to_string();
    let old_refresh = saved["refresh_token"]
        .as_str()
        .expect("Refresh token required")
        .to_string();
    // Expire only the local timestamp in this disposable credential to exercise Rust refresh.
    saved["expires_at"] = serde_json::json!(0);
    crate::credentials::set("oauth/osmogrep", &saved.to_string()).unwrap();
    let after = load()
        .expect("Rust refresh and account lookup must succeed")
        .unwrap();
    assert!(
        before.user.id == after.user.id,
        "Refresh must preserve the approving account"
    );
    let rotated: serde_json::Value =
        serde_json::from_str(&crate::credentials::get("oauth/osmogrep").unwrap().unwrap()).unwrap();
    let new_access = rotated["access_token"].as_str().unwrap().to_string();
    assert!(
        rotated["refresh_token"].as_str().unwrap() != old_refresh,
        "Refresh token must rotate"
    );
    assert!(new_access != old_access, "Access token must rotate");
    assert!(rotated["expires_at"].as_u64().unwrap() > 0);
    let endpoint = auth::account_endpoint("/api/cli/account").unwrap();
    assert!(fetch(&endpoint, &new_access).is_ok());
    auth::logout("osmogrep").expect("Rust logout must revoke and remove this test credential");
    assert!(crate::credentials::get("oauth/osmogrep").unwrap().is_none());
    assert!(load().unwrap().is_none());
    for access in [old_access, new_access] {
        let error = fetch(&endpoint, &access)
            .expect_err("Revocation must invalidate both access generations");
        assert!(error.contains("expired or was revoked"));
    }
    println!("LIVE_ACCOUNT_CHECKS_PASSED: private file credential persistence, account summary, forced client refresh, rotating tokens, logout, both access generations revoked; no profile or secrets exported.");
}
