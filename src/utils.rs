use std::io::ErrorKind;
use std::path::Path;

pub(crate) async fn remove_file_ignore_not_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

pub(crate) async fn remove_dir_all_ignore_not_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
    match tokio::fs::remove_dir_all(path).await {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}
