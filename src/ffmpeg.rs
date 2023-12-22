use std::path::PathBuf;

use anyhow::Context;

use crate::download::{Downloader, InternalDownloadTask};

enum Platform {
    Unsupported,
    Linux,
    Windows,
    MacOs,
    FreeBsd,
}

enum Architecture {
    Unsupported,
    X86_64,
    X86,
    Aarch64,
    Arm,
}

impl Platform {
    const fn current() -> Self {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "freebsd") {
            Platform::FreeBsd
        } else {
            Platform::Unsupported
        }
    }
}

impl Architecture {
    const fn current() -> Self {
        if cfg!(target_arch = "x86_64") {
            Architecture::X86_64
        } else if cfg!(target_arch = "x86") {
            Architecture::X86
        } else if cfg!(target_arch = "aarch64") {
            Architecture::Aarch64
        } else if cfg!(target_arch = "arm") {
            Architecture::Arm
        } else {
            Architecture::Unsupported
        }
    }
}

fn ffmpeg_download_url() -> Result<String, anyhow::Error> {
    const CURRENT_PLATFORM: Platform = Platform::current();
    const CURRENT_ARCHITECTURE: Architecture = Architecture::current();

    let platform_name = match CURRENT_PLATFORM {
        Platform::Unsupported => anyhow::bail!("unsupported platform"),
        Platform::Linux => "linux",
        Platform::Windows => "win32",
        Platform::MacOs => "darwin",
        Platform::FreeBsd => "freebsd",
    };

    let architecture_name = match CURRENT_ARCHITECTURE {
        Architecture::Unsupported => anyhow::bail!("unsupported architecture"),
        Architecture::X86_64 => "x64",
        Architecture::X86 => "ia32",
        Architecture::Aarch64 => "arm64",
        Architecture::Arm => "arm64",
    };

    let supported = match (CURRENT_PLATFORM, CURRENT_ARCHITECTURE) {
        (Platform::Unsupported, _) | (_, Architecture::Unsupported) => false,
        (Platform::Linux, Architecture::X86_64) => true,
        (Platform::Linux, Architecture::X86) => true,
        (Platform::Linux, Architecture::Aarch64) => true,
        (Platform::Linux, Architecture::Arm) => true,
        (Platform::Windows, Architecture::X86_64) => true,
        (Platform::Windows, Architecture::X86) => true,
        (Platform::Windows, Architecture::Aarch64) => false,
        (Platform::Windows, Architecture::Arm) => false,
        (Platform::MacOs, Architecture::X86_64) => true,
        (Platform::MacOs, Architecture::X86) => false,
        (Platform::MacOs, Architecture::Aarch64) => true,
        (Platform::MacOs, Architecture::Arm) => false,
        (Platform::FreeBsd, Architecture::X86_64) => true,
        (Platform::FreeBsd, Architecture::X86) => false,
        (Platform::FreeBsd, Architecture::Aarch64) => false,
        (Platform::FreeBsd, Architecture::Arm) => false,
    };

    if !supported {
        anyhow::bail!("unsupported platform architecture");
    }

    Ok(format!(
        "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-{}-{}.gz",
        platform_name, architecture_name
    ))
}

pub(crate) struct Ffmpeg {
    data_dir: PathBuf,
}

impl Ffmpeg {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Ffmpeg { data_dir }
    }

    pub(crate) async fn auto_download(&self, downloader: &Downloader) -> Result<PathBuf, anyhow::Error> {
        if let Some(path) = self.get_ffmpeg_path() {
            return Ok(path);
        }

        let ffmpeg_url = ffmpeg_download_url()?;
        let gzip_path = self.get_ffmpeg_data_path(true);
        let download_task = InternalDownloadTask::new(gzip_path.clone(), ffmpeg_url)
            .overwrite_file(true)
            .custom_message(Some("Downloading FFmpeg".to_string()));

        downloader.download_to_file(download_task).await?;

        let gzip_file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&gzip_path)
            .await
            .with_context(|| "failed to open compressed FFmpeg file")?;

        let buf_reader = tokio::io::BufReader::new(gzip_file);
        let mut decoder = async_compression::tokio::bufread::GzipDecoder::new(buf_reader);

        let ffmepg_path = self.get_ffmpeg_data_path(false);
        let open_options = {
            let mut open_options = tokio::fs::OpenOptions::new();
            open_options.write(true);
            open_options.truncate(true);
            open_options.create(true);
            #[cfg(unix)]
            open_options.mode(0o755);
            open_options
        };
        let mut output_file = open_options
            .open(&ffmepg_path)
            .await
            .with_context(|| "failed to open or create FFmpeg file")?;

        if let Err(err) = tokio::io::copy(&mut decoder, &mut output_file).await {
            let _ = tokio::fs::remove_file(&ffmepg_path).await;
            return Err(err).with_context(|| "failed to decompress the compressed FFmpeg file");
        }

        let _ = tokio::fs::remove_file(&gzip_path).await;

        Ok(ffmepg_path)
    }

    fn ffmpeg_executable_name() -> &'static str {
        if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        }
    }

    fn get_ffmpeg_data_path(&self, gzip: bool) -> PathBuf {
        self.data_dir.join(if gzip {
            "ffmpeg.gz"
        } else {
            Self::ffmpeg_executable_name()
        })
    }

    pub(crate) fn get_ffmpeg_path(&self) -> Option<PathBuf> {
        pathsearch::find_executable_in_path(Self::ffmpeg_executable_name()).or_else(|| {
            let data_path = self.get_ffmpeg_data_path(false);

            if data_path.exists() {
                Some(data_path)
            } else {
                None
            }
        })
    }
}
