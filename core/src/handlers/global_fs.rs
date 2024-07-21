use std::fs;
use std::path::PathBuf;

use axum::{
    body::{Bytes, StreamBody},
    extract::{DefaultBodyLimit, Multipart, Path},
    http,
    routing::{delete, get, put},
    Json, Router,
};
use axum_auth::AuthBearer;

use color_eyre::eyre::{eyre, Context};
use headers::{HeaderMap, HeaderName};
use reqwest::header::CONTENT_LENGTH;
use serde::{Deserialize, Serialize};

use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use ts_rs::TS;

use crate::{
    auth::user::UserAction,
    error::{Error, ErrorKind},
    events::{new_fs_event, CausedBy, Event, FSOperation, FSTarget},
    util::{list_dir, rand_alphanumeric, zip_files},
    AppState,
};

use super::util::decode_base64;
use crate::prelude::path_to_tmp;
use tempfile::TempDir;

pub enum DownloadableFile {
    NormalFile(PathBuf),
    ZippedFile((PathBuf, TempDir)),
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FileType {
    File,
    Directory,
    Unknown,
}
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename = "ClientFile")]
#[ts(export)]
pub struct FileEntry {
    pub name: String,
    pub file_stem: String,
    pub extension: Option<String>,
    pub path: String,
    pub size: Option<u64>,
    pub creation_time: Option<u64>,
    pub modification_time: Option<u64>,
    pub file_type: FileType,
}

impl From<&std::path::Path> for FileEntry {
    fn from(path: &std::path::Path) -> Self {
        let file_type = if path.is_dir() {
            FileType::Directory
        } else if path.is_file() {
            FileType::File
        } else {
            FileType::Unknown
        };
        Self {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: if path.is_file() {
                path.metadata().ok().map(|m| m.len())
            } else {
                None
            },
            file_stem: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            extension: path.extension().map(|s| s.to_string_lossy().into_owned()),
            // unix timestamp
            // if we cant get the time, return none
            creation_time: path
                .metadata()
                .ok()
                .and_then(|m| m.created().ok())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
            modification_time: path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),

            file_type,
        }
    }
}

#[derive(Deserialize)]
struct UploadFilePathParams {
    base64_absolute_path: String,
    overwrite: Option<String>,
}

async fn list_files(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<Vec<FileEntry>>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;

    requester.try_action(
        &UserAction::ReadGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);
    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    let ret: Vec<FileEntry> = list_dir(&path, None)
        .await?
        .iter()
        .map(|p| {
            let r: FileEntry = p.as_path().into();
            r
        })
        .collect();
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Read,
        FSTarget::Directory(path),
        caused_by,
    ));
    Ok(Json(ret))
}

async fn read_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<String, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;

    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::ReadGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);
    let ret = tokio::fs::read_to_string(&path).await.context(
        "
        Failed to read file
    ",
    )?;
    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Read,
        FSTarget::File(path),
        caused_by,
    ));
    Ok(ret)
}

async fn get_tmp_path(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<String>, Error> {
    let tmp_path = path_to_tmp()
        .clone()
        .into_os_string()
        .into_string()
        .expect("Failed to get path to tmp directory");

    Ok(Json(tmp_path))
}

async fn write_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
    body: Bytes,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;

    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);

    tokio::fs::write(&path, body)
        .await
        .context(format!("Failed to write to file {}", path.display()))?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Write,
        FSTarget::File(path),
        caused_by,
    ));
    Ok(Json(()))
}

async fn make_directory(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;

    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);
    tokio::fs::create_dir(&path).await.context(format!(
        "
        Failed to create directory {}
    ",
        path.display()
    ))?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Create,
        FSTarget::Directory(path),
        caused_by,
    ));
    Ok(Json(()))
}

async fn move_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path((base64_absolute_path_source, base64_absolute_path_dest)): Path<(String, String)>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let path_source = decode_base64(&base64_absolute_path_source)?;
    let path_dest = decode_base64(&base64_absolute_path_dest)?;

    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;

    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    crate::util::fs::rename(&path_source, &path_dest).await?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };

    state.event_broadcaster.send(new_fs_event(
        FSOperation::Move {
            source: PathBuf::from(&path_source),
        },
        FSTarget::File(PathBuf::from(path_source)),
        caused_by,
    ));

    Ok(Json(()))
}

async fn remove_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);

    tokio::fs::remove_file(&path)
        .await
        .context(format!("Failed to remove file {}", path.display()))?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Delete,
        FSTarget::File(path),
        caused_by,
    ));
    Ok(Json(()))
}

async fn remove_dir(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);

    tokio::fs::remove_dir_all(&path)
        .await
        .context(format!("Failed to remove directory {}", path.display()))?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username,
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Delete,
        FSTarget::Directory(path),
        caused_by,
    ));

    Ok(Json(()))
}

async fn new_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let path = PathBuf::from(absolute_path);

    tokio::fs::File::create(&path)
        .await
        .context(format!("Failed to create file {}", path.display()))?;

    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username.clone(),
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Create,
        FSTarget::File(path),
        caused_by,
    ));

    Ok(Json(()))
}

async fn download_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(base64_absolute_path): Path<String>,
    AuthBearer(token): AuthBearer,
) -> Result<String, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;
    requester.try_action(
        &UserAction::ReadGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;
    let path = PathBuf::from(absolute_path);
    let downloadable_file_path: PathBuf;
    let downloadable_file = if fs::metadata(path.clone()).unwrap().is_dir() {
        let lodestone_tmp = path_to_tmp().clone();
        let temp_dir =
            tempfile::tempdir_in(lodestone_tmp).context("Failed to create temporary file")?;
        let mut temp_file_path: PathBuf = temp_dir.path().into();
        temp_file_path.push(path.file_name().unwrap());
        temp_file_path.set_extension("zip");
        let files = Vec::from([path.clone()]);
        zip_files(&files, temp_file_path.clone(), true).context("Failed to zip file")?;
        downloadable_file_path = temp_file_path.clone();
        DownloadableFile::ZippedFile((downloadable_file_path.clone(), temp_dir))
    } else {
        downloadable_file_path = path.clone();
        DownloadableFile::NormalFile(path.clone())
    };

    let key = rand_alphanumeric(32);
    state
        .download_urls
        .lock()
        .await
        .insert(key.clone(), downloadable_file);
    let caused_by = CausedBy::User {
        user_id: requester.uid,
        user_name: requester.username.clone(),
    };
    state.event_broadcaster.send(new_fs_event(
        FSOperation::Download,
        FSTarget::File(downloadable_file_path),
        caused_by,
    ));
    Ok(key)
}

async fn upload_file(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(UploadFilePathParams {
        base64_absolute_path,
        overwrite,
    }): Path<UploadFilePathParams>,
    headers: HeaderMap,
    AuthBearer(token): AuthBearer,
    mut multipart: Multipart,
) -> Result<Json<()>, Error> {
    let absolute_path = decode_base64(&base64_absolute_path)?;
    let requester = state
        .users_manager
        .read()
        .await
        .try_auth(&token)
        .ok_or_else(|| Error {
            kind: ErrorKind::Unauthorized,
            source: eyre!("Token error"),
        })?;

    requester.try_action(
        &UserAction::WriteGlobalFile,
        state.global_settings.lock().await.safe_mode(),
    )?;

    let overwrite = match overwrite {
        Some(should_overwrite) => should_overwrite == "overwrite",
        None => false,
    };

    let path_to_dir = PathBuf::from(absolute_path);

    tokio::fs::create_dir_all(&path_to_dir)
        .await
        .context(format!(
            "Failed to create directory {}",
            path_to_dir.display()
        ))?;

    let total = headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<f64>().ok());

    let (progression_start_event, event_id) = Event::new_progression_event_start(
        "Uploading file(s)",
        total,
        None,
        CausedBy::User {
            user_id: requester.uid.clone(),
            user_name: requester.username.clone(),
        },
    );
    state.event_broadcaster.send(progression_start_event);

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let name = field
            .file_name()
            .ok_or_else(|| Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Missing file name"),
            })?
            .to_owned();
        let path = path_to_dir.join(&name);
        let path = if path.exists() && !overwrite {
            // add a postfix to the file name
            let mut postfix = 1;
            // get the file name without the extension
            let file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
            loop {
                let new_path = path.with_file_name(format!(
                    "{}_{}.{}",
                    file_name,
                    postfix,
                    path.extension().unwrap().to_str().unwrap()
                ));
                if !new_path.exists() {
                    break new_path;
                }
                postfix += 1;
            }
        } else {
            path
        };
        let mut file = tokio::fs::File::create(&path)
            .await
            .context(format!("Failed to create file {}", path.display()))?;

        while let Some(chunk) = match field.chunk().await {
            Ok(v) => v,
            Err(e) => {
                tokio::fs::remove_file(&path).await.ok();
                state
                    .event_broadcaster
                    .send(Event::new_progression_event_end(
                        event_id,
                        false,
                        Some(&e.to_string()),
                        None,
                    ));
                return Err(Error {
                    kind: ErrorKind::BadRequest,
                    source: eyre!("Failed to read chunk: {}", e),
                });
            }
        } {
            state
                .event_broadcaster
                .send(Event::new_progression_event_update(
                    &event_id,
                    format!("Uploading {name}"),
                    chunk.len() as f64,
                ));
            file.write_all(&chunk).await.map_err(|_| {
                std::fs::remove_file(&path).ok();
                eyre!("Failed to write chunk")
            })?;
        }

        let caused_by = CausedBy::User {
            user_id: requester.uid.clone(),
            user_name: requester.username.clone(),
        };
        state.event_broadcaster.send(new_fs_event(
            FSOperation::Upload,
            FSTarget::File(path),
            caused_by,
        ));
    }
    state
        .event_broadcaster
        .send(Event::new_progression_event_end(
            event_id,
            true,
            Some("Upload complete: "),
            None,
        ));

    Ok(Json(()))
}

async fn download(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(key): Path<String>,
) -> Result<
    (
        [(HeaderName, String); 3],
        StreamBody<ReaderStream<tokio::fs::File>>,
    ),
    Error,
> {
    if let Some(downloadable_file) = state.download_urls.lock().await.get(&key) {
        let path = match downloadable_file {
            DownloadableFile::NormalFile(path) => path,
            DownloadableFile::ZippedFile((path, _)) => path,
        };

        let file = tokio::fs::File::open(&path)
            .await
            .context(format!("Failed to open file {}", path.display()))?;

        let headers = [
            (
                http::header::CONTENT_DISPOSITION,
                "application/octet-stream".to_string(),
            ),
            (
                http::header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    path.file_name()
                        .and_then(|s| s.to_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown".to_string())
                ),
            ),
            if let Ok(metadata) = file.metadata().await {
                (http::header::CONTENT_LENGTH, metadata.len().to_string())
            } else {
                // if we can't get the file size, we just don't set the header
                // but the rust compiler enforces array length to be known at compile time
                // so we just set a dummy header
                (http::header::ACCEPT_LANGUAGE, "*".to_string())
            },
        ];
        let stream = ReaderStream::new(file);
        let body = StreamBody::new(stream);

        Ok((headers, body))
    } else {
        Err(Error {
            kind: ErrorKind::NotFound,
            source: eyre!("File not found with the download key"),
        })
    }
}

pub fn get_global_fs_routes(state: AppState) -> Router {
    Router::new()
        .route("/fs/:base64_absolute_path/ls", get(list_files))
        .route("/fs/:base64_absolute_path/read", get(read_file))
        .route("/fs/:base64_absolute_path/write", put(write_file))
        .route("/fs/:base64_absolute_path/mkdir", put(make_directory))
        .route(
            "/fs/:base64_absolute_path/move/:base64_relative_path_dest",
            put(move_file),
        )
        .route("/fs/:base64_absolute_path/rm", delete(remove_file))
        .route("/fs/:base64_absolute_path/rmdir", delete(remove_dir))
        .route("/fs/:base64_absolute_path/new", put(new_file))
        .route("/fs/:base64_absolute_path/download", get(download_file))
        .route("/fs/:base64_absolute_path/upload", put(upload_file))
        .layer(DefaultBodyLimit::disable())
        .route(
            "/fs/:base64_absolute_path/upload/:overwrite",
            put(upload_file),
        )
        .layer(DefaultBodyLimit::disable())
        .route("/fs/tmp_path", get(get_tmp_path))
        .route("/file/:key", get(download))
        .with_state(state)
}
