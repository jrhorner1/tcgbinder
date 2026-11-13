// Compiles the Objective-C Vision reader on Apple platforms; elsewhere there is nothing
// to build and the crate reports that reading isn't available.
fn main() {
    println!("cargo:rerun-if-changed=native/read.m");
    println!("cargo:rerun-if-changed=native/read.h");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "macos" && os != "ios" {
        return;
    }
    cc::Build::new().file("native/read.m").flag("-fobjc-arc").flag("-fmodules").compile("tcgbvision");
    for framework in ["Foundation", "CoreImage", "ImageIO", "Vision", "CoreGraphics"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
