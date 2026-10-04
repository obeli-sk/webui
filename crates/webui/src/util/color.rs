use std::hash::DefaultHasher;
use std::hash::Hash as _;
use std::hash::Hasher as _;

pub fn generate_color(s: &str) -> String {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();
    generate_color_from_hash(hash)
}

/// Returns a CSS `light-dark(<light>, <dark>)` color, resolved by the theme's `color-scheme`.
pub fn generate_color_from_hash(hash: u64) -> String {
    // Hue: 20-340, skipping the ±20° band around red (0/360) reserved for errors
    let h = 20 + (hash % 321);
    // Saturation: 70-100% (Vibrant)
    let s = 70 + ((hash >> 16) % 31);
    // Lightness: 65-85% (Readable on dark background)
    let l = 65 + ((hash >> 32) % 21);
    // Light theme uses OKLCH so every hue keeps readable contrast on white; +20° keeps red (~29°) out.
    let light_h = (h + 20) % 360;
    let light_c = 0.10 + ((hash >> 16) % 5) as f64 * 0.01;
    let light_l = 42 + ((hash >> 32) % 9);

    format!("light-dark(oklch({light_l}% {light_c:.2} {light_h}), hsl({h}, {s}%, {l}%))")
}
