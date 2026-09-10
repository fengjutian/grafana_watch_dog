use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use std::{fs, fs::File, path::Path};

fn ensure_windows_icon() {
    let path = Path::new("icons/icon.ico");
    if path.exists() {
        return;
    }
    fs::create_dir_all("icons").expect("create icons directory");
    let size = 64;
    let mut rgba = vec![0_u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let dx = x as f32 - 31.5;
            let dy = y as f32 - 31.5;
            let inside = dx * dx + dy * dy < 30.0 * 30.0;
            rgba[i..i + 4].copy_from_slice(if inside {
                &[53, 111, 96, 255]
            } else {
                &[0, 0, 0, 0]
            });
            if inside && ((x > 28 && x < 35) || (y > 28 && y < 35)) {
                rgba[i..i + 4].copy_from_slice(&[225, 241, 234, 255]);
            }
        }
    }
    let mut dir = IconDir::new(ResourceType::Icon);
    dir.add_entry(
        IconDirEntry::encode(&IconImage::from_rgba_data(size as u32, size as u32, rgba))
            .expect("encode icon"),
    );
    dir.write(File::create(path).expect("create icon"))
        .expect("write icon");
}

fn main() {
    ensure_windows_icon();
    tauri_build::build()
}
