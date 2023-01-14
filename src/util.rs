use std::collections::HashSet;
use tempdir::TempDir;
use tokio::fs::File;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::path::{Path, PathBuf};
use tokio::fs::remove_file;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize)]
pub struct Authentication {
    username: String,
    password: String,
}

use crate::prelude::PATH_TO_BINARIES;
use crate::traits::{Error, ErrorInner};
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetupProgress {
    pub current_step: (u8, String),
    pub total_steps: u8,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub total: Option<u64>,
    pub downloaded: u64,
    pub step: u64,
    pub download_name: String,
}
pub async fn download_file(
    url: &str,
    path: &Path,
    name_override: Option<&str>,
    on_download: &(dyn Fn(DownloadProgress) + Send + Sync),
    overwrite_old: bool,
) -> Result<PathBuf, Error> {
    let client = Client::new();
    let response = client.get(url).send().await.map_err(|e| Error {
        inner: ErrorInner::FailedToUpload,
        detail: format!("Failed to send GET request to {} : {}", url, e),
    })?;
    response.error_for_status_ref().map_err(|e| Error {
        inner: ErrorInner::APIChanged,
        detail: format!("Failed to download file {} : {}", url, e),
    })?;
    tokio::fs::create_dir_all(path).await.map_err(|_| Error {
        inner: ErrorInner::FailedToUpload,
        detail: format!("Failed to create directory {}", path.display()),
    })?;

    let file_name;
    if let Some(name) = name_override {
        file_name = name.to_string();
    } else {
        file_name = response
            .headers()
            .get("Content-Disposition")
            .map_or_else(
                || "unknown".to_string(),
                |h| {
                    h.to_str()
                        .map_or_else(|_| "unknown".to_string(), |s| s.to_string())
                },
            )
            // parse filename's value from the header, remove the ""
            .split(';')
            .nth(1)
            .unwrap_or("unknown")
            .split('=')
            .nth(1)
            .unwrap_or("unknown")
            .replace('\"', "");
    }
    if !overwrite_old && path.join(&file_name).exists() {
        return Err(Error {
            inner: ErrorInner::FiledOrDirAlreadyExists,
            detail: format!("{} already exists", path.join(&file_name).display()),
        });
    }
    remove_file(path.join(&file_name)).await.ok();
    let total_size = response.content_length();
    let pb = ProgressBar::new(total_size.unwrap_or(0));
    pb.set_style(ProgressStyle::default_bar()
        .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .progress_chars("#>-"));
    pb.set_message(&format!("Downloading {}", url));

    let mut downloaded_file = File::create(path.join(&file_name))
        .await
        .map_err(|_| Error {
            inner: ErrorInner::FailedToWriteFileOrDir,
            detail: format!("Failed to create file {}", path.join(&file_name).display()),
        })?;
    let mut downloaded: u64 = 0;
    let mut new_downloaded: u64 = 0;
    let threshold = total_size.unwrap_or(500000) / 100;
    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.expect("Error while downloading file");
        downloaded_file.write_all(&chunk).await.map_err(|e| Error {
            inner: ErrorInner::FailedToWriteFileOrDir,
            detail: format!(
                "Failed to write to file {}, {}",
                path.join(&file_name).display(),
                e
            ),
        })?;
        new_downloaded += chunk.len() as u64;
        let step = new_downloaded - downloaded;
        if step > threshold {
            on_download(DownloadProgress {
                total: total_size,
                downloaded,
                step,
                download_name: file_name.clone(),
            });
            downloaded = new_downloaded;
        }

        pb.set_position(new_downloaded);
    }
    Ok(path.join(&file_name))
}

/// List all files in a directory
/// files_or_dir = 0 -> files, 1 -> directories
pub async fn list_dir(
    path: &Path,
    filter_file_or_dir: Option<bool>,
) -> Result<Vec<PathBuf>, Error> {
    let ret: Result<Vec<PathBuf>, Error> = tokio::task::spawn_blocking({
        let path = path.to_owned();
        move || {
            Ok(std::fs::read_dir(&path)
                .map_err(|_| Error {
                    inner: ErrorInner::FailedToReadFileOrDir,
                    detail: "".to_string(),
                })?
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_ok())
                .filter(|entry| match filter_file_or_dir {
                    // unwrap is safe because we checked if file_type is ok
                    Some(true) => entry.file_type().unwrap().is_dir(),
                    Some(false) => entry.file_type().unwrap().is_file(),
                    None => true,
                })
                .map(|entry| entry.path())
                .collect())
        }
    })
    .await
    .unwrap();
    ret
}

pub async fn unzip_file(
    file: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    overwrite_old: bool,
) -> Result<HashSet<PathBuf>, Error> {
    let file = file.as_ref();
    let dest = dest.as_ref();
    let os = std::env::consts::OS;
    let arch = if std::env::consts::ARCH == "x86_64" {
        "x64"
    } else {
        std::env::consts::ARCH
    };
    let _7zip_name = format!("7z_{}_{}", os, arch);
    let _7zip_path = PATH_TO_BINARIES
        .with(|v| v.clone())
        .join("7zip")
        .join(&_7zip_name);
    if !_7zip_path.is_file() {
        return Err(Error{ inner: ErrorInner::FileOrDirNotFound, detail: format!("Runtime dependency {} is not found at {}. Consider downloading the dependency to .lodestone/bin/7zip/, or reinstall Lodestone", _7zip_name, _7zip_path.display()) });
    }
    tokio::fs::create_dir_all(dest).await.map_err(|_| Error {
        inner: ErrorInner::FailedToWriteFileOrDir,
        detail: format!("Failed to create directory {}", dest.display()),
    })?;
    let before: HashSet<PathBuf>;

    let tmp_dir = TempDir::new("lodestone")
        .map_err(|e| Error {
            inner: ErrorInner::FailedToWriteFileOrDir,
            detail: format!("Failed to create temp dir, {}", e),
        })?
        .path()
        .to_owned();

    let overwrite_arg = if overwrite_old { "-aoa" } else { "-aou" };

    if file.extension().ok_or(Error {
        inner: ErrorInner::MalformedFile,
        detail: "Not a zip file".to_string(),
    })? == "gz"
    {
        dont_spawn_terminal(
            Command::new(&_7zip_path)
                .arg("x")
                .arg(file)
                .arg(overwrite_arg)
                .arg(format!("-o{}", tmp_dir.display())),
        )
        .status()
        .await
        .map_err(|_| Error {
            inner: ErrorInner::FailedToExecute,
            detail: "Failed to execute 7zip".to_string(),
        })?;

        before = list_dir(dest, None)
            .await
            .map_err(|_| Error {
                inner: ErrorInner::FailedToReadFileOrDir,
                detail: "".to_string(),
            })?
            .iter()
            .cloned()
            .collect();

        dont_spawn_terminal(
            Command::new(&_7zip_path)
                .arg("x")
                .arg(&tmp_dir)
                .arg(overwrite_arg)
                .arg("-ttar")
                .arg(format!("-o{}", dest.display())),
        )
        .status()
        .await
        .map_err(|_| Error {
            inner: ErrorInner::FailedToExecute,
            detail: "Failed to execute 7zip".to_string(),
        })?;
    } else {
        before = list_dir(dest, None)
            .await
            .map_err(|_| Error {
                inner: ErrorInner::FailedToReadFileOrDir,
                detail: "".to_string(),
            })?
            .iter()
            .cloned()
            .collect();
        dont_spawn_terminal(
            Command::new(&_7zip_path)
                .arg("x")
                .arg(file)
                .arg(format!("-o{}", dest.display()))
                .arg(overwrite_arg),
        )
        .status()
        .await
        .map_err(|_| Error {
            inner: ErrorInner::FailedToExecute,
            detail: "Failed to execute 7zip".to_string(),
        })?;
    }
    let after: HashSet<PathBuf> = list_dir(dest, None)
        .await
        .map_err(|_| Error {
            inner: ErrorInner::FailedToReadFileOrDir,
            detail: "".to_string(),
        })?
        .iter()
        .cloned()
        .collect();
    Ok((&after - &before).iter().cloned().collect())
}

pub fn rand_alphanumeric(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).collect()
}

// safe_path only works on linux and messes up on windows
// this is a hacky solution
pub fn scoped_join_win_safe<R: AsRef<Path>, U: AsRef<Path>>(
    root: R,
    unsafe_path: U,
) -> Result<PathBuf, Error> {
    let mut ret = safe_path::scoped_join(&root, unsafe_path).map_err(|e| Error {
        inner: ErrorInner::MalformedFile,
        detail: format!("Failed to join path: {}", e),
    })?;
    if cfg!(windows) {
        // construct a new path
        // that replace the prefix component with the component of the root path
        ret = ret
            .components()
            .skip(1)
            .fold(root.as_ref().to_path_buf(), |mut acc, c| {
                acc.push(c.as_os_str());
                acc
            });
    }
    Ok(ret)
}

pub fn dont_spawn_terminal(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    cmd
}

pub fn format_byte_download(bytes: u64, total: u64) -> String {
    let mut bytes = bytes as f64;
    let mut total = total as f64;
    let mut unit = "B";
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "KB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "MB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "GB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "TB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "PB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "EB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "ZB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        total /= 1024.0;
        unit = "YB";
    }
    format!("{:.1} / {:.1} {}", bytes, total, unit)
}

pub fn format_byte(bytes: u64) -> String {
    let mut bytes = bytes as f64;
    let mut unit = "B";
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "KB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "MB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "GB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "TB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "PB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "EB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "ZB";
    }
    if bytes > 1024.0 {
        bytes /= 1024.0;
        unit = "YB";
    }
    format!("{:.1} {}", bytes, unit)
}