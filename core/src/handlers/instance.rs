use std::path::PathBuf;

use axum::routing::{delete, get, post};
use axum::Router;
use axum::{extract::Path, Json};
use axum_auth::AuthBearer;

use bollard::container::ListContainersOptions;
use bollard::Docker;
use color_eyre::eyre::{eyre, Context};
use serde::Deserialize;
use tracing::{error, info};

use crate::auth::user::UserAction;
use crate::error::{Error, ErrorKind};
use crate::events::{CausedBy, Event, ProgressionEndValue, ProgressionStartValue};

use crate::implementations::generic;
use crate::traits::t_configurable::GameType;

use crate::implementations::minecraft::MinecraftInstance;
use crate::prelude::{path_to_instances, GameInstance};
use crate::traits::t_configurable::manifest::SetupValue;
use crate::traits::t_configurable::Game::Generic;
use crate::traits::{t_configurable::TConfigurable, t_server::TServer, InstanceInfo, TInstance};
use crate::types::{DotLodestoneConfig, InstanceUuid};
use crate::util::{unzip_file, unzip_file_async, UnzipOption};
use crate::{implementations::minecraft, traits::t_server::State, AppState};

use super::instance_setup_configs::HandlerGameType;
use super::util::decode_base64;

pub async fn get_instance_list(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<Vec<InstanceInfo>>, Error> {
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;
    let mut list_of_configs: Vec<InstanceInfo> = Vec::new();

    for instance in state.instances.iter() {
        if requester.can_perform_action(&UserAction::ViewInstance(instance.uuid().await)) {
            list_of_configs.push(instance.get_instance_info().await);
        }
    }
    let docker_bridge = state.docker_bridge.clone();
    let vec = docker_bridge.list_containers().await.unwrap_or_default();

    list_of_configs.extend(vec);

    list_of_configs.sort_by(|a, b| a.creation_time.cmp(&b.creation_time));

    Ok(Json(list_of_configs))
}

pub async fn get_instance_info(
    Path(uuid): Path<InstanceUuid>,
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<InstanceInfo>, Error> {
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;

    let instance = state.instances.get(&uuid).ok_or_else(|| Error {
        kind: ErrorKind::NotFound,
        source: eyre!("Instance not found"),
    })?;

    requester.try_action(
        &UserAction::ViewInstance(uuid.clone()),
        state.global_settings.lock().await.safe_mode(),
    )?;
    Ok(Json(instance.get_instance_info().await))
}

pub async fn create_minecraft_instance_from_zip(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    Path((game_type, base64_absolute_path)): Path<(HandlerGameType, String)>,
    Json(manifest_value): Json<SetupValue>,
) -> Result<Json<InstanceUuid>, Error> {
    let zip_path = decode_base64(&base64_absolute_path)?;
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;
    requester.try_action(
        &UserAction::CreateInstance,
        state.global_settings.lock().await.safe_mode(),
    )?;
    let mut perm = requester.permissions;

    let mut instance_uuid = InstanceUuid::default();

    for entry in state.instances.iter() {
        if let Some(uuid) = entry.key().as_ref().get(0..8) {
            if uuid == &instance_uuid.no_prefix()[0..8] {
                instance_uuid = InstanceUuid::default();
            }
        }
    }

    let instance_uuid = instance_uuid;

    let flavour = game_type.try_into()?;

    let setup_config = MinecraftInstance::construct_setup_config(manifest_value, flavour).await?;

    let setup_path = path_to_instances().join(format!(
        "{}-{}",
        setup_config.name,
        &instance_uuid.no_prefix()[0..8]
    ));

    tokio::fs::create_dir_all(&setup_path)
        .await
        .context("Failed to create instance directory")?;

    let zip = PathBuf::from(&zip_path);

    unzip_file_async(&zip, UnzipOption::ToDir(setup_path.clone()))
        .await
        .context("Failed to unzip given file")?;

    let dot_lodestone_config = DotLodestoneConfig::new(instance_uuid.clone(), game_type.into());

    // write dot lodestone config

    tokio::fs::write(
        setup_path.join(".lodestone_config"),
        serde_json::to_string_pretty(&dot_lodestone_config).unwrap(),
    )
    .await
    .context("Failed to write .lodestone_config file")?;

    tokio::task::spawn({
        let uuid = instance_uuid.clone();
        let instance_name = setup_config.name.clone();
        let event_broadcaster = state.event_broadcaster.clone();
        let caused_by = CausedBy::User {
            user_id: requester.uid.clone(),
            user_name: requester.username.clone(),
        };
        async move {
            let (progression_start_event, event_id) = Event::new_progression_event_start(
                format!("Setting up Minecraft server {instance_name}"),
                Some(10.0),
                Some(ProgressionStartValue::InstanceCreation {
                    instance_uuid: uuid.clone(),
                }),
                caused_by,
            );
            event_broadcaster.send(progression_start_event);
            let minecraft_instance = match minecraft::MinecraftInstance::restore(
                setup_path.clone(),
                dot_lodestone_config,
                state.event_broadcaster.clone(),
                state.macro_executor.clone(),
            )
            .await
            {
                Ok(v) => {
                    event_broadcaster.send(Event::new_progression_event_end(
                        event_id,
                        true,
                        Some("Instance created successfully"),
                        Some(ProgressionEndValue::InstanceCreation(
                            v.get_instance_info().await,
                        )),
                    ));
                    v
                }
                Err(e) => {
                    event_broadcaster.send(Event::new_progression_event_end(
                        event_id,
                        false,
                        Some(&format!("Instance creation failed: {e}")),
                        None,
                    ));
                    crate::util::fs::remove_dir_all(setup_path)
                        .await
                        .context("Failed to remove directory after instance creation failed")
                        .unwrap();
                    return;
                }
            };
            let mut port_manager = state.port_manager.lock().await;
            port_manager.add_port(setup_config.port);
            perm.can_start_instance.insert(uuid.clone());
            perm.can_stop_instance.insert(uuid.clone());
            perm.can_view_instance.insert(uuid.clone());
            perm.can_read_instance_file.insert(uuid.clone());
            perm.can_write_instance_file.insert(uuid.clone());
            // ignore errors since we don't care if the permissions update fails
            let _ = state
                .users_manager
                .write()
                .await
                .update_permissions(&requester.uid, perm, CausedBy::System)
                .await
                .map_err(|e| {
                    error!("Failed to update permissions: {:?}", e);
                    e
                });
            state
                .instances
                .insert(uuid.clone(), minecraft_instance.into());
        }
    });
    Ok(Json(instance_uuid))
}

pub async fn create_minecraft_instance(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    Path(game_type): Path<HandlerGameType>,
    Json(manifest_value): Json<SetupValue>,
) -> Result<Json<InstanceUuid>, Error> {
    println!("wowie");
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;
    requester.try_action(
        &UserAction::CreateInstance,
        state.global_settings.lock().await.safe_mode(),
    )?;
    let mut perm = requester.permissions;

    let mut instance_uuid = InstanceUuid::default();

    for entry in state.instances.iter() {
        if let Some(uuid) = entry.key().as_ref().get(0..8) {
            if uuid == &instance_uuid.no_prefix()[0..8] {
                instance_uuid = InstanceUuid::default();
            }
        }
    }

    let instance_uuid = instance_uuid;

    let flavour = game_type.try_into()?;

    let setup_config = MinecraftInstance::construct_setup_config(manifest_value, flavour).await?;

    let setup_path = path_to_instances().join(format!(
        "{}-{}",
        setup_config.name,
        &instance_uuid.no_prefix()[0..8]
    ));

    tokio::fs::create_dir_all(&setup_path)
        .await
        .context("Failed to create instance directory")?;

    let dot_lodestone_config = DotLodestoneConfig::new(instance_uuid.clone(), game_type.into());

    // write dot lodestone config

    tokio::fs::write(
        setup_path.join(".lodestone_config"),
        serde_json::to_string_pretty(&dot_lodestone_config).unwrap(),
    )
    .await
    .context("Failed to write .lodestone_config file")?;

    tokio::task::spawn({
        let uuid = instance_uuid.clone();
        let instance_name = setup_config.name.clone();
        let event_broadcaster = state.event_broadcaster.clone();
        let caused_by = CausedBy::User {
            user_id: requester.uid.clone(),
            user_name: requester.username.clone(),
        };
        async move {
            let (progression_start_event, event_id) = Event::new_progression_event_start(
                format!("Setting up Minecraft server {instance_name}"),
                Some(10.0),
                Some(ProgressionStartValue::InstanceCreation {
                    instance_uuid: uuid.clone(),
                }),
                caused_by,
            );
            event_broadcaster.send(progression_start_event);
            let minecraft_instance = match minecraft::MinecraftInstance::new(
                setup_config.clone(),
                dot_lodestone_config,
                setup_path.clone(),
                &event_id,
                state.event_broadcaster.clone(),
                state.macro_executor.clone(),
            )
            .await
            {
                Ok(v) => {
                    event_broadcaster.send(Event::new_progression_event_end(
                        event_id,
                        true,
                        Some("Instance created successfully"),
                        Some(ProgressionEndValue::InstanceCreation(
                            v.get_instance_info().await,
                        )),
                    ));
                    v
                }
                Err(e) => {
                    event_broadcaster.send(Event::new_progression_event_end(
                        event_id,
                        false,
                        Some(&format!("Instance creation failed: {e}")),
                        None,
                    ));
                    crate::util::fs::remove_dir_all(setup_path)
                        .await
                        .context("Failed to remove directory after instance creation failed")
                        .unwrap();
                    return;
                }
            };
            let mut port_manager = state.port_manager.lock().await;
            port_manager.add_port(setup_config.port);
            perm.can_start_instance.insert(uuid.clone());
            perm.can_stop_instance.insert(uuid.clone());
            perm.can_view_instance.insert(uuid.clone());
            perm.can_read_instance_file.insert(uuid.clone());
            perm.can_write_instance_file.insert(uuid.clone());
            // ignore errors since we don't care if the permissions update fails
            let _ = state
                .users_manager
                .write()
                .await
                .update_permissions(&requester.uid, perm, CausedBy::System)
                .await
                .map_err(|e| {
                    error!("Failed to update permissions: {:?}", e);
                    e
                });
            state
                .instances
                .insert(uuid.clone(), minecraft_instance.into());
        }
    });
    Ok(Json(instance_uuid))
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenericSetupConfig {
    url: String,
    setup_value: SetupValue,
}

pub async fn create_generic_instance(
    axum::extract::State(state): axum::extract::State<AppState>,
    AuthBearer(token): AuthBearer,
    Json(setup_config): Json<GenericSetupConfig>,
) -> Result<Json<()>, Error> {
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;
    requester.try_action(
        &UserAction::CreateInstance,
        state.global_settings.lock().await.safe_mode(),
    )?;
    let mut instance_uuid = InstanceUuid::default();
    for entry in state.instances.iter() {
        if let Some(uuid) = entry.key().as_ref().get(0..8) {
            if uuid == &instance_uuid.no_prefix()[0..8] {
                instance_uuid = InstanceUuid::default();
            }
        }
    }

    let instance_uuid = instance_uuid;

    let setup_path = path_to_instances().join(format!(
        "{}-{}",
        setup_config.setup_value.name,
        &instance_uuid.no_prefix()[0..8]
    ));

    tokio::fs::create_dir_all(&setup_path)
        .await
        .context("Failed to create instance directory")?;

    let dot_lodestone_config = DotLodestoneConfig::new(instance_uuid.clone(), GameType::Generic);
    let event_broadcaster = state.event_broadcaster.clone();
    tokio::task::spawn(async move {
        let (progression_start_event, event_id) = Event::new_progression_event_start(
            format!("Setting up instance {}", setup_config.setup_value.name),
            Some(100.0),
            Some(ProgressionStartValue::InstanceCreation {
                instance_uuid: instance_uuid.clone(),
            }),
            CausedBy::User {
                user_id: requester.uid,
                user_name: requester.username,
            },
        );
        event_broadcaster.send(progression_start_event);
        let instance = match generic::GenericInstance::new(
            setup_config.url.into(),
            setup_path.clone(),
            dot_lodestone_config.clone(),
            setup_config.setup_value,
            &event_id,
            state.event_broadcaster.clone(),
            state.macro_executor.clone(),
        )
        .await
        {
            Ok(v) => {
                info!("Atom created successfully");
                event_broadcaster.send(Event::new_progression_event_end(
                    event_id,
                    true,
                    Some("Instance created successfully"),
                    Some(ProgressionEndValue::InstanceCreation(
                        v.get_instance_info().await,
                    )),
                ));
                v
            }
            Err(e) => {
                error!("Atom creation failed: {:?}", e);
                event_broadcaster.send(Event::new_progression_event_end(
                    event_id,
                    false,
                    Some(&format!("Instance creation failed: {e}")),
                    None,
                ));
                crate::util::fs::remove_dir_all(setup_path)
                    .await
                    .map_err(Error::log)
                    .context("Failed to remove directory after instance creation failed")
                    .unwrap();
                return;
            }
        };

        // write dot lodestone config

        tokio::fs::write(
            setup_path.join(".lodestone_config"),
            serde_json::to_string_pretty(&dot_lodestone_config).unwrap(),
        )
        .await
        .context("Failed to write .lodestone_config file")
        .unwrap();

        state
            .instances
            .insert(instance_uuid.clone(), instance.into());
    });

    Ok(Json(()))
}

pub async fn delete_instance(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(uuid): Path<InstanceUuid>,
    AuthBearer(token): AuthBearer,
) -> Result<Json<()>, Error> {
    let requester = state.users_manager.read().await.try_auth_or_err(&token)?;
    requester.try_action(
        &UserAction::DeleteInstance,
        state.global_settings.lock().await.safe_mode(),
    )?;
    let caused_by = CausedBy::User {
        user_id: requester.uid.clone(),
        user_name: requester.username.clone(),
    };
    if let Some((_, instance)) = state.instances.remove(&uuid) {
        if !(instance.state().await == State::Stopped) {
            state.instances.insert(uuid.clone(), instance);
            Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Instance must be stopped before deletion"),
            })
        } else {
            let (progression_event_start, event_id) = Event::new_progression_event_start(
                format!("Deleting instance {}", instance.name().await),
                Some(10.0),
                None,
                caused_by,
            );
            let event_broadcaster = state.event_broadcaster.clone();
            event_broadcaster.send(progression_event_start);
            if let Err(e) =
                tokio::fs::remove_file(instance.path().await.join(".lodestone_config")).await
            {
                event_broadcaster.send(Event::new_progression_event_end(
                    event_id,
                    false,
                    Some("Failed to delete .lodestone_config. Instance not deleted"),
                    None,
                ));
                state.instances.insert(uuid.clone(), instance);
                return Err::<Json<()>, std::io::Error>(e)
                    .context("Failed to delete .lodestone_config file. Instance not deleted")
                    .map_err(Into::into);
            }

            state
                .port_manager
                .lock()
                .await
                .deallocate(instance.port().await);
            let instance_path = instance.path().await;
            // if instance is generic
            if let GameInstance::GenericInstance(i) = instance {
                i.destruct().await;
            };
            let res = crate::util::fs::remove_dir_all(instance_path).await;
            match &res {
                Ok(_) => event_broadcaster.send(Event::new_progression_event_end(
                    event_id,
                    true,
                    Some("Instance deleted successfully"),
                    Some(ProgressionEndValue::InstanceDelete {
                        instance_uuid: uuid.clone(),
                    }),
                )),
                Err(e) => {
                    event_broadcaster.send(Event::new_progression_event_end(
                        event_id,
                        false,
                        Some(&format!(
                            "Failed to delete some of all of instance's files : {e}"
                        )),
                        None,
                    ));
                }
            }
            res.map(|_| Json(()))
        }
    } else {
        Err(Error {
            kind: ErrorKind::NotFound,
            source: eyre!("Instance not found"),
        })
    }
}

pub fn get_instance_routes(state: AppState) -> Router {
    Router::new()
        .route("/instance/list", get(get_instance_list))
        .route(
            "/instance/create/:game_type",
            post(create_minecraft_instance),
        )
        .route(
            "/instance/create_from_zip/:game_type/:base64_absolute_path",
            post(create_minecraft_instance_from_zip),
        )
        .route("/instance/create_generic", post(create_generic_instance))
        .route("/instance/:uuid", delete(delete_instance))
        .route("/instance/:uuid/info", get(get_instance_info))
        .with_state(state)
}
