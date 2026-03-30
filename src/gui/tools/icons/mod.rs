use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};

static ICON_CACHE: OnceLock<Mutex<HashMap<String, TextureHandle>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, TextureHandle>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rasterize_svg(svg_bytes: &[u8], target_px: u32) -> Option<ColorImage> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_bytes, &opt).ok()?;
    let size = tree.size();
    let int = size.to_int_size();
    let max_side = int.width().max(int.height()).max(1) as f32;
    let scale = (target_px as f32 / max_side).max(1.0 / max_side);

    let w = ((int.width() as f32) * scale).ceil().max(1.0) as u32;
    let h = ((int.height() as f32) * scale).ceil().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    let mut pm = pixmap.as_mut();
    resvg::render(&tree, transform, &mut pm);

    let data = pixmap.data().to_vec();
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &data,
    ))
}

fn apply_tint(image: &mut ColorImage, tint: Color32) {
    if tint == Color32::WHITE {
        return;
    }
    let [tr, tg, tb, ta] = tint.to_array();
    for pixel in image.pixels.iter_mut() {
        let [r, g, b, a] = pixel.to_array();
        let nr = (r as u16 * tr as u16 / 255) as u8;
        let ng = (g as u16 * tg as u16 / 255) as u8;
        let nb = (b as u16 * tb as u16 / 255) as u8;
        let na = (a as u16 * ta as u16 / 255) as u8;
        *pixel = Color32::from_rgba_unmultiplied(nr, ng, nb, na);
    }
}

/// Load a built-in SVG icon, rasterize it to `target_px`, optionally tint it, and cache it.
pub fn load_icon(
    ctx: &Context,
    icon: IconId,
    target_px: u32,
    tint: Option<Color32>,
) -> TextureHandle {
    let key = format!("tools.icon:{:?}:{}:{:?}", icon, target_px, tint);

    if let Some(tex) = cache().lock().unwrap().get(&key).cloned() {
        return tex;
    }

    let bytes = icon_bytes(icon);
    let mut image = rasterize_svg(bytes, target_px)
        .unwrap_or_else(|| panic!("Failed to rasterize icon {:?}", icon));

    if let Some(tint) = tint {
        apply_tint(&mut image, tint);
    }

    let tex = ctx.load_texture(key.clone(), image, TextureOptions::LINEAR);
    cache().lock().unwrap().insert(key, tex.clone());
    tex
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    Edit,
    Path,
    Select,
}

fn icon_bytes(icon: IconId) -> &'static [u8] {
    match icon {
        IconId::Edit => include_bytes!("edit_icon.svg"),
        IconId::Path => include_bytes!("path_icon.svg"),
        IconId::Select => include_bytes!("select_icon.svg"),
    }
}
