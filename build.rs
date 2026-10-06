// Re-embed the frontend once web/dist appears or changes: rust-embed alone
// never notices a folder that was missing at the previous build.
fn main() {
    println!("cargo:rerun-if-changed=web/dist");
}
