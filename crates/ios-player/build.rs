// Native commands are called only from Rust through `run_mobile_plugin`, so they deliberately
// have no webview permissions and do not belong in the generated command ACL.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
