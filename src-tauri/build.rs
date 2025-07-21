fn main() {
    #[allow(unused_mut)]
    let mut attributes = tauri_build::Attributes::new();

    #[cfg(windows)]
    {
        let opencv_dll = std::env::current_dir()
            .unwrap()
            .join("opencv_world4110.dll");
        if !std::fs::exists(&opencv_dll).is_ok_and(|f| f) {
            println!("opencv_world4110.dll not found in src-tauri/, searching PATH...");
            match which::which_all("opencv_world4110.dll")
                .ok()
                .and_then(|mut iter| iter.next())
            {
                Some(path) => {
                    println!("found opencv_world4110.dll in {}.", path.display());
                    std::fs::copy(path, opencv_dll).expect("should be able to copy");
                }
                None => {
                    panic!("opencv_world4110.dll not found in either `src-tauri` or PATH. Provide one in `src-tauri` or add a path that contains it to PATH.");
                }
            }
        }

        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }

    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    #[cfg(windows)]
    {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS");
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV");
        if Ok("windows") == target_os.as_deref() && Ok("msvc") == target_env.as_deref() {
            add_manifest();
        }
    }
}

#[cfg(windows)]
fn add_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let mut manifest = std::env::current_dir().unwrap();
    manifest.push(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", WINDOWS_MANIFEST_FILE);
    // Embed the Windows application manifest file.
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.to_str().unwrap()
    );
    // Turn linker warnings into errors.
    println!("cargo:rustc-link-arg=/WX");
}
