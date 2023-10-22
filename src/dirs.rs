use std::path::PathBuf;

use anyhow::Context;

pub(crate) async fn get_data_dir() -> Result<PathBuf, anyhow::Error> {
    let data_dir = dirs::data_dir().map(|path| path.join("sdl")).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|path| path.join("sdl-data")))
    });

    if let Some(data_dir) = data_dir {
        tokio::fs::create_dir_all(&data_dir).await?;
        Ok(data_dir)
    } else {
        anyhow::bail!("failed to find data directory path");
    }
}

pub(crate) fn get_save_directory(custom_save_directory: Option<PathBuf>) -> Result<PathBuf, anyhow::Error> {
    custom_save_directory
        .ok_or(())
        .or_else(|_| std::env::current_dir().with_context(|| "failed to get current working directory"))
}
