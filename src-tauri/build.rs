#[cfg(windows)]
use std::path::PathBuf;

fn main() {
    #[allow(unused_mut)]
    let mut attributes = tauri_build::Attributes::new();

    #[cfg(windows)]
    {
        let dlls = [
            "opencv_world4110.dll",
            "libleptonica-6.dll",
            "libtesseract-5.5.dll",
        ];
        let src_dirs_env = [
            "OPENCV_DLL_PATH",
            "LEPTONICA_DLL_PATH",
            "TESSERACT_DLL_PATH",
        ];

        for (env, dll) in src_dirs_env.into_iter().zip(dlls) {
            let dll_path = std::env::current_dir().unwrap().join(dll);
            if !std::fs::exists(&dll_path).is_ok_and(|f| f) {
                println!("{dll} not found in src-tauri/, searching in {env}...");
                let dir = PathBuf::from(std::env::var(env).expect("can resolve path")).join(dll);
                if dir.exists() {
                    println!("found {dll} in {}.", dir.display());
                    std::fs::copy(dir, dll_path).expect("should be able to copy");
                } else {
                    println!(
                        "{dll} not found in {}/, searching in PATH...",
                        dir.display()
                    );
                    match which::which_all(dll).ok().and_then(|mut iter| iter.next()) {
                        Some(path) => {
                            println!("found {dll} in {}.", path.display());
                            std::fs::copy(path, dll_path).expect("should be able to copy");
                        }
                        None => {
                            panic!("{dll} not found in any of `src-tauri` or {env} or PATH. Provide one in `src-tauri` or add a path that contains it to {env}/PATH.");
                        }
                    }
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
