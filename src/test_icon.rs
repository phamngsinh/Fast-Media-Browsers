fn main() {
    let path = std::path::Path::new("C:\\");
    let icon = file_icon_provider::get_file_icon(path, file_icon_provider::IconSize::Large);
}
