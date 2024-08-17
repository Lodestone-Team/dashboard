use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use color_eyre::eyre::{eyre, Context};
use sysinfo::{Pid, PidExt, ProcessExt, SystemExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::error::{Error, ErrorKind};
use crate::events::{CausedBy, Event, EventInner, InstanceEvent, InstanceEventInner};
use crate::implementations::minecraft::line_parser::{
    parse_player_joined, parse_player_left, parse_player_msg, parse_server_started,
    parse_system_msg, PlayerMessage,
};
use crate::implementations::minecraft::player::MinecraftPlayer;
use crate::implementations::minecraft::util::name_to_uuid;
use crate::macro_executor::{DefaultWorkerOptionGenerator, SpawnResult};
use crate::traits::t_configurable::TConfigurable;
use crate::traits::t_macro::TaskEntry;
use crate::traits::t_server::{MonitorReport, State, StateAction, TServer};

use crate::types::Snowflake;
use crate::util::{dont_spawn_terminal, list_dir};

use super::r#macro::resolve_macro_invocation;
use super::{Flavour, ForgeBuildVersion, MinecraftInstance};
use tracing::{error, info, warn};

#[async_trait::async_trait]
impl TServer for MinecraftInstance {
    async fn start(&self, cause_by: CausedBy, block: bool) -> Result<(), Error> {
        let config = self.config.lock().await.clone();
        self.state.lock().await.try_transition(
            StateAction::UserStart,
            Some(&|state| {
                self.event_broadcaster.send(Event {
                    event_inner: EventInner::InstanceEvent(InstanceEvent {
                        instance_name: config.name.clone(),
                        instance_uuid: self.uuid.clone(),
                        instance_event_inner: InstanceEventInner::StateTransition { to: state },
                    }),
                    snowflake: Snowflake::default(),
                    details: "Starting server".to_string(),
                    caused_by: cause_by.clone(),
                });
            }),
        )?;

        if !port_scanner::local_port_available(config.port as u16) {
            return Err(Error {
                kind: ErrorKind::Internal,
                source: eyre!("Port {} is already in use", config.port),
            });
        }

        let prelaunch = resolve_macro_invocation(&self.path_to_instance, "prelaunch");
        if let Some(prelaunch) = prelaunch {
            let res: Result<SpawnResult, Error> = self
                .macro_executor
                .spawn(
                    prelaunch,
                    Vec::new(),
                    CausedBy::System,
                    Box::new(DefaultWorkerOptionGenerator),
                    None,
                    None,
                    Some(self.uuid.clone()),
                )
                .await;

            if let Ok(SpawnResult {
                macro_pid: pid,
                exit_future,
                detach_future,
            }) = res
            {
                self.pid_to_task_entry.lock().await.insert(
                    pid,
                    TaskEntry {
                        pid,
                        name: "prelaunch".to_string(),
                        creation_time: chrono::Utc::now().timestamp(),
                    },
                );
                tokio::select! {
                    _ = exit_future => {
                        info!("Prelaunch script exited");
                    }
                    _ = detach_future => {
                        info!("Prelaunch script requested detach");
                    }
                }
            }
        } else {
            info!(
                "[{}] No prelaunch script found, skipping",
                config.name.clone()
            );
        }

        let jre = if let Some(jre) = &config.java_cmd {
            PathBuf::from(jre)
        } else {
            self.path_to_runtimes
                .join("java")
                .join(format!("jre{}", config.jre_major_version))
                .join(if std::env::consts::OS == "macos" {
                    "Contents/Home/bin"
                } else {
                    "bin"
                })
                .join("java")
        };

        let server_start_command = if config.custom_cmd.is_none()
            || (config.custom_cmd.is_some() && config.custom_cmd.clone().unwrap().is_empty())
        {
            let mut server_start_command = Command::new(&jre);
            let server_start_command = server_start_command
                .arg(format!("-Xmx{}M", config.max_ram))
                .arg(format!("-Xms{}M", config.min_ram))
                .args(
                    &config
                        .cmd_args
                        .iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<&String>>(),
                );

            let server_start_command = match &config.flavour {
                Flavour::Forge { build_version } => {
                    let ForgeBuildVersion(build_version) = build_version
                        .as_ref()
                        .ok_or_else(|| eyre!("Forge version not found"))?;
                    let version_parts: Vec<&str> = config.version.split('.').collect();
                    let major_version: i32 = version_parts[1]
                        .parse()
                        .context("Unable to parse major Minecraft version for Forge")?;

                    if 17 <= major_version {
                        let forge_args = match std::env::consts::OS {
                            "windows" => "win_args.txt",
                            _ => "unix_args.txt",
                        };

                        let mut full_forge_args = std::ffi::OsString::from("@");
                        full_forge_args.push(
                            self.path_to_instance
                                .join("libraries")
                                .join("net")
                                .join("minecraftforge")
                                .join("forge")
                                .join(build_version.as_str())
                                .join(forge_args)
                                .into_os_string()
                                .as_os_str(),
                        );

                        server_start_command.arg(full_forge_args)
                    } else if (7..=16).contains(&major_version) {
                        let files = list_dir(&self.path_to_instance, Some(false))
                            .await
                            .context("Failed to find forge.jar")?;
                        let forge_jar_name = files
                            .iter()
                            .find(|p| {
                                p.extension().unwrap_or_default() == "jar"
                                    && p.file_name()
                                        .unwrap_or_default()
                                        .to_str()
                                        .unwrap_or_default()
                                        .starts_with(format!("forge-{}-", config.version,).as_str())
                            })
                            .ok_or_else(|| eyre!("Failed to find forge.jar"))?;
                        server_start_command
                            .arg("-jar")
                            .arg(&self.path_to_instance.join(forge_jar_name))
                    } else {
                        // 1.5 doesn't work due to JRE issues
                        // 1.4 doesn't work since forge doesn't provide an installer
                        let files = list_dir(&self.path_to_instance, Some(false))
                            .await
                            .context("Failed to find minecraftforge.jar")?;
                        let server_jar_name = files
                            .iter()
                            .find(|p| {
                                p.extension().unwrap_or_default() == "jar"
                                    && p.file_name()
                                        .unwrap_or_default()
                                        .to_str()
                                        .unwrap_or_default()
                                        .starts_with("minecraftforge")
                            })
                            .ok_or_else(|| eyre!("Failed to find minecraftforge.jar"))?;
                        server_start_command
                            .arg("-jar")
                            .arg(&self.path_to_instance.join(server_jar_name))
                    }
                }
                _ => server_start_command
                    .arg("-jar")
                    .arg(&self.path_to_instance.join("server.jar")),
            };

            let server_start_command = server_start_command.arg("nogui");

            server_start_command
        } else {
            let mut args: Vec<String> = config
                .custom_cmd
                .unwrap()
                .split(" ")
                .map(|c| String::from(c))
                .collect();
            let mut server_start_command = Command::new(args.remove(0));
            let server_start_command = server_start_command.args(
                &config
                    .cmd_args
                    .iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<&String>>(),
            );
            server_start_command
        };

        let server_start_command = server_start_command.current_dir(&self.path_to_instance);

        match dont_spawn_terminal(server_start_command)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut proc) => {
                let stdin = proc.stdin.take().ok_or_else(|| {
                    error!(
                        "[{}] Failed to take stdin during startup",
                        config.name.clone()
                    );
                    eyre!("Failed to take stdin during startup")
                })?;
                self.stdin.lock().await.replace(stdin);
                let stdout = proc.stdout.take().ok_or_else(|| {
                    error!(
                        "[{}] Failed to take stdout during startup",
                        config.name.clone()
                    );
                    eyre!("Failed to take stdout during startup")
                })?;
                let stderr = proc.stderr.take().ok_or_else(|| {
                    error!(
                        "[{}] Failed to take stderr during startup",
                        config.name.clone()
                    );
                    eyre!("Failed to take stderr during startup")
                })?;
                *self.process.lock().await = Some(proc);
                tokio::task::spawn({
                    let mut __self = self.clone();
                    let event_broadcaster = __self.event_broadcaster.clone();
                    let uuid = __self.uuid.clone();
                    let name = config.name.clone();
                    let players_manager = __self.players_manager.clone();
                    async move {
                        let mut did_start = false;

                        let mut stdout_reader = BufReader::new(stdout);
                        let mut stderr_reader = BufReader::new(stderr);

                        loop {
                            let (line_res, is_stdout) = tokio::select!(
                                line_res = async {
                                    let mut line = Vec::new();
                                    match stdout_reader.read_until(b'\n', &mut line).await {
                                        Ok(0) => return Ok(None),
                                        Err(e) => return Err(e),
                                        Ok(_) => {}

                                    };
                                    Ok(Some(line))
                                } => {
                                    (line_res, true)
                                },
                                line_res = async {
                                    let mut line = Vec::new();
                                    match stderr_reader.read_until(b'\n', &mut line).await {
                                        Ok(0) => return Ok(None),
                                        Err(e) => return Err(e),
                                        Ok(_) => {}
                                    };
                                    Ok(Some(line))
                                } => {
                                    (line_res, false)
                                }
                            );
                            let _ = line_res.as_ref().map_err(|e| {
                                error!("[{}] Failed to read from stdout/stderr: {}", name, e);
                            });

                            if let Ok(line) = line_res {
                                if let Some(line) = line {
                                    let line = String::from_utf8_lossy(&line).to_string();
                                    if !is_stdout {
                                        // info!("[{}] {}", name, line);
                                        warn!("[{}] {}", name, line);
                                    }
                                    event_broadcaster.send(Event {
                                        event_inner: EventInner::InstanceEvent(InstanceEvent {
                                            instance_uuid: uuid.clone(),
                                            instance_event_inner:
                                                InstanceEventInner::InstanceOutput {
                                                    message: line.clone(),
                                                },
                                            instance_name: name.clone(),
                                        }),
                                        details: "".to_string(),
                                        snowflake: Snowflake::default(),
                                        caused_by: CausedBy::System,
                                    });

                                    if parse_server_started(&line) && !did_start {
                                        did_start = true;
                                        __self
                                            .state
                                            .lock()
                                            .await
                                            .try_transition(
                                                StateAction::InstanceStart,
                                                Some(&|state| {
                                                    event_broadcaster.send(Event {
                                                event_inner: EventInner::InstanceEvent(
                                                    InstanceEvent {
                                                        instance_name: config.name.clone(),
                                                        instance_uuid: __self.uuid.clone(),
                                                        instance_event_inner:
                                                            InstanceEventInner::StateTransition {
                                                                to: state,
                                                            },
                                                    },
                                                ),
                                                snowflake: Snowflake::default(),
                                                details: "Starting server".to_string(),
                                                caused_by: cause_by.clone(),
                                            });
                                                }),
                                            )
                                            .unwrap();
                                        info!("[{}] Instance started", name);

                                        if let (Some(true), Some(rcon_psw), Some(rcon_port)) = {
                                            let lock = __self.configurable_manifest.lock().await;

                                            let a = lock
                                                .get_unique_setting_key("enable-rcon")
                                                .and_then(|v| {
                                                    v.get_value().map(|v| v.try_as_boolean().ok())
                                                })
                                                .flatten();

                                            let b = lock
                                                .get_unique_setting_key("rcon.password")
                                                .and_then(|v| {
                                                    v.get_value().map(|v| v.try_as_string().ok())
                                                })
                                                .flatten()
                                                .cloned();

                                            let c = lock
                                                .get_unique_setting_key("rcon.port")
                                                .and_then(|v| {
                                                    v.get_value()
                                                        .map(|v| v.try_as_unsigned_integer().ok())
                                                })
                                                .flatten();
                                            (a, b, c)
                                        } {
                                            let max_retry = 3;
                                            for i in 0..max_retry {
                                                let rcon =
                                                <rcon::Connection<tokio::net::TcpStream>>::builder(
                                                )
                                                .enable_minecraft_quirks(true)
                                                .connect(
                                                    &format!("localhost:{}", rcon_port),
                                                    &rcon_psw,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    warn!(
                                                    "[{}] Failed to connect to RCON: {}, retry {}/{}",
                                                    config.name,
                                                    e, i, max_retry
                                                );
                                                    e
                                                });
                                                if let Ok(rcon) = rcon {
                                                    info!("[{}] Connected to RCON", config.name);
                                                    __self.rcon_conn.lock().await.replace(rcon);
                                                    break;
                                                }
                                                tokio::time::sleep(Duration::from_secs(
                                                    2_u64.pow(i),
                                                ))
                                                .await;
                                            }
                                        } else {
                                            warn!("RCON is not enabled or misconfigured, skipping");
                                            __self.rcon_conn.lock().await.take();
                                        }
                                    }
                                    if let Some(system_msg) = parse_system_msg(&line) {
                                        let _ = event_broadcaster.send(Event {
                                            event_inner: EventInner::InstanceEvent(InstanceEvent {
                                                instance_uuid: uuid.clone(),
                                                instance_event_inner:
                                                    InstanceEventInner::SystemMessage {
                                                        message: line,
                                                    },
                                                instance_name: name.clone(),
                                            }),
                                            details: "".to_string(),
                                            snowflake: Snowflake::default(),
                                            caused_by: CausedBy::System,
                                        });
                                        if let Some(player_name) = parse_player_joined(&system_msg)
                                        {
                                            players_manager.lock().await.add_player(
                                                MinecraftPlayer {
                                                    name: player_name.clone(),
                                                    uuid: name_to_uuid(&player_name).await,
                                                },
                                                __self.name().await,
                                            );
                                        } else if let Some(player_name) =
                                            parse_player_left(&system_msg)
                                        {
                                            players_manager
                                                .lock()
                                                .await
                                                .remove_by_name(&player_name, __self.name().await);
                                        }
                                    } else if let Some(PlayerMessage { player, message }) =
                                        parse_player_msg(&line)
                                    {
                                        event_broadcaster.send(Event {
                                            event_inner: EventInner::InstanceEvent(InstanceEvent {
                                                instance_uuid: uuid.clone(),
                                                instance_event_inner:
                                                    InstanceEventInner::PlayerMessage {
                                                        player,
                                                        player_message: message,
                                                    },
                                                instance_name: name.clone(),
                                            }),
                                            details: "".to_string(),
                                            snowflake: Snowflake::default(),
                                            caused_by: CausedBy::System,
                                        });
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                        info!("Instance {} process shutdown", name);
                        __self
                            .state
                            .lock()
                            .await
                            .try_transition(
                                StateAction::InstanceStop,
                                Some(&|state| {
                                    event_broadcaster.send(Event {
                                        event_inner: EventInner::InstanceEvent(InstanceEvent {
                                            instance_name: config.name.clone(),
                                            instance_uuid: __self.uuid.clone(),
                                            instance_event_inner:
                                                InstanceEventInner::StateTransition { to: state },
                                        }),
                                        snowflake: Snowflake::default(),
                                        details: "Instance stopping as server process exited"
                                            .to_string(),
                                        caused_by: cause_by.clone(),
                                    });
                                }),
                            )
                            .unwrap();
                        __self.players_manager.lock().await.clear(name);
                        __self.rcon_conn.lock().await.take();
                    }
                });
                self.config.lock().await.has_started = true;
                self.write_config_to_file().await?;
                let instance_uuid = self.uuid.clone();
                let mut rx = self.event_broadcaster.subscribe();

                if block {
                    while let Ok(event) = rx.recv().await {
                        if let EventInner::InstanceEvent(InstanceEvent {
                            instance_uuid: event_instance_uuid,
                            instance_event_inner: InstanceEventInner::StateTransition { to },
                            ..
                        }) = event.event_inner
                        {
                            if instance_uuid == event_instance_uuid {
                                if to == State::Running {
                                    return Ok(()); // Instance started successfully
                                } else if to == State::Stopped {
                                    return Err(eyre!(
                                        "Instance exited unexpectedly before starting"
                                    )
                                    .into());
                                }
                            }
                        }
                    }
                    Err(eyre!("Sender shutdown").into())
                } else {
                    Ok(())
                }
            }
            Err(e) => {
                error!("Failed to start server, {}", e);
                self.state
                    .lock()
                    .await
                    .try_transition(
                        StateAction::InstanceStop,
                        Some(&|state| {
                            self.event_broadcaster.send(Event {
                                event_inner: EventInner::InstanceEvent(InstanceEvent {
                                    instance_name: config.name.clone(),
                                    instance_uuid: self.uuid.clone(),
                                    instance_event_inner: InstanceEventInner::StateTransition {
                                        to: state,
                                    },
                                }),
                                snowflake: Snowflake::default(),
                                details: "Starting server".to_string(),
                                caused_by: cause_by.clone(),
                            });
                        }),
                    )
                    .unwrap();
                Err(e).context("Failed to start server")?;
                unreachable!();
            }
        }
    }
    async fn stop(&self, cause_by: CausedBy, block: bool) -> Result<(), Error> {
        let config = self.config.lock().await.clone();

        self.state.lock().await.try_transition(
            StateAction::UserStop,
            Some(&|state| {
                self.event_broadcaster.send(Event {
                    event_inner: EventInner::InstanceEvent(InstanceEvent {
                        instance_name: config.name.clone(),
                        instance_uuid: self.uuid.clone(),
                        instance_event_inner: InstanceEventInner::StateTransition { to: state },
                    }),
                    snowflake: Snowflake::default(),
                    details: "Stopping server".to_string(),
                    caused_by: cause_by.clone(),
                });
            }),
        )?;
        let name = config.name.clone();
        let _uuid = self.uuid.clone();
        self.stdin
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| {
                error!("[{}] Failed to stop instance: stdin not available", name);
                eyre!("Failed to stop instance: stdin not available")
            })?
            .write_all(b"stop\n")
            .await
            .context("Failed to write to stdin")
            .map_err(|e| {
                error!("[{}] Failed to stop instance: {}", name, e);
                e
            })?;
        self.rcon_conn.lock().await.take();
        let mut rx = self.event_broadcaster.subscribe();
        let instance_uuid = self.uuid.clone();

        if block {
            while let Ok(event) = rx.recv().await {
                if let EventInner::InstanceEvent(InstanceEvent {
                    instance_uuid: event_instance_uuid,
                    instance_event_inner: InstanceEventInner::StateTransition { to },
                    ..
                }) = event.event_inner
                {
                    if instance_uuid == event_instance_uuid && to == State::Stopped {
                        return Ok(());
                    }
                }
            }
            Err(eyre!("Sender shutdown").into())
        } else {
            Ok(())
        }
    }

    async fn restart(&self, caused_by: CausedBy, block: bool) -> Result<(), Error> {
        if block {
            self.stop(caused_by.clone(), block).await?;
            self.start(caused_by, block).await
        } else {
            self.state
                .lock()
                .await
                .try_new_state(StateAction::UserStop, None)?;

            let mut __self = self.clone();
            tokio::task::spawn(async move {
                __self.stop(caused_by.clone(), true).await.unwrap();
                __self.start(caused_by, block).await.unwrap()
            });
            Ok(())
        }
    }

    async fn kill(&self, _cause_by: CausedBy) -> Result<(), Error> {
        let config = self.config.lock().await.clone();

        if self.state().await == State::Stopped {
            warn!("[{}] Instance is already stopped", config.name.clone());
            return Err(eyre!("Instance is already stopped").into());
        }
        if let Some(process) = self.process.lock().await.as_mut() {
            process
                .kill()
                .await
                .context("Failed to kill process")
                .map_err(|e| {
                    error!("[{}] Failed to kill instance: {}", config.name.clone(), e);
                    e
                })?;
        }
        {
            error!(
                "[{}] Process not available, assuming instance is stopped",
                config.name.clone()
            );
            *self.state.lock().await = State::Stopped;
            self.event_broadcaster
                .send(Event::new_instance_state_transition(
                    self.uuid.clone(),
                    config.name.clone(),
                    State::Stopped,
                ));
            Err(eyre!("Process not available, assuming instance is stopped"))?;
        }
        Ok(())
    }

    async fn state(&self) -> State {
        *self.state.lock().await
    }

    async fn send_command(&self, command: &str, cause_by: CausedBy) -> Result<(), Error> {
        let config = self.config.lock().await.clone();
        if self.state().await == State::Stopped {
            Err(eyre!("Instance is stopped").into())
        } else {
            match self.stdin.lock().await.as_mut() {
                Some(stdin) => match {
                    if command == "stop" {
                        self.state.lock().await.try_new_state(
                            StateAction::UserStop,
                            Some(&|state| {
                                self.event_broadcaster.send(Event {
                                    event_inner: EventInner::InstanceEvent(InstanceEvent {
                                        instance_name: config.name.clone(),
                                        instance_uuid: self.uuid.clone(),
                                        instance_event_inner: InstanceEventInner::StateTransition {
                                            to: state,
                                        },
                                    }),
                                    snowflake: Snowflake::default(),
                                    details: "Starting server".to_string(),
                                    caused_by: cause_by.clone(),
                                });
                            }),
                        )?;
                    }
                    stdin.write_all(format!("{}\n", command).as_bytes()).await
                } {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        warn!(
                            "[{}] Failed to send command to instance: {}",
                            config.name.clone(),
                            e
                        );
                        Err(e).context("Failed to send command to instance")?;
                        unreachable!()
                    }
                },
                None => {
                    let err_msg =
                        "Failed to write to stdin because stdin is None. Please report this bug.";
                    error!("[{}] {}", config.name.clone(), err_msg);
                    Err(eyre!(err_msg).into())
                }
            }
        }
    }
    async fn monitor(&self) -> MonitorReport {
        let mut sys = self.system.lock().await;
        sys.refresh_memory();
        if let Some(pid) = self.process.lock().await.as_ref().and_then(|p| p.id()) {
            sys.refresh_process(Pid::from_u32(pid));
            let proc = (*sys).process(Pid::from_u32(pid));
            if let Some(proc) = proc {
                let cpu_usage =
                    sys.process(Pid::from_u32(pid)).unwrap().cpu_usage() / sys.cpus().len() as f32;

                let memory_usage = proc.memory();
                let disk_usage = proc.disk_usage();
                let start_time = proc.start_time();
                MonitorReport {
                    memory_usage: Some(memory_usage),
                    disk_usage: Some(disk_usage.into()),
                    cpu_usage: Some(cpu_usage),
                    start_time: Some(start_time),
                }
            } else {
                MonitorReport::default()
            }
        } else {
            MonitorReport::default()
        }
    }
}
