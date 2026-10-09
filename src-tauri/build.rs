fn main() {
    // An app permission manifest: every whatRust command gets `allow-<command>` /
    // `deny-<command>` permissions, and the files in `capabilities/` grant each
    // window exactly the commands it uses. Without it no capability could grant
    // a command to the remote WhatsApp page, and Tauri rejects every custom
    // command from a remote origin — which silently killed notifications and
    // the unread badge (issues #3, #32). See src/ipc_acl_tests.rs.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            // WhatsApp page (bridge.js)
            "notify",
            "set_unread",
            "dlog",
            "open_download",
            "reveal_download",
            // settings window
            "get_settings",
            "set_settings",
            "open_settings",
            "list_accounts",
            "add_account",
            "remove_account",
            "rename_account",
            "open_account",
            "set_app_lock_password",
            "change_app_lock_password",
            "disable_app_lock",
            "set_app_lock_options",
            "set_biometric_enabled",
            "lock_app",
            // settings window and lock screen
            "get_lock_status",
            // lock screen
            "unlock_password",
            "unlock_biometric",
            "reset_app_lock",
        ]),
    ))
    .expect("failed to run tauri-build");
}
