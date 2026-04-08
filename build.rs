use std::env;
use std::path::PathBuf;

use image::imageops::FilterType;

fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/CopyPasteIcon.png");

    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();

    if cfg!(target_os = "windows") {
        let generated_icon = generate_windows_icon();
        let mut res = winresource::WindowsResource::new();
        let icon_path = generated_icon.to_string_lossy();
        res.set_icon(icon_path.as_ref());
        res.set_icon_with_id(icon_path.as_ref(), "32512");
        res.compile().unwrap();
    }
}

fn generate_windows_icon() -> PathBuf {
    let source = PathBuf::from("assets/CopyPasteIcon.png");
    let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("app-icon.ico");
    let image = image::open(&source).expect("failed to load assets/CopyPasteIcon.png");
    let square = crop_center_square(image);
    let resized = square.resize_exact(256, 256, FilterType::Lanczos3);
    resized
        .save_with_format(&output, image::ImageFormat::Ico)
        .expect("failed to write generated app icon");
    output
}

fn crop_center_square(image: image::DynamicImage) -> image::DynamicImage {
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let side = width.min(height);
    let offset_x = (width - side) / 2;
    let offset_y = (height - side) / 2;
    image::DynamicImage::ImageRgba8(
        image::imageops::crop_imm(&rgba, offset_x, offset_y, side, side).to_image(),
    )
}
