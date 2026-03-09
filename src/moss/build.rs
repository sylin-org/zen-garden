fn main() {
    garden_build_utils::capture_build_number();

    // Embed Windows resources (icon) on Windows builds
    #[cfg(target_os = "windows")]
    {
        let _ = embed_resource::compile("res/zen-garden.rc", embed_resource::NONE);
    }
}
