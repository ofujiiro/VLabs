// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing files so a crash can never leave a half-written one behind.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `path` atomically: first to a temporary file in the same
/// folder, flushed to disk, then renamed over the target. Readers see either
/// the old file or the complete new one, never a mixture.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let folder = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?;
    let temporary = folder.join(format!(
        ".{}.{}.partial",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::write_atomically;

    #[test]
    fn replaces_a_file_and_leaves_no_temporary_behind() {
        let folder =
            std::env::temp_dir().join(format!("daw-formats-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("song.bin");
        write_atomically(&path, b"first").unwrap();
        write_atomically(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let leftovers: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != "song.bin")
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files were left behind: {leftovers:?}"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_untouched() {
        let folder =
            std::env::temp_dir().join(format!("daw-formats-atomic-fail-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("song.bin");
        write_atomically(&path, b"good").unwrap();
        // A folder in the way of the temporary file makes the write fail.
        let blocker = folder.join(format!(".song.bin.{}.partial", std::process::id()));
        std::fs::create_dir_all(&blocker).unwrap();
        assert!(write_atomically(&path, b"bad").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"good");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
