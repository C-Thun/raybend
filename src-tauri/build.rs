fn main() {
    for key in [
        "RAYBEND_CHANNEL",
        "RAYBEND_PHOTO_AI",
        "TAURI_CONFIG",
        "RAYBEND_UPDATER_PUBLIC_KEY",
        "RAYBEND_DISTRIBUTION",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    tauri_build::build()
}
