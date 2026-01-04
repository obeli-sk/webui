use std::hash::DefaultHasher;
use std::hash::Hash as _;
use std::hash::Hasher as _;

pub fn generate_color(s: &str) -> String {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let hash = hasher.finish();
    generate_color_from_hash(hash)
}

pub fn generate_color_from_hash(hash: u64) -> String {
    // Hue: 0-360
    let h = hash % 360;
    // Saturation: 70-100% (Vibrant)
    let s = 70 + ((hash >> 16) % 31);
    // Lightness: 65-85% (Readable on dark background)
    let l = 65 + ((hash >> 32) % 21);

    format!("hsl({}, {}%, {}%)", h, s, l)
}
