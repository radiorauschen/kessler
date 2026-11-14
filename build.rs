use std::path::PathBuf;
use std::process::Command;

fn main() {
    stamp_the_build();

    println!("cargo:rustc-check-cfg=cfg(kessler_media)");
    println!("cargo:rustc-check-cfg=cfg(kessler_tray)");
    println!("cargo:rerun-if-changed=src/media/statusbar.m");

    let source = "src/media/macos.m";
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let out: PathBuf = match std::env::var("OUT_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };
    let object = out.join("kessler_media.o");
    let archive = out.join("libkesslermedia.a");

    let compiler = std::env::var("CC").unwrap_or_else(|_| "clang".to_string());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => "",
    };
    let mut command = Command::new(&compiler);
    if !arch.is_empty() {
        command.args(["-arch", arch]);
    }
    let compiled = command
        .args(["-c", "-fobjc-arc", "-O2", "-Wall", "-o"])
        .arg(&object)
        .arg(source)
        .status();

    match compiled {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=kessler: {compiler} konnte {source} nicht übersetzen ({status}); \
                 medientasten und dock-menü bleiben aus"
            );
            return;
        }
        Err(e) => {
            println!(
                "cargo:warning=kessler: {compiler} lässt sich nicht starten ({e}); \
                 medientasten und dock-menü bleiben aus"
            );
            return;
        }
    }

    let archived = Command::new("ar")
        .arg("crs")
        .arg(&archive)
        .arg(&object)
        .status();
    match archived {
        Ok(status) if status.success() => {}
        _ => {
            println!(
                "cargo:warning=kessler: ar konnte {} nicht bauen; \
                 medientasten und dock-menü bleiben aus",
                archive.display()
            );
            return;
        }
    }

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=kesslermedia");
    println!("cargo:rustc-link-lib=framework=MediaPlayer");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-cfg=kessler_media");

    menu_bar(&compiler, arch, &out);
}

fn menu_bar(compiler: &str, arch: &str, out: &std::path::Path) {
    let source = "src/media/statusbar.m";
    let object = out.join("kessler_tray.o");
    let archive = out.join("libkesslertray.a");
    let mut command = Command::new(compiler);
    if !arch.is_empty() {
        command.args(["-arch", arch]);
    }
    let compiled = command
        .args(["-c", "-fobjc-arc", "-O2", "-Wall", "-o"])
        .arg(&object)
        .arg(source)
        .status();
    if !matches!(compiled, Ok(status) if status.success()) {
        println!("cargo:warning=kessler: {source} liess sich nicht übersetzen; das symbol in der menüleiste bleibt aus");
        return;
    }
    let archived = Command::new("ar").arg("crs").arg(&archive).arg(&object).status();
    if !matches!(archived, Ok(status) if status.success()) {
        println!("cargo:warning=kessler: ar konnte {} nicht bauen; das symbol in der menüleiste bleibt aus", archive.display());
        return;
    }
    println!("cargo:rustc-link-lib=static=kesslertray");
    println!("cargo:rustc-cfg=kessler_tray");
}

fn stamp_the_build() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let hash = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let Some(hash) = hash else {
        println!("cargo:rustc-env=KESSLER_COMMIT=");
        return;
    };

    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    println!(
        "cargo:rustc-env=KESSLER_COMMIT={hash}{}",
        if dirty { "+" } else { "" }
    );
}
