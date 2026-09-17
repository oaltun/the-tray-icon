use crate::app::status::Status;
use crate::app::appdata::AppData;

pub fn load_icon(appdata: &AppData, filename: &str) -> crate::Icon {
    let path = appdata.icon_path(filename);
    if path.exists() {
        if let Ok(svg_data) = std::fs::read(&path) {
            if let Ok(icon) = svg_to_icon(&svg_data) {
                return icon;
            }
        }
    }
    fallback_icon_for(Status::Ok)
}

pub fn fallback_icon_for(status: Status) -> crate::Icon {
    let (r, g, b) = match status {
        Status::Ok => (0x00, 0xff, 0x00),
        Status::Warn => (0xff, 0xa5, 0x00),
        Status::Err => (0xff, 0x00, 0x00),
    };

    let size = 32;
    let mut img = image::RgbaImage::new(size, size);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - size as f32 / 2.0;
            let dy = y as f32 - size as f32 / 2.0;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= size as f32 / 2.0 {
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            } else {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }

    let rgba = img.into_raw();
    crate::Icon::from_rgba(rgba, size, size).expect("failed to create fallback icon")
}

pub fn aggregate_status(statuses: &std::collections::HashMap<String, Status>) -> Status {
    if statuses.values().any(|s| *s == Status::Err) {
        Status::Err
    } else if statuses.values().any(|s| *s == Status::Warn) {
        Status::Warn
    } else {
        Status::Ok
    }
}

fn svg_to_icon(svg_data: &[u8]) -> anyhow::Result<crate::Icon> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg_data, &options)?;
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap"))?;

    let mut pixmap_mut = resvg::tiny_skia::PixmapMut::from_bytes(pixmap.data_mut(), width, height)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap mut"))?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap_mut,
    );

    let rgba = pixmap.take();
    Ok(crate::Icon::from_rgba(rgba, width, height)?)
}
