// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask installer` and `cargo xtask appimage` (decision 0022): the
//! Windows installer (NSIS) and the Linux AppImage, made from `dist/`.
//! The templates in `packaging/` name the product only through
//! placeholders filled in here from `daw-ident`, so a rename stays a
//! one-line change (decision 0002).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The placeholders of the templates, filled in.
fn fill(template: &str, extra: &[(&str, String)]) -> String {
    let mut text = template
        .replace("@PRODUCT@", daw_ident::PRODUCT_NAME)
        .replace("@EXE@", daw_ident::EXECUTABLE_NAME)
        .replace("@EXT@", daw_ident::PROJECT_FILE_EXTENSION)
        .replace("@APPID@", daw_ident::APPLICATION_ID)
        .replace("@URL@", daw_ident::PROJECT_URL)
        .replace("@VERSION@", env!("CARGO_PKG_VERSION"));
    for (key, value) in extra {
        text = text.replace(key, value);
    }
    text
}

fn describe(path: &Path, error: &std::io::Error) -> String {
    format!("{}: {error}", path.display())
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| describe(path, &e))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| describe(parent, &e))?;
    }
    fs::write(path, text).map_err(|e| describe(path, &e))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| describe(parent, &e))?;
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| describe(from, &e))
}

/// A program to run: `variable` if set, else `name` (found on the PATH).
fn tool(variable: &str, name: &str) -> PathBuf {
    std::env::var_os(variable).map_or_else(|| PathBuf::from(name), PathBuf::from)
}

/// Makes the Windows installer from `dist` (which holds the Windows
/// program): writes the NSIS script beside it and runs `makensis`
/// (`MAKENSIS` names it if it is not on the PATH). Returns the installer.
pub fn installer(root: &Path, dist: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let program = dist.join(format!("{}.exe", daw_ident::EXECUTABLE_NAME));
    if !program.is_file() {
        return Err(format!(
            "{} is missing: the installer is made from a Windows build (cargo xtask dist on Windows)",
            program.display()
        ));
    }
    fs::create_dir_all(out_dir).map_err(|e| describe(out_dir, &e))?;
    let out = out_dir.join(format!(
        "{}-{}-setup.exe",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION")
    ));
    let slashes = |p: &Path| p.display().to_string().replace('\\', "/");
    let script = fill(
        &read(&root.join("packaging/windows/installer.nsi.in"))?,
        &[("@DIST@", slashes(dist)), ("@OUT@", slashes(&out))],
    );
    let script_path = out_dir.join("installer.nsi");
    write(&script_path, &script)?;
    let status = Command::new(tool("MAKENSIS", "makensis"))
        .arg("-V2")
        .arg(&script_path)
        .status()
        .map_err(|e| format!("could not run makensis (install NSIS, or set MAKENSIS): {e}"))?;
    if !status.success() {
        return Err("makensis failed".to_owned());
    }
    Ok(out)
}

/// Makes the Linux AppImage from `dist` (which holds the Linux program):
/// assembles the AppDir in `work` and runs `appimagetool` (`APPIMAGETOOL`
/// names it if it is not on the PATH). Returns the AppImage.
pub fn appimage(root: &Path, dist: &Path, work: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let exe = daw_ident::EXECUTABLE_NAME;
    let id = daw_ident::APPLICATION_ID;
    let program = dist.join(exe);
    if !program.is_file() {
        return Err(format!(
            "{} is missing: the AppImage is made from a Linux build (cargo xtask dist)",
            program.display()
        ));
    }
    let app = work.join("AppDir");
    if app.exists() {
        fs::remove_dir_all(&app).map_err(|e| describe(&app, &e))?;
    }
    let usr = app.join("usr");
    copy(&program, &usr.join("bin").join(exe))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let target = usr.join("bin").join(exe);
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .map_err(|e| describe(&target, &e))?;
    }
    let packaging = root.join("packaging/linux");
    let desktop = fill(&read(&packaging.join("app.desktop.in"))?, &[]);
    write(&app.join(format!("{id}.desktop")), &desktop)?;
    write(
        &usr.join(format!("share/applications/{id}.desktop")),
        &desktop,
    )?;
    write(
        &usr.join(format!("share/metainfo/{id}.metainfo.xml")),
        &fill(&read(&packaging.join("metainfo.xml.in"))?, &[]),
    )?;
    write(
        &usr.join(format!("share/mime/packages/{id}.xml")),
        &fill(&read(&packaging.join("mime.xml.in"))?, &[]),
    )?;
    let icons = dist.join("icons");
    copy(&icons.join("icon-256.png"), &app.join(format!("{id}.png")))?;
    copy(&icons.join("icon-256.png"), &app.join(".DirIcon"))?;
    for size in [16, 24, 32, 48, 64, 128, 256] {
        copy(
            &icons.join(format!("icon-{size}.png")),
            &usr.join(format!("share/icons/hicolor/{size}x{size}/apps/{id}.png")),
        )?;
    }
    let docs = usr.join("share/doc").join(exe);
    for file in [
        "LICENSE",
        "THIRD_PARTY.md",
        "THIRD_PARTY_NOTICES.txt",
        "user-guide.html",
    ] {
        copy(&dist.join(file), &docs.join(file))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(format!("usr/bin/{exe}"), app.join("AppRun"))
        .map_err(|e| describe(&app, &e))?;

    fs::create_dir_all(out_dir).map_err(|e| describe(out_dir, &e))?;
    let out = out_dir.join(format!(
        "{}-{}-x86_64.AppImage",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION")
    ));
    let mut command = Command::new(tool("APPIMAGETOOL", "appimagetool"));
    command
        .env("ARCH", "x86_64")
        // Runs without FUSE (containers, CI).
        .env("APPIMAGE_EXTRACT_AND_RUN", "1");
    if let Some(runtime) = std::env::var_os("APPIMAGE_RUNTIME") {
        command.arg("--runtime-file").arg(runtime);
    }
    let status = command
        .arg(&app)
        .arg(&out)
        .status()
        .map_err(|e| format!("could not run appimagetool (or set APPIMAGETOOL): {e}"))?;
    if !status.success() {
        return Err("appimagetool failed".to_owned());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_name_the_product_only_through_placeholders() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for template in [
            "packaging/windows/installer.nsi.in",
            "packaging/linux/app.desktop.in",
            "packaging/linux/metainfo.xml.in",
            "packaging/linux/mime.xml.in",
        ] {
            let text = read(&root.join(template)).unwrap();
            for name in [
                daw_ident::PRODUCT_NAME,
                daw_ident::APPLICATION_ID,
                daw_ident::PROJECT_FILE_EXTENSION,
            ] {
                assert!(!text.contains(name), "{template} spells out {name}");
            }
            let filled = fill(
                &text,
                &[("@DIST@", "d".to_owned()), ("@OUT@", "o".to_owned())],
            );
            assert!(
                !filled.contains('@'),
                "{template} has an unknown placeholder"
            );
        }
    }
}
