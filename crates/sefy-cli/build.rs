// Embeds the Windows executable icon. The icon is the lacodda line mark
// exported to a multi-size .ico, one level per size (S for 16/24, M for 32/48,
// L for 64 and up), so Explorer picks the right drawing for each view.
//
// Without this the mark was drawn, exported and committed, and then shown
// nowhere: a console application with no resource gets the generic Windows
// executable icon in Explorer, on a pinned shortcut and in the properties
// dialog. The file existed since 0.1.0 and nothing read it.
//
// The .ico lives inside this crate rather than in the repository's assets/,
// because `cargo install sefy` is a documented install path and cargo packages
// only what sits under the crate directory: an icon one level up would build
// on this machine and vanish from the crates.io build, which is the kind of
// difference nobody notices until a user reports a blank icon. The SVG masters
// it is rasterized from stay in assets/ - they serve GitHub and the docs site,
// and the exporter writes the .ico here.
fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("failed to embed the Windows resources");
    }
}
