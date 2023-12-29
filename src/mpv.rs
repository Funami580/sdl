use std::io::Cursor;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use rand::Rng;
use tokio::io::AsyncWriteExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::compat::FuturesAsyncWriteCompatExt;

use crate::download::get_episode_name;
use crate::downloaders::{DownloadTask, SeriesInfo};

pub(crate) fn start_mpv(url: &str, debug: bool) -> Result<(), anyhow::Error> {
    let mut mpv_cmd = tokio::process::Command::new(mpv_name());

    if !debug {
        mpv_cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        mpv_cmd.arg("--no-terminal");
    }

    let title = "sdl";

    mpv_cmd
        .arg("--{")
        .arg(format!("--force-media-title={title}"))
        .arg(url)
        .arg("--}")
        .spawn()
        .map(|_| ())
        .with_context(|| "failed to start mpv")
}

pub(crate) async fn start_mpv_with_ipc(
    mut rx_stream: UnboundedReceiverStream<DownloadTask>,
    series_info: SeriesInfo,
    debug: bool,
) -> Result<(), anyhow::Error> {
    let ipc_path_mpv = if cfg!(unix) {
        let mut i = 0u32;

        loop {
            let path = format!("/tmp/mpvsocket_sdl{i:0>4}");
            let exists = tokio::fs::try_exists(&path)
                .await
                .with_context(|| format!("failed to check if the file \"{}\" exists", path))?;

            if !exists {
                break path;
            }

            match i.checked_add(1) {
                Some(next_i) => i = next_i,
                None => anyhow::bail!("failed to find a name for the mpv socket file"),
            }
        }
    } else {
        let mut rand = rand::thread_rng();
        format!(r"mpvsocket_sdl{:0>4}", rand.gen_range(0..=9999))
    };

    let (first_url, first_title) = match rx_stream.next().await {
        Some(task) => {
            let url = task.download_url;
            let title = get_episode_name(Some(&series_info.title), Some(&task.language), &task.episode_info, true);
            (url, title)
        }
        None => anyhow::bail!("failed to get at least one episode url"),
    };

    let mut mpv_cmd = Command::new(mpv_name());

    if !debug {
        mpv_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        mpv_cmd.arg("--no-terminal");
    }

    mpv_cmd
        .arg(format!("--input-ipc-server={ipc_path_mpv}"))
        .arg("--{")
        .arg(format!("--force-media-title={first_title}"))
        .arg(first_url)
        .arg("--}")
        .spawn()
        .with_context(|| "failed to start mpv")?;

    let ipc_path_rs = if cfg!(unix) {
        ipc_path_mpv.to_owned()
    } else {
        format!("@{}", ipc_path_mpv)
    };

    let mpv_ipc_result = run_mpv_ipc(&ipc_path_rs, rx_stream, series_info).await;

    if cfg!(unix) {
        let _ = tokio::fs::remove_file(&ipc_path_mpv).await;
    }

    mpv_ipc_result
}

async fn run_mpv_ipc(
    ipc_path_rs: &str,
    mut rx_stream: UnboundedReceiverStream<DownloadTask>,
    series_info: SeriesInfo,
) -> Result<(), anyhow::Error> {
    // Try for 10 seconds to connect to IPC
    let ipc = {
        let mut tries = 0u8;

        loop {
            match interprocess::local_socket::tokio::LocalSocketStream::connect(ipc_path_rs).await {
                Ok(ipc) => break ipc,
                Err(err) => {
                    tries += 1;

                    if tries == 200 {
                        return Err(err).with_context(|| "failed to connect to ipc socket");
                    }

                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    };
    let (_, ipc_write) = ipc.into_split();
    let mut ipc_write = ipc_write.compat_write();

    while let Some(task) = rx_stream.next().await {
        let url = task.download_url;
        let title = get_episode_name(Some(&series_info.title), Some(&task.language), &task.episode_info, true);
        let title_len = title.as_bytes().len();
        let title_arg = format!("force-media-title=%{title_len}%{title}");

        let mut mpv_cmd = serde_json::json!({
            "command": [
                "loadfile",
                url,
                "append-play",
                title_arg
            ]
        })
        .to_string();
        mpv_cmd.push('\n');

        let mut message = Cursor::new(mpv_cmd);
        ipc_write
            .write_all_buf(&mut message)
            .await
            .with_context(|| "failed to send url to mpv playlist: failed write")?;
        ipc_write
            .flush()
            .await
            .with_context(|| "failed to send url to mpv playlist: failed flush")?;
    }

    Ok(())
}

fn mpv_name() -> &'static str {
    if cfg!(unix) {
        "mpv"
    } else {
        "mpv.exe"
    }
}
