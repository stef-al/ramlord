//! Puts the app icon and a version block into the Windows exe (SPEC.md,
//! M6). The core draws the icon and writes the resource bytes; this script
//! only hands them to the linker: a `.res` file to MSVC's `link.exe`, which
//! converts it itself, and a COFF object with a `.rsrc` section to MinGW's
//! `ld`, the same thing `windres` makes. No resource compiler is needed.

use std::path::PathBuf;

use ramlord_core::render_app_icon;
use ramlord_core::resources::{self, IconImage, Version};
use ramlord_core::scene::APP_ICON_SIZES;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    if var("CARGO_CFG_TARGET_OS") != "windows" {
        return;
    }
    let number = |name: &str| var(name).parse::<u16>().expect("a version number");
    let version = Version::ramlord(
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH"),
    );
    // DIBs up to 64 pixels; a PNG at 256, where a DIB would take 260 KB.
    let images: Vec<IconImage> = APP_ICON_SIZES
        .iter()
        .map(|&size| {
            let frame = render_app_icon(size).expect("an icon size");
            if size < 256 {
                return IconImage::Pixels(frame);
            }
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, size, size);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut w| w.write_image_data(&resources::rgba(&frame)))
                .expect("the 256 pixel icon as a PNG");
            IconImage::Png { size, bytes }
        })
        .collect();
    let all = resources::resources(&images, &version);
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let (path, bytes) = match (
        var("CARGO_CFG_TARGET_ENV").as_str(),
        var("CARGO_CFG_TARGET_ARCH").as_str(),
    ) {
        ("msvc", _) => (out.join("ramlord.res"), resources::res_file(&all)),
        ("gnu", "x86_64") => (
            out.join("ramlord-resources.o"),
            resources::coff_object(&all),
        ),
        (env, arch) => {
            println!("cargo:warning=no icon for {arch} {env}: the exe gets Windows' default");
            return;
        }
    };
    std::fs::write(&path, bytes).expect("the resource file");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}
