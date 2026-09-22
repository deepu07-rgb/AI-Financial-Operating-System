fn main() {
    println!("cargo:rerun-if-env-changed=AIFOS_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=AIFOS_UPDATE_PUBKEY");
    tauri_build::build()
}
