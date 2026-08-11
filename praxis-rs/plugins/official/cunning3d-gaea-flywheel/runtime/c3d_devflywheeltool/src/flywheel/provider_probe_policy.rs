fn parity_probe_provider(bin: &str) -> Option<&'static str> {
    let valid_name = bin
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if !valid_name {
        return None;
    }
    if bin.starts_with("gaea_") {
        return Some("gaea");
    }
    matches!(bin, "polybevel_blender_cube_compare").then_some("blender")
}
