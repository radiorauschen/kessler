//! Compiles the small Objective-C file that talks to macOS, and links it.
//!
//! Everything here is optional by design. On any other system it does nothing;
//! on a Mac without a working compiler it says so and does nothing. The cfg it
//! sets — `kessler_media` — is what the Rust side checks, so a failure here
//! costs the media keys and the Dock menu and not the build.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    stamp_the_build();

    println!("cargo:rustc-check-cfg=cfg(kessler_media)");
    println!("cargo:rustc-check-cfg=cfg(kessler_tray)");
    println!("cargo:rustc-check-cfg=cfg(kessler_touchbar)");
    println!("cargo:rustc-check-cfg=cfg(kessler_route)");
    println!("cargo:rerun-if-changed=src/media/statusbar.m");
    println!("cargo:rerun-if-changed=src/media/touchbar.m");
    println!("cargo:rerun-if-changed=src/media/route.c");

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
    touch_bar(&compiler, arch, &out);
    route(&compiler, arch, &out);
}

/// Where the sound goes (`src/media/route.c`): a pause when it would go on
/// out of the speakers once the headphones are gone. Built on its own like
/// the Touch Bar: a file that will not compile costs this and nothing else.
fn route(compiler: &str, arch: &str, out: &std::path::Path) {
    let source = "src/media/route.c";
    let object = out.join("kessler_route.o");
    let archive = out.join("libkesslerroute.a");
    let mut command = Command::new(compiler);
    if !arch.is_empty() {
        command.args(["-arch", arch]);
    }
    let compiled = command.args(["-c", "-O2", "-Wall", "-o"]).arg(&object).arg(source).status();
    if !matches!(compiled, Ok(status) if status.success()) {
        println!("cargo:warning=kessler: {source} liess sich nicht übersetzen; ohne kopfhörer wird nicht pausiert");
        return;
    }
    let archived = Command::new("ar").arg("crs").arg(&archive).arg(&object).status();
    if !matches!(archived, Ok(status) if status.success()) {
        println!("cargo:warning=kessler: ar konnte {} nicht bauen; ohne kopfhörer wird nicht pausiert", archive.display());
        return;
    }
    println!("cargo:rustc-link-lib=static=kesslerroute");
    println!("cargo:rustc-link-lib=framework=CoreAudio");
    println!("cargo:rustc-cfg=kessler_route");
}

/// The Touch Bar (`src/media/touchbar.m`), built on its own as the menu bar's
/// symbol is: a file that will not compile costs the Touch Bar and nothing
/// else.
fn touch_bar(compiler: &str, arch: &str, out: &std::path::Path) {
    let source = "src/media/touchbar.m";
    let object = out.join("kessler_touchbar.o");
    let archive = out.join("libkesslertouchbar.a");
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
        println!("cargo:warning=kessler: {source} liess sich nicht übersetzen; die touch bar bleibt leer");
        return;
    }
    let archived = Command::new("ar").arg("crs").arg(&archive).arg(&object).status();
    if !matches!(archived, Ok(status) if status.success()) {
        println!("cargo:warning=kessler: ar konnte {} nicht bauen; die touch bar bleibt leer", archive.display());
        return;
    }
    println!("cargo:rustc-link-lib=static=kesslertouchbar");
    println!("cargo:rustc-cfg=kessler_touchbar");
}

/// The symbol in the menu bar (`src/media/statusbar.m`), built on its own:
/// a file that will not compile costs the symbol and nothing else — not the
/// media keys, which are built above and work without it.
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

/// Put the commit this build came from into the binary.
///
/// The version number alone cannot answer "am I running what I just built?" —
/// and that question has already cost an afternoon here, when cargo reported
/// "Finished" in 0.4 seconds and the new code was not in the binary. A short
/// commit hash next to the version answers it at a glance, and a `+` says the
/// working copy had uncommitted changes when it was built.
///
/// Outside a git checkout — a downloaded source tarball, say — there is simply
/// nothing to stamp, and the version stands on its own.
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
