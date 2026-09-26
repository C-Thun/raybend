fn main() {
    for key in [
        "RAYBEND_CHANNEL",
        "RAYBEND_UPDATER_PUBLIC_KEY",
        "RAYBEND_DISTRIBUTION",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    tauri_build::build()
}
