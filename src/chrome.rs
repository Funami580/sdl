use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use selenium_manager::SeleniumManager;
use thirtyfour::common::config::WebDriverConfigBuilder;
use thirtyfour::extensions::query::ElementPollerNoWait;
use thirtyfour::ChromiumLikeCapabilities;

use crate::download::{self, Downloader, InternalDownloadTask};

const UBLOCK_GITHUB_API_URL: &str = "https://api.github.com/repos/gorhill/uBlock/releases/latest";

pub(crate) struct ChromeDriver<'a> {
    data_dir: &'a Path,
    downloader: &'a Downloader,
}

impl<'a> ChromeDriver<'a> {
    pub(crate) async fn get(
        data_dir: &'a Path,
        downloader: &'a Downloader,
        headless: bool,
    ) -> Result<(thirtyfour::WebDriver, Child), anyhow::Error> {
        let chrome_driver = ChromeDriver { data_dir, downloader };
        chrome_driver.chrome_driver(headless).await
    }

    async fn chrome_driver(&self, headless: bool) -> Result<(thirtyfour::WebDriver, Child), anyhow::Error> {
        // Launch ChromeDriver
        let chromedriver_path = Self::get_chromedriver_path()
            .await
            .with_context(|| "failed to find or fetch ChromeDriver")?;

        let Some(port) = portpicker::pick_unused_port() else {
            anyhow::bail!("no free port found for ChromeDriver");
        };

        log::trace!("Starting ChromeDriver on port {}", port);

        let mut chromedriver_cmd = Command::new(chromedriver_path);

        if headless {
            chromedriver_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }

        let child_process = chromedriver_cmd
            .arg(format!("--port={}", port))
            .spawn()
            .with_context(|| "failed to start ChromeDriver")?;

        // ChromeDriver Capabilities
        let mut caps = thirtyfour::DesiredCapabilities::chrome();
        caps.set_no_sandbox().unwrap();
        caps.set_disable_dev_shm_usage().unwrap();
        caps.add_arg("--disable-blink-features=AutomationControlled").unwrap();
        caps.add_arg("window-size=1920,1080").unwrap();
        caps.add_arg("disable-infobars").unwrap();
        if headless {
            caps.add_arg("--headless=new").unwrap();
        }
        caps.add_exclude_switch("enable-automation").unwrap();

        // Add uBlock Origin extension, if possible
        let ublock_dir = self.data_dir.join("uBlock");

        if let Err(err) = self.prepare_ublock(&ublock_dir).await {
            log::warn!("Failed to prepare uBlock Origin: {:#}", err);
        }

        match Self::get_ublock_directory(&ublock_dir).await {
            Ok(ublock_dir) => {
                if let Some(ublock_dir) = ublock_dir.to_str() {
                    caps.add_arg(&format!("--load-extension={ublock_dir}")).unwrap();
                } else {
                    log::warn!("Failed to add uBlock Origin as extension: path to directory is not valid UTF-8");
                }
            }
            Err(err) => log::warn!("Failed to add uBlock Origin as extension: {:#}", err),
        }

        // Initialize ChromeDriver
        let mut driver = None;

        for _ in 0..20 {
            match thirtyfour::WebDriver::new_with_config(
                &format!("http://localhost:{}", port),
                caps.clone(),
                WebDriverConfigBuilder::new()
                    .poller(Arc::new(ElementPollerNoWait))
                    .build(),
            )
            .await
            {
                Ok(d) => {
                    driver = Some(d);
                    break;
                }
                Err(err) => {
                    log::warn!("Could not connect to ChromeDriver: {}", err);
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }

        let Some(driver) = driver else {
            anyhow::bail!("failed to initialize ChromeDriver");
        };

        let dev_tools = thirtyfour::extensions::cdp::ChromeDevTools::new(driver.handle.clone());

        // Remove window.cdc_... properties
        dev_tools
            .execute_cdp_with_params(
                "Page.removeScriptToEvaluateOnNewDocument",
                serde_json::json!({"identifier": "1"}),
            )
            .await
            .unwrap();

        // Patch navigator.webdriver property
        dev_tools
            .execute_cdp_with_params(
                "Page.addScriptToEvaluateOnNewDocument",
                serde_json::json!({
                    "source": r#"
                Object.defineProperty(window, "navigator", {
                    value: new Proxy(navigator, {
                        has: (target, key) => (key === "webdriver" ? false : key in target),
                        get: (target, key) =>
                        key === "webdriver"
                            ? false
                            : typeof target[key] === "function"
                            ? target[key].bind(target)
                            : target[key],
                    }),
                });
            "#
                }),
            )
            .await
            .unwrap();

        Ok((driver, child_process))
    }

    async fn get_chromedriver_path() -> Option<PathBuf> {
        match selenium_manager::chrome::ChromeManager::new() {
            Ok(mut manager) => match tokio::task::spawn_blocking(move || manager.setup()).await {
                Ok(result) => match result {
                    Ok(driver_path) => return Some(driver_path),
                    Err(err) => log::debug!("Failed to set up ChromeDriver: {}", err),
                },
                Err(err) => log::debug!("Failed to set up ChromeDriver: {}", err),
            },
            Err(err) => log::debug!("Failed to create Chrome Manager: {}", err),
        }

        let executable_name = if cfg!(windows) {
            "chromedriver.exe"
        } else {
            "chromedriver"
        };

        pathsearch::find_executable_in_path(executable_name)
    }

    async fn prepare_ublock(&self, ublock_dir: &PathBuf) -> Result<(), anyhow::Error> {
        let current_version_file = self.data_dir.join("current_ublock_version");

        let current_version_read = tokio::fs::read_to_string(&current_version_file).await;
        let current_version = match current_version_read.as_deref() {
            Ok(contents) => Some(contents.trim()),
            Err(err) => {
                if err.kind() != ErrorKind::NotFound {
                    log::warn!("Failed to read current uBlock Origin version file: {err}");
                }

                None
            }
        };

        let github_response = download::get_page_json(UBLOCK_GITHUB_API_URL, None, None).await?;

        const UNEXPECT_JSON_ERR_MSG: &str = "unexpected GitHub API json response";
        let serde_json::Value::Object(json_object) = github_response else {
            anyhow::bail!(UNEXPECT_JSON_ERR_MSG)
        };
        let Some(serde_json::Value::String(latest_version)) = json_object.get("tag_name") else {
            anyhow::bail!(UNEXPECT_JSON_ERR_MSG)
        };

        let download = if let Some(current_version) = current_version {
            if latest_version == current_version {
                log::trace!("uBlock Origin up-to-date");
                false
            } else {
                log::info!("uBlock Origin out-of-date... Updating...");
                true
            }
        } else {
            log::info!("uBlock Origin not installed... Installing...");
            true
        };

        if !download {
            return Ok(());
        }

        let ublock_download_file_path = self.data_dir.join("uBlock.zip");

        if let Err(err) = tokio::fs::remove_file(&ublock_download_file_path).await {
            if err.kind() != ErrorKind::NotFound {
                return Err(err).with_context(|| "failed to remove old uBlock Origin asset file");
            }
        }

        let Some(serde_json::Value::Array(assets)) = json_object.get("assets") else {
            anyhow::bail!(UNEXPECT_JSON_ERR_MSG)
        };
        let mut found_asset = false;

        for asset in assets {
            let Some(serde_json::Value::String(asset_name)) = asset.get("name") else {
                anyhow::bail!(UNEXPECT_JSON_ERR_MSG)
            };

            if !asset_name.contains("chromium") {
                continue;
            }

            let Some(serde_json::Value::String(download_url)) = asset.get("browser_download_url") else {
                anyhow::bail!(UNEXPECT_JSON_ERR_MSG)
            };
            self.downloader
                .download_to_file(
                    InternalDownloadTask::new(ublock_download_file_path.clone(), download_url.to_owned())
                        .overwrite_file(true)
                        .custom_message(Some("Downloading uBlock Origin".to_string())),
                )
                .await?;
            found_asset = true;

            break;
        }

        if !found_asset {
            anyhow::bail!("could not find the latest uBlock Origin asset for Chromium");
        }

        if let Err(err) = tokio::fs::remove_dir_all(ublock_dir).await {
            if err.kind() != ErrorKind::NotFound {
                return Err(err).with_context(|| "failed to remove old uBlock Origin extension directory");
            }
        }

        tokio::fs::create_dir_all(ublock_dir)
            .await
            .with_context(|| "failed to create uBlock Origin extension directory")?;

        if let Err(err) = zip_extensions::zip_extract(&ublock_download_file_path, ublock_dir) {
            let _ = tokio::fs::remove_file(&current_version_file).await;
            let _ = tokio::fs::remove_dir_all(ublock_dir).await;
            return Err(err).with_context(|| "failed to extract uBlock Origin asset file")?;
        }

        let _ = tokio::fs::remove_file(&ublock_download_file_path).await;
        tokio::fs::write(&current_version_file, &latest_version)
            .await
            .with_context(|| "failed to update uBlock Origin version file")?;

        Ok(())
    }

    async fn get_ublock_directory(ublock_dir: &Path) -> Result<PathBuf, anyhow::Error> {
        let mut ublock_dir_files = tokio::fs::read_dir(&ublock_dir)
            .await
            .with_context(|| "failed to list files in uBlock Origin extension directory")?;
        let mut directory = None;
        let mut encountered_file = false;

        while let Some(file) = ublock_dir_files
            .next_entry()
            .await
            .with_context(|| "failed to get file in uBlock Origin extension directory")?
        {
            if encountered_file {
                return Ok(ublock_dir.to_path_buf());
            }

            let is_directory = file
                .file_type()
                .await
                .with_context(|| "failed to get file type of file in uBlock Origin extension directory")?
                .is_dir();

            if is_directory {
                directory = Some(file.path());
            }

            encountered_file = true;
        }

        if !encountered_file {
            anyhow::bail!("uBlock Origin extension directory is empty");
        }

        Ok(directory.unwrap_or_else(|| ublock_dir.to_path_buf()))
    }
}

pub async fn get_user_agent(driver: &thirtyfour::WebDriver) -> Option<String> {
    driver
        .execute("return navigator.userAgent;", vec![])
        .await
        .ok()
        .and_then(|result| result.json().as_str().map(|user_agent| user_agent.to_string()))
}
