use std::fs::File;
use std::path::{Path, PathBuf};

const COMMANDS: &[&str] = &[
    "workboard_handshake",
    "workboard_query",
    "workboard_execute",
    "workboard_subscribe",
];

fn main() {
    let icon_path = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"))
        .join("workboard.ico");
    write_icon(&icon_path);
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS))
        .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(icon_path));
    tauri_build::try_build(attributes).expect("Tauri build configuration");
}

fn write_icon(path: &Path) {
    let mut directory = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16, 32, 64, 256] {
        let mut pixels = vec![0_u8; size * size * 4];
        for y in 0..size {
            for x in 0..size {
                let offset = (y * size + x) * 4;
                let inset = size / 8;
                let mark = y > size / 4
                    && y < size * 3 / 4
                    && ((x > size / 4 && x < size * 3 / 8)
                        || (x > size * 5 / 8 && x < size * 3 / 4)
                        || (y > size * 5 / 8 && x > size * 3 / 8 && x < size * 5 / 8));
                let color = if mark {
                    [243, 245, 248, 255]
                } else if x >= inset && x < size - inset && y >= inset && y < size - inset {
                    [49, 95, 189, 255]
                } else {
                    [11, 13, 18, 255]
                };
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
        let image = ico::IconImage::from_rgba_data(size as u32, size as u32, pixels);
        directory.add_entry(ico::IconDirEntry::encode(&image).expect("encode Workboard icon"));
    }
    directory
        .write(File::create(path).expect("create Workboard icon"))
        .expect("write Workboard icon");
}
