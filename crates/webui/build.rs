use cargo_metadata::camino::Utf8PathBuf;
use std::{fs::File, io::Write};
use syntect::{highlighting::ThemeSet, html::ClassStyle};

fn main() {
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap();
    generate_syntect_css(&pkg_name);
}

fn get_css_path(filename: &str, webui_package_name: &str) -> Utf8PathBuf {
    let meta = cargo_metadata::MetadataCommand::new().exec().unwrap();
    let package = meta
        .packages
        .iter()
        .find(|p| p.name.as_str() == webui_package_name)
        .unwrap_or_else(|| panic!("package `{webui_package_name}` must exist"));
    package.manifest_path.parent().unwrap().join(filename)
}

fn generate_syntect_css(webui_package_name: &str) {
    const DEFAULT_THEME: &str = "base16-ocean.dark"; // NB: Sync with syntect_code_block
    let css_path = get_css_path("syntect.css", webui_package_name);
    if !css_path.exists() {
        let mut css_file = File::create(css_path).unwrap();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get(DEFAULT_THEME)
            .expect("DEFAULT_THEME must be found");
        let content = syntect::html::css_for_theme_with_class_style(theme, ClassStyle::Spaced)
            .expect("Failed to generate CSS for theme");
        css_file.write_all(content.as_bytes()).unwrap();
        css_file.flush().unwrap();
    }
}
