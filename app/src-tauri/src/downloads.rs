use std::{
    io,
    path::{Path, PathBuf},
};

pub fn trusted_backup_url(url: &tauri::Url) -> bool {
    [
        "blob:tauri://localhost/",
        "blob:http://tauri.localhost/",
        "blob:https://tauri.localhost/",
    ]
    .iter()
    .any(|prefix| url.as_str().starts_with(prefix))
}

pub fn destination(directory: &Path) -> io::Result<PathBuf> {
    if !directory.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Downloads must be absolute",
        ));
    }
    std::fs::create_dir_all(directory)?;
    for index in 0..10_000 {
        let name = if index == 0 {
            "momo-backup.zip".into()
        } else {
            format!("momo-backup ({index}).zip")
        };
        let candidate = directory.join(name);
        // lstat also treats dangling symlinks as occupied; WebKit's default
        // allow-overwrite=false protects against a later filesystem race.
        match candidate.symlink_metadata() {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Too many existing backup files",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_blobs_from_the_packaged_application_origin() {
        for value in [
            "blob:tauri://localhost/123",
            "blob:http://tauri.localhost/123",
            "blob:https://tauri.localhost/123",
        ] {
            assert!(trusted_backup_url(&value.parse().unwrap()));
        }
        for value in [
            "https://example.com/archive.zip",
            "blob:https://example.com/123",
            "blob:tauri://localhost.evil/123",
            "file:///tmp/archive.zip",
        ] {
            assert!(!trusted_backup_url(&value.parse().unwrap()));
        }
    }
    #[test]
    fn uses_downloads_and_keeps_existing_backups_and_symlinks_untouched() {
        let directory = std::env::temp_dir().join(format!(
            "momo downloads 中文 {}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("momo-backup.zip"), b"existing backup").unwrap();
        let next = destination(&directory).unwrap();
        assert_eq!(next, directory.join("momo-backup (1).zip"));
        assert_eq!(
            std::fs::read(directory.join("momo-backup.zip")).unwrap(),
            b"existing backup"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.join("missing"), &next).unwrap();
            assert_eq!(
                destination(&directory).unwrap(),
                directory.join("momo-backup (2).zip")
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
        assert!(destination(Path::new("relative/path")).is_err());
    }
}
