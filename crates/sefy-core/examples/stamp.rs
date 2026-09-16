//! Sets a sync stamp on a vault, for checking how an older binary treats it.
//!
//! Not a product feature: `sefy sync` is what stamps a vault in normal use.
//! This exists so the round trip "new build stamps, published build writes,
//! new build reads" can be run by hand against a real older binary, which is
//! the only way to see what that build does with a table it has never heard
//! of.
//!
//! Usage: cargo run -p sefy-core --example stamp -- <vault> <password>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (path, password) = (&args[1], &args[2]);

    let mut vault = sefy_core::Vault::open(path, password.as_bytes()).expect("opens");
    vault.record_sync("file", "push").expect("records");
    vault.save().expect("saves");

    println!("{:?}", vault.last_sync().expect("reads back"));
}
