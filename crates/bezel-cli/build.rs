fn main() -> std::io::Result<()> {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=windows.manifest");
        tauri_winres::WindowsResource::new()
            .set_icon("../../apps/bezel-studio/src-tauri/icons/icon.ico")
            .set_manifest(include_str!("windows.manifest"))
            .compile()?;
    }
    Ok(())
}
