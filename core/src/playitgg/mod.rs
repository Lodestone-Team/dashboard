/*
Copyright 2022 Developed Methods LLC

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
*/

pub mod helper;
pub mod tcp_client;
pub mod utils;

mod playit_secret;
use std::sync::Mutex;
use std::time::Duration;
use toml::Value;
mod errors;
use crate::error::{Error, ErrorKind};
use crate::events::{CausedBy, Event, EventInner, PlayitggRunnerEvent, PlayitggRunnerEventInner};
use crate::prelude::lodestone_path;
use crate::types::Snowflake;
use crate::AppState;
use axum::Json;
use color_eyre::eyre::eyre;
use helper::*;
use playit_agent_core::api::api::ApiError;
use playit_agent_core::api::{
    api::{
        AgentType, ClaimSetupResponse, PortType as PlayitPortType, ReqClaimExchange, ReqClaimSetup,
        ReqTunnelsList,
    },
    PlayitApi,
};
use playit_secret::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::{atomic::Ordering, Arc};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use ts_rs::TS;
use utils::*;

#[derive(Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[ts(export)]
pub struct TunnelUuid(String);

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayitTunnelParams {
    pub local_port: u16,
    pub port_type: PortType,
}

#[derive(Serialize, Deserialize, TS, Clone)]
#[ts(export)]
pub enum PortType {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "udp")]
    Udp,
    #[serde(rename = "both")]
    Both,
}

impl From<PortType> for PlayitPortType {
    fn from(port_type: PortType) -> Self {
        match port_type {
            PortType::Tcp => PlayitPortType::Tcp,
            PortType::Udp => PlayitPortType::Udp,
            PortType::Both => PlayitPortType::Both,
        }
    }
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayitTunnelInfo {
    pub local_ip: String,
    pub local_port: u16,
    pub tunnel_id: TunnelUuid,
    pub name: String,
    pub server_address: String,
    pub active: bool,
}
pub struct TunnelHandle(Arc<AtomicBool>, JoinHandle<()>);

#[derive(Serialize, Deserialize, TS, Clone)]
#[ts(export)]
pub struct PlayitSignupData {
    pub url: String,
    pub claim_code: String,
}

pub async fn start_cli(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<()>, Error> {
    if let Some(keep_running) = state.playit_keep_running.lock().await.clone() {
        if keep_running.load(Ordering::SeqCst) {
            return Ok(Json(()));
        }
    }

    let playitgg_key = state.playitgg_key.lock().await.clone();
    if let Some(playitgg_key) = playitgg_key {
        let api = PlayitApi::create(API_BASE.to_string(), Some(playitgg_key.clone()));
        let lookup = {
            let data = api.agents_rundata().await;
            if let Ok(data) = data {
                let lookup = Arc::new(LocalLookup {
                    data: Mutex::new(vec![]),
                });
                lookup.update(data.tunnels).await;

                lookup
            } else {
                return Err(eyre!("Failed to get rundata").into());
            }
        };

        let runner = TunnelRunner::new(API_BASE.to_string(), playitgg_key, lookup.clone()).await;

        if let Ok(runner) = runner {
            let keep_runing = runner.keep_running();
            state
                .playit_keep_running
                .lock()
                .await
                .replace(keep_runing.clone());

            tokio::spawn(async move {
                runner.run(state.event_broadcaster.clone(), true).await;
            });
        } else {
            return Err(eyre!("Failed to create runner").into());
        }
    } else {
        return Err(eyre!("No playitgg key found").into());
    }

    Ok(Json(()))
}

pub async fn stop_cli(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<()>, Error> {
    if let Some(keep_running) = state.playit_keep_running.lock().await.clone() {
        if keep_running.load(Ordering::SeqCst) {
            state.event_broadcaster.send(Event {
                event_inner: EventInner::PlayitggRunnerEvent(PlayitggRunnerEvent {
                    playitgg_runner_event_inner: PlayitggRunnerEventInner::RunnerStopped,
                }),
                snowflake: Snowflake::default(),
                details: "Stopped".to_string(),
                caused_by: CausedBy::System,
            });
            keep_running.store(false, Ordering::SeqCst);
        }
    }

    Ok(Json(()))
}

pub async fn cli_is_running(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<bool>, Error> {
    if let Some(keep_running) = state.playit_keep_running.lock().await.clone() {
        Ok(Json(keep_running.load(Ordering::SeqCst)))
    } else {
        Ok(Json(false))
    }
}

pub async fn generate_signup_link(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<PlayitSignupData>, Error> {
    let api = PlayitApi::create(API_BASE.to_string(), None);

    let claim_code = claim_generate();
    let url = claim_url(&claim_code)
        .map_err(|e| eyre!("Failed to generate signup url with error code {:?}", e))
        .unwrap();
    let signup_data = Json(PlayitSignupData {
        url,
        claim_code: claim_code.clone(),
    });
    let ret_data = signup_data.clone();

    tokio::spawn(async move {
        loop {
            let setup = api
                .claim_setup(ReqClaimSetup {
                    code: claim_code.to_string(),
                    agent_type: AgentType::Assignable,
                    version: format!("playit-cli {}", "1.0.0-rc3"),
                })
                .await
                .map_err(|e| eyre!("Failed to claim setup {:?}", e))
                .unwrap();

            match setup {
                ClaimSetupResponse::UserAccepted => {
                    println!("User accepted, exchanging code for secret");
                    break;
                }
                ClaimSetupResponse::UserRejected => {
                    println!("User rejected");
                    return Err(Error {
                        kind: ErrorKind::Internal,
                        source: eyre!("Failed to confirm signup with error {:?}", setup),
                    });
                }
                _ => {}
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        let api = PlayitApi::create(API_BASE.to_string(), None);
        match api
            .claim_exchange(ReqClaimExchange {
                code: signup_data.claim_code.to_string(),
            })
            .await
        {
            Ok(res) => {
                let mut secret_key_path = lodestone_path().clone();
                secret_key_path.push("playit.toml");

                let mut toml = toml::map::Map::new();
                toml.insert("last_update".to_string(), Value::Integer(0));
                toml.insert(
                    "api_url".to_string(),
                    Value::String("https://api.playit.cloud/agent".to_string()),
                );
                toml.insert(
                    "ping_target_addresses".to_string(),
                    Value::Array(vec![
                        Value::String("23.133.216.1:5530".to_string()),
                        Value::String("ping.ply.gg".to_string()),
                    ]),
                );
                toml.insert(
                    "control_address".to_string(),
                    Value::String("control.playit.gg".to_string()),
                );
                toml.insert("refresh_from_api".to_string(), Value::Boolean(true));
                toml.insert("api_refresh_rate".to_string(), Value::Integer(5000));
                toml.insert("ping_interval".to_string(), Value::Integer(5000));
                toml.insert(
                    "secret_key".to_string(),
                    Value::String(res.secret_key.clone()),
                );
                toml.insert("mappings".to_string(), Value::Array(vec![]));

                let mut file = tokio::fs::File::create(secret_key_path)
                    .await
                    .map_err(|e| eyre!("Failed to create playit secret file with error {:?}", e))?;
                file.write_all(toml.to_string().as_bytes())
                    .await
                    .map_err(|e| eyre!("Failed to write playit secret key with error {:?}", e))?;

                let api = PlayitApi::create(API_BASE.to_string(), Some(res.secret_key.clone()));

                let lookup = {
                    let data = api.agents_rundata().await;
                    if let Ok(data) = data {
                        let lookup = Arc::new(LocalLookup {
                            data: Mutex::new(vec![]),
                        });
                        lookup.update(data.tunnels).await;

                        lookup
                    } else {
                        return Err(eyre!("Failed to get rundata").into());
                    }
                };

                let runner =
                    TunnelRunner::new(API_BASE.to_string(), res.secret_key.clone(), lookup.clone())
                        .await
                        .map_err(|e| eyre!("Failed to create tunnel object with error {:?}", e))
                        .unwrap();

                tokio::spawn(async move {
                    let signal = runner.keep_running();
                    tokio::spawn(runner.run(state.event_broadcaster.clone(), false));
                    loop {
                        let tunnels = api
                            .agents_rundata()
                            .await
                            .map_err(|e| {
                                eyre!("Failed to get tunnels from playitgg with error {:?}", e)
                            })
                            .unwrap();

                        if !tunnels.tunnels.is_empty() {
                            signal.store(false, Ordering::SeqCst);
                            state
                                .playitgg_key
                                .lock()
                                .await
                                .replace(res.secret_key.clone());
                            break;
                        }

                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                });
                Ok(())
            }
            Err(ApiError::Fail(error)) => Err(Error {
                kind: ErrorKind::Internal,
                source: eyre!("Api error: {:?}", error),
            }),
            Err(error) => Err(Error {
                kind: ErrorKind::Internal,
                source: eyre!("Failed to confirm signup with error {:?}", error),
            }),
        }
    });

    Ok(ret_data)
}

pub async fn verify_key(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<bool>, Error> {
    let secret_key = match state.playitgg_key.lock().await.clone() {
        Some(key) => key,
        None => return Ok(Json(false)),
    };
    Ok(Json(is_valid_secret_key(secret_key).await))
}

pub async fn get_tunnels(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<Vec<PlayitTunnelInfo>>, Error> {
    let secret = if let Some(secret) = state.playitgg_key.lock().await.clone() {
        secret
    } else {
        return Err(Error {
            kind: ErrorKind::Internal,
            source: eyre!("Couldn't find Playit key"),
        });
    };
    let api = make_client(String::from("https://api.playit.gg"), secret.clone());
    let response = api
        .tunnels_list_json(ReqTunnelsList {
            tunnel_id: None,
            agent_id: None,
        })
        .await;
    if let Ok(response) = response {
        let tunnels_value = response.get("tunnels");
        if let Some(tunnels_value) = tunnels_value {
            let tunnels = tunnels_value.as_array();
            if let Some(tunnels) = tunnels {
                let mut res: Vec<PlayitTunnelInfo> = vec![];
                for tunnel in tunnels {
                    let id_value = tunnel.get("id");
                    let name_value = tunnel.get("name");
                    let active_value = tunnel.get("active");

                    if !((id_value.is_some() && id_value.unwrap().as_str().is_some())
                        && (name_value.is_some() && name_value.unwrap().as_str().is_some())
                        && (active_value.is_some() && active_value.unwrap().as_bool().is_some()))
                    {
                        return Err(Error {
                            kind: ErrorKind::Internal,
                            source: eyre!("Got malformed response from Playit"),
                        });
                    }

                    let id = id_value.unwrap().as_str().unwrap().to_string();
                    let name = name_value.unwrap().as_str().unwrap().to_string();
                    let active = active_value.unwrap().as_bool().unwrap();

                    if !((tunnel.get("alloc").is_some()
                        && tunnel.get("alloc").unwrap().get("data").is_some())
                        && (tunnel.get("origin").is_some()
                            && tunnel.get("origin").unwrap().get("data").is_some()))
                    {
                        return Err(Error {
                            kind: ErrorKind::Internal,
                            source: eyre!("Got malformed response from Playit"),
                        });
                    }

                    let alloc_data = tunnel.get("alloc").unwrap().get("data").unwrap();
                    let origin_data = tunnel.get("origin").unwrap().get("data").unwrap();

                    let local_port_value = origin_data.get("local_port");
                    let local_ip_value = origin_data.get("local_ip");
                    let assigned_domain_value = alloc_data.get("assigned_domain");

                    if !(local_port_value.is_some()
                        && local_ip_value.is_some()
                        && assigned_domain_value.is_some())
                    {
                        return Err(Error {
                            kind: ErrorKind::Internal,
                            source: eyre!("Got malformed response from Playit"),
                        });
                    }

                    let local_port = local_port_value.unwrap().as_i64();
                    let local_ip = local_ip_value.unwrap().as_str();
                    let assigned_domain = assigned_domain_value.unwrap().as_str();

                    if !(local_port.is_some() && local_ip.is_some() && assigned_domain.is_some()) {
                        return Err(Error {
                            kind: ErrorKind::Internal,
                            source: eyre!("Got malformed response from Playit"),
                        });
                    }

                    res.push(PlayitTunnelInfo {
                        local_ip: local_ip.unwrap().to_string(),
                        local_port: local_port.unwrap() as u16,
                        name,
                        tunnel_id: TunnelUuid(id),
                        active,
                        server_address: format!("{}", assigned_domain.unwrap(),),
                    });
                }
                Ok(Json(res))
            } else {
                Err(Error {
                    kind: ErrorKind::Internal,
                    source: eyre!("Got malformed response from Playit"),
                })
            }
        } else {
            Err(Error {
                kind: ErrorKind::Internal,
                source: eyre!("Got malformed response from Playit"),
            })
        }
    } else {
        Err(Error {
            kind: ErrorKind::Internal,
            source: eyre!("Couldn't connect to Playit"),
        })
    }
}
