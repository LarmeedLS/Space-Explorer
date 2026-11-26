use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Always watch build.rs itself so Cargo does not fall back to watching the
    // entire workspace (which would trigger recompiles whenever runtime assets
    // like assets/icons/icon.png change).
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let icons_dir = manifest_dir.join("assets").join("icons");
    let icon_ico = icons_dir.join("icon.ico");
    let icon_rc = icons_dir.join("icon.rc");

    // Only register rerun-if-changed for paths that actually exist; Cargo treats
    // non-existent rerun-if-changed paths as permanently dirty and recompiles
    // on every run.
    if icon_ico.exists() {
        println!("cargo:rerun-if-changed=assets/icons/icon.ico");
    }
    if icon_rc.exists() {
        println!("cargo:rerun-if-changed=assets/icons/icon.rc");
    }
    if !icon_ico.exists() || !icon_rc.exists() {
        if icons_dir.exists() {
            println!("cargo:rerun-if-changed=assets/icons");
        }
        println!(
            "cargo:warning=Skipping Windows .exe icon embedding (missing assets/icons/icon.ico or icon.rc)"
        );
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env == "gnu" {
        // MinGW cross-compilation (Linux -> Windows in GitHub Actions / tools/package.sh) or native MinGW
        let res_obj = out_dir.join("icon.o");
        let windres_candidates = ["x86_64-w64-mingw32-windres", "windres"];
        for tool in windres_candidates {
            let status = Command::new(tool)
                .args([
                    "-I",
                    icon_ico.parent().unwrap().to_str().unwrap(),
                    "-i",
                    icon_rc.to_str().unwrap(),
                    "-O",
                    "coff",
                    "-o",
                    res_obj.to_str().unwrap(),
                ])
                .status();
            if let Ok(s) = status {
                if s.success() {
                    println!("cargo:rustc-link-arg={}", res_obj.display());
                    return;
                }
            }
        }
        println!("cargo:warning=windres not found; Windows .exe will not have an embedded Explorer icon");
    } else if target_env == "msvc" {
        // Native Windows MSVC build
        let res_file = out_dir.join("icon.res");
        let rc_candidates = ["rc.exe", "llvm-rc"];
        for tool in rc_candidates {
            let status = Command::new(tool)
                .args([
                    "/I",
                    icon_ico.parent().unwrap().to_str().unwrap(),
                    "/fo",
                    res_file.to_str().unwrap(),
                    icon_rc.to_str().unwrap(),
                ])
                .status();
            if let Ok(s) = status {
                if s.success() {
                    println!("cargo:rustc-link-arg={}", res_file.display());
                    return;
                }
            }
        }
        println!("cargo:warning=rc.exe / llvm-rc not found; Windows .exe will not have an embedded Explorer icon");
    }
}
