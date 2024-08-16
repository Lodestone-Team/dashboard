#![allow(clippy::comparison_chain, clippy::type_complexity)]

use crate::error::ErrorKind;
use crate::event_broadcaster::EventBroadcaster;
use crate::handlers::extension::get_extension_routes;
use crate::migration::migrate;
use crate::prelude::{
    init_app_state, init_paths, lodestone_path, path_to_global_settings, path_to_stores,
    path_to_tmp, path_to_users, VERSION,
};
use crate::traits::t_configurable::GameType;
use crate::traits::t_server::State;
use crate::{
    db::write::write_event_to_db_task,
    global_settings::GlobalSettingsData,
    handlers::{
        checks::get_checks_routes, core_info::get_core_info_routes, events::get_events_routes,
        gateway::get_gateway_routes, global_fs::get_global_fs_routes,
        global_settings::get_global_settings_routes, instance::*,
        instance_config::get_instance_config_routes, instance_fs::get_instance_fs_routes,
        instance_macro::get_instance_macro_routes, instance_players::get_instance_players_routes,
        instance_server::get_instance_server_routes,
        instance_setup_configs::get_instance_setup_config_routes, monitor::get_monitor_routes,
        playitgg::get_playitgg_routes, setup::get_setup_route, system::get_system_routes,
        users::get_user_routes,
    },
    util::rand_alphanumeric,
};

use auth::user::UsersManager;
use axum::Router;

use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use color_eyre::eyre::Context;
use color_eyre::Report;
use dashmap::DashMap;
use error::Error;
use events::{CausedBy, Event};
use futures::Future;
use global_settings::GlobalSettings;
use implementations::{generic, minecraft};
use macro_executor::MacroExecutor;
use playitgg::utils::is_valid_secret_key;
use port_manager::PortManager;
use prelude::GameInstance;
use reqwest::{header, Method};
use ringbuffer::{AllocRingBuffer, RingBufferWrite};

use fs3::FileExt;
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, Pool};
use std::sync::atomic::AtomicBool;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use sysinfo::{CpuExt, SystemExt};
use tokio::{
    select,
    sync::{broadcast::error::RecvError, Mutex, RwLock},
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, EnvFilter};
use traits::{t_configurable::TConfigurable, t_server::MonitorReport, t_server::TServer};
use types::{DotLodestoneConfig, InstanceUuid};
use uuid::Uuid;

pub mod auth;
mod command_console;
pub mod db;
mod deno_ops;
mod docker_bridge;
pub mod error;
mod event_broadcaster;
mod events;
mod extension;
pub mod global_settings;
mod handlers;
pub mod implementations;
pub mod macro_executor;
mod migration;
mod output_types;
pub mod playitgg;
mod port_manager;
pub mod prelude;
pub mod tauri_export;
mod traits;
pub mod types;
pub mod util;
use handlers::global_fs::DownloadableFile;

#[derive(Clone)]
pub struct AppState {
    instances: Arc<DashMap<InstanceUuid, GameInstance>>,
    users_manager: Arc<RwLock<UsersManager>>,
    events_buffer: Arc<Mutex<AllocRingBuffer<Event>>>,
    console_out_buffer: Arc<Mutex<HashMap<InstanceUuid, AllocRingBuffer<Event>>>>,
    monitor_buffer: Arc<Mutex<HashMap<InstanceUuid, AllocRingBuffer<MonitorReport>>>>,
    event_broadcaster: EventBroadcaster,
    uuid: String,
    up_since: i64,
    global_settings: Arc<Mutex<GlobalSettings>>,
    system: Arc<Mutex<sysinfo::System>>,
    port_manager: Arc<Mutex<PortManager>>,
    first_time_setup_key: Arc<Mutex<Option<String>>>,
    playitgg_key: Arc<Mutex<Option<String>>>,
    download_urls: Arc<Mutex<HashMap<String, DownloadableFile>>>,
    macro_executor: MacroExecutor,
    sqlite_pool: sqlx::SqlitePool,
    docker_bridge: docker_bridge::DockerBridge,
    playit_keep_running: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl AppState {
    /// Kill all instances
    pub async fn cleanup(&mut self) {
        for instance in self.instances.iter() {
            let instance = instance.value().clone();
            tokio::task::spawn(async move {
                if let Err(e) = instance.kill(CausedBy::System).await {
                    error!("Failed to kill instance: {}", e);
                };
            });
        }
    }

    pub async fn clear_console_buffer(&self, uuid: &InstanceUuid) {
        self.console_out_buffer.lock().await.remove(uuid);
    }
}

async fn restore_instances(
    instances_path: &Path,
    event_broadcaster: EventBroadcaster,
    macro_executor: MacroExecutor,
) -> Result<DashMap<InstanceUuid, GameInstance>, Error> {
    let ret: DashMap<InstanceUuid, GameInstance> = DashMap::new();

    for entry in instances_path
        .read_dir()
        .context("Failed to read instances directory")?
    {
        let path = match entry {
            Ok(v) => v.path(),
            Err(e) => {
                error!("Error while restoring instance, failed to read instance directory : {e}");
                continue;
            }
        };
        let dot_lodestone_config_file = match std::fs::File::open(path.join(".lodestone_config")) {
            Ok(v) => v,
            Err(e) => {
                error!("Error while restoring instance {}, failed to read .lodestone_config file : {e}", path.display());
                continue;
            }
        };
        let dot_lodestone_config: DotLodestoneConfig = match serde_json::from_reader(
            dot_lodestone_config_file,
        ) {
            Ok(v) => v,
            Err(e) => {
                error!("Error while restoring instance {}, failed to parse .lodestone_config file : {e}", path.display());
                continue;
            }
        };

        debug!("restoring instance: {}", path.display());
        let uuid_instance: (InstanceUuid, GameInstance) = match dot_lodestone_config.game_type() {
            GameType::MinecraftJava => {
                let instance = match minecraft::MinecraftInstance::restore(
                    path.to_owned(),
                    dot_lodestone_config.clone(),
                    event_broadcaster.clone(),
                    macro_executor.clone(),
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        error!(
                            "Error while restoring Minecraft Java instance {} : {e}",
                            path.display()
                        );
                        continue;
                    }
                };
                debug!("Restored Minecraft Java instance successfully");
                (dot_lodestone_config.uuid().to_owned(), instance.into())
            }
            GameType::Generic => {
                let instance = match generic::GenericInstance::restore(
                    path.to_owned(),
                    dot_lodestone_config.clone(),
                    event_broadcaster.clone(),
                    macro_executor.clone(),
                )
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        error!(
                            "Error while restoring atom instance {} : {e}",
                            path.display()
                        );
                        continue;
                    }
                };
                debug!("Restored Generic instance successfully");
                (dot_lodestone_config.uuid().to_owned(), instance.into())
            }
            GameType::MinecraftBedrock => todo!(),
        };
        let uuid = uuid_instance.0;
        let instance = uuid_instance.1;
        if ret.contains_key(&uuid) {
            warn!("UUID {} is repeated.", uuid.to_string());
        }
        ret.insert(uuid, instance);
    }
    Ok(ret)
}

fn setup_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    let file_appender =
        tracing_appender::rolling::hourly(lodestone_path().join("log"), "lodestone_core.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // set up a subscriber that logs formatted tracing events to stdout without colors without setting it as the default

    #[cfg(debug_assertions)]
    {
        let fmt_layer_stdout = tracing_subscriber::fmt::layer()
            // Use a more compact, abbreviated log format
            .compact()
            // Display source code file paths
            .with_file(true)
            // Display source code line numbers
            .with_line_number(true)
            // Display the thread ID an event was recorded on
            .with_thread_ids(false)
            // Don't display the event's target (module path)
            .with_target(true)
            .with_writer(std::io::stdout);
        let fmt_layer_file = tracing_subscriber::fmt::layer()
            // Use a more compact, abbreviated log format
            .compact()
            // Display source code file paths
            .with_file(true)
            // Display source code line numbers
            .with_line_number(true)
            // Display the thread ID an event was recorded on
            .with_thread_ids(false)
            // Don't display the event's target (module path)
            .with_target(true)
            .with_ansi(false)
            .with_writer(non_blocking);

        tracing_subscriber::registry()
            .with(fmt_layer_stdout)
            .with(fmt_layer_file)
            .with(EnvFilter::from("lodestone_core=debug"))
            .init();
    }

    #[cfg(not(debug_assertions))]
    {
        let fmt_layer_stdout = tracing_subscriber::fmt::layer()
            // Use a more compact, abbreviated log format
            .compact()
            // Display source code file paths
            .with_file(false)
            // Display source code line numbers
            .with_line_number(false)
            // Display the thread ID an event was recorded on
            .with_thread_ids(false)
            // Don't display the event's target (module path)
            .with_target(false)
            .with_writer(std::io::stdout)
            .with_filter(EnvFilter::from("lodestone_core=info"));

        let fmt_layer_file = tracing_subscriber::fmt::layer()
            // Use a more compact, abbreviated log format
            .compact()
            // Display source code file paths
            .with_file(true)
            // Display source code line numbers
            .with_line_number(true)
            // Display the thread ID an event was recorded on
            .with_thread_ids(false)
            // Don't display the event's target (module path)
            .with_target(true)
            .with_ansi(false)
            .with_writer(non_blocking)
            .with_filter(EnvFilter::from("lodestone_core=debug"));

        tracing_subscriber::registry()
            // .with(ErrorLayer::default())
            .with(fmt_layer_stdout)
            .with(fmt_layer_file)
            .init();
    }

    _guard
}

fn output_sys_info() {
    info!("lodestone_core version {}", VERSION.with(|v| v.clone()));
    // output system info
    info!("System info:");
    info!("  OS: {}", std::env::consts::OS);
    info!("  Arch: {}", std::env::consts::ARCH);
    // CPU and RAM info
    let sys = sysinfo::System::new_all();
    let cpu_name = sys
        .cpus()
        .first()
        .map_or_else(|| "Unknown CPU", |v| v.brand());
    let ram = sys.total_memory();
    info!("  CPU: {cpu_name}");
    info!(
        "  RAM: {ram} bytes ({ram_gb} GB)",
        ram = ram,
        ram_gb = ram / 1024 / 1024 / 1024
    );
}

async fn check_for_core_update() {
    #[derive(serde::Deserialize)]
    pub struct Release {
        pub tag_name: String,
    }
    pub async fn get_latest_release() -> Result<Version, Report> {
        let release_url =
            "https://api.github.com/repos/Lodestone-Team/lodestone_core/releases/latest";
        let client = reqwest::Client::new();

        let response = client
            .get(release_url)
            .header("User-Agent", "lodestone_cli")
            .send()
            .await?;
        response.error_for_status_ref()?;

        let release: Release = response.json().await?;
        // tag_name is prefixed with a v, so we need to remove it to get the version
        let tag_name = release.tag_name.trim_start_matches('v');
        let latest_version = Version::parse(tag_name)?;
        Ok(latest_version)
    }

    let latest_version = match get_latest_release().await {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to get latest release: {}", e);
            return;
        }
    };
    let current_version = VERSION.with(|v| v.clone());

    if current_version >= latest_version {
        info!("lodestone_core is up to date");
        return;
    }

    if latest_version.pre.is_empty() {
        // we don't want to update to a pre-release
        info!(
            "A new version of lodestone_core is available: {}",
            latest_version
        );
        info!(
            "Read how to update here: {url}",
            url = "https://github.com/Lodestone-Team/lodestone/wiki/Updating"
        );
    } else {
        info!(
            "A new pre-release version of lodestone_core is available: {}",
            latest_version
        );
        info!(
            "Read how to update here: {url}",
            url = "https://github.com/Lodestone-Team/lodestone/wiki/Updating"
        );
    }
}

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, default_value = "false")]
    pub is_cli: bool,
    #[arg(long, default_value = "false")]
    pub is_desktop: bool,
    #[arg(short, long)]
    pub lodestone_path: Option<PathBuf>,
}

pub async fn run(
    args: Args,
) -> Result<
    (
        impl Future<Output = ()>,
        AppState,
        tracing_appender::non_blocking::WorkerGuard,
        tokio::sync::oneshot::Sender<()>,
    ),
    Error,
> {
    let _ = color_eyre::install().map_err(|e| {
        error!("Failed to install color_eyre: {}", e);
    });
    let lodestone_path = if let Some(path) = args.lodestone_path {
        path
    } else {
        PathBuf::from(match std::env::var("LODESTONE_PATH") {
            Ok(v) => v,
            Err(_) => {
                match home::home_dir()
                    .unwrap_or_else(|| {
                        std::env::current_dir()
                            .expect("what kinda os are you running lodestone on???")
                    })
                    .join(".lodestone")
                    .to_str()
                {
                    Some(p) => p.to_string(),
                    None => {
                        return Err(Error {
                            kind: ErrorKind::Internal,
                            source: Report::msg("Failed to get home dir"),
                        })
                    }
                }
            }
        })
    };
    init_paths(lodestone_path.clone());
    info!("Lodestone path: {}", lodestone_path.display());
    std::env::set_current_dir(&lodestone_path).map_err(|_| Error {
        kind: ErrorKind::Internal,
        source: Report::msg("Failed to set current dir"),
    })?;
    let guard = setup_tracing();
    if args.is_desktop {
        info!("Lodestone Core running in Tauri");
    }
    if !args.is_cli && !args.is_desktop {
        warn!("Lodestone Core is not meant to be run as a standalone program. Please use Lodestone CLI instead.");
        warn!("Download it here: https://github.com/Lodestone-Team/lodestone_cli")
    }
    check_for_core_update().await;
    output_sys_info();

    let lockfile_path = lodestone_path.join("lodestone.lock");
    let lock_file =
        std::fs::File::create(lockfile_path.as_path()).expect("failed to create lockfile");
    if lock_file.try_lock_exclusive().is_err() {
        return Err(Error {
            kind: ErrorKind::Internal,
            source: Report::msg("Another instance of lodestone is running"),
        });
    }

    let _ = migrate(&lodestone_path).map_err(|e| {
        error!("Error while migrating lodestone: {}. Lodestone will still start, but one or more instance may be in an erroneous state", e);
    });

    let path_to_instances = lodestone_path.join("instances");

    let (tx, _rx) = EventBroadcaster::new(512);

    let mut users_manager = UsersManager::new(tx.clone(), HashMap::new(), path_to_users().clone());

    users_manager.load_users().await?;

    let mut global_settings = GlobalSettings::new(
        path_to_global_settings().clone(),
        tx.clone(),
        GlobalSettingsData::default(),
    );

    global_settings.load_from_file().await?;

    let first_time_setup_key = if !users_manager.as_ref().iter().any(|(_, user)| user.is_owner) {
        let key = rand_alphanumeric(16);
        // log the first time setup key in green so it's easy to find
        info!(
            "First time setup key: {}",
            ansi_term::Color::Green.paint(key.clone())
        );
        info!("This is a one-time, in-memory randomly generated key that allows you to create the owner account.");
        info!(
            "{}",
            ansi_term::Color::Red.paint("DO NOT SHARE THIS KEY WITH ANYONE!")
        );
        Some(key)
    } else {
        None
    };

    let playitgg_key = if let Ok(playitgg_file) =
        tokio::fs::read_to_string(lodestone_path.join("playit.toml")).await
    {
        let toml_data: toml::Table = toml::from_str(&playitgg_file).unwrap();
        if let Some(res) = toml_data["secret_key"].as_str() {
            if is_valid_secret_key(res.to_string()).await {
                println!("Validated playitgg key...");
                Some(res.to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let macro_executor = MacroExecutor::new(tx.clone(), tokio::runtime::Handle::current());
    let instances = restore_instances(&path_to_instances, tx.clone(), macro_executor.clone())
        .await
        .map_err(|_| Error {
            kind: ErrorKind::Internal,
            source: Report::msg("failed to restore instances"),
        })?;

    let mut allocated_ports = HashSet::new();
    for instance_entry in instances.iter() {
        allocated_ports.insert(instance_entry.value().port().await);
    }
    let shared_state = AppState {
        instances: Arc::new(instances),
        users_manager: Arc::new(RwLock::new(users_manager)),
        events_buffer: Arc::new(Mutex::new(AllocRingBuffer::with_capacity(512))),
        console_out_buffer: Arc::new(Mutex::new(HashMap::new())),
        monitor_buffer: Arc::new(Mutex::new(HashMap::new())),
        event_broadcaster: tx.clone(),
        uuid: Uuid::new_v4().to_string(),
        up_since: chrono::Utc::now().timestamp(),
        port_manager: Arc::new(Mutex::new(PortManager::new(allocated_ports))),
        first_time_setup_key: Arc::new(Mutex::new(first_time_setup_key)),
        playitgg_key: Arc::new(Mutex::new(playitgg_key)),
        system: Arc::new(Mutex::new(sysinfo::System::new_all())),
        download_urls: Arc::new(Mutex::new(HashMap::new())),
        playit_keep_running: Arc::new(Mutex::new(None)),
        global_settings: Arc::new(Mutex::new(global_settings)),
        macro_executor,
        sqlite_pool: Pool::connect_with(
            SqliteConnectOptions::from_str(&format!(
                "sqlite://{}/data.db",
                path_to_stores().display()
            ))
            .map_err(|_| Error {
                kind: ErrorKind::Internal,
                source: Report::msg("Failed to create sqlite connection options"),
            })?
            .create_if_missing(true),
        )
        .await
        .map_err(|_| Error {
            kind: ErrorKind::Internal,
            source: Report::msg("Failed to create sqlite pool"),
        })?,
        docker_bridge: docker_bridge::DockerBridge::new(
            tx.clone(),
            path_to_stores().join("docker_bridge.json"),
        )
        .await
        .unwrap(),
    };

    command_console::init(shared_state.clone());
    init_app_state(shared_state.clone());

    for mut entry in shared_state.instances.iter_mut() {
        let instance = entry.value_mut();
        if instance.auto_start().await {
            info!("Auto starting instance {}", instance.name().await);
            if let Err(e) = instance.start(CausedBy::System, false).await {
                error!(
                    "Failed to start instance {}: {:?}",
                    entry.value().name().await,
                    e
                );
            }
        }
    }

    let event_buffer_task = {
        let event_buffer = shared_state.events_buffer.clone();
        let console_out_buffer = shared_state.console_out_buffer.clone();
        let mut event_receiver = tx.subscribe();
        async move {
            loop {
                let result = event_receiver.recv().await;
                if let Err(error) = result.as_ref() {
                    match error {
                        RecvError::Lagged(_) => {
                            warn!("Event buffer lagged");
                            continue;
                        }
                        RecvError::Closed => {
                            warn!("Event buffer closed");
                            break;
                        }
                    }
                }
                let event = result.unwrap();
                if event.is_event_console_message() {
                    console_out_buffer
                        .lock()
                        .await
                        .entry(event.get_instance_uuid().unwrap())
                        .or_insert_with(|| AllocRingBuffer::with_capacity(1024))
                        .push(event.clone());
                } else {
                    event_buffer.lock().await.push(event.clone());
                }
            }
        }
    };

    let write_to_db_task = write_event_to_db_task(tx.subscribe(), shared_state.sqlite_pool.clone());

    let monitor_report_task = {
        let monitor_buffer = shared_state.monitor_buffer.clone();
        let instances = shared_state.instances.clone();
        async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                for entry in instances.iter() {
                    let report = entry.value().monitor().await;
                    monitor_buffer
                        .lock()
                        .await
                        .entry(entry.key().to_owned())
                        .or_insert_with(|| AllocRingBuffer::with_capacity(64))
                        .push(report);
                }
                interval.tick().await;
            }
        }
    };

    let tls_config_result = RustlsConfig::from_pem_file(
        lodestone_path.join("tls").join("cert.pem"),
        lodestone_path.join("tls").join("key.pem"),
    )
    .await;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    Ok((
        {
            let shared_state = shared_state.clone();
            async move {
                let cors = CorsLayer::new()
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PATCH,
                        Method::PUT,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allow_headers([header::ORIGIN, header::CONTENT_TYPE, header::AUTHORIZATION]) // Note I can't find X-Auth-Token but it was in the original rocket version, hope it's fine
                    .allow_origin(Any);

                let trace = TraceLayer::new_for_http();

                let api_routes = Router::new()
                    .merge(get_events_routes(shared_state.clone()))
                    .merge(get_instance_setup_config_routes(shared_state.clone()))
                    .merge(get_instance_server_routes(shared_state.clone()))
                    .merge(get_instance_config_routes(shared_state.clone()))
                    .merge(get_instance_players_routes(shared_state.clone()))
                    .merge(get_instance_routes(shared_state.clone()))
                    .merge(get_system_routes(shared_state.clone()))
                    .merge(get_checks_routes(shared_state.clone()))
                    .merge(get_user_routes(shared_state.clone()))
                    .merge(get_core_info_routes(shared_state.clone()))
                    .merge(get_setup_route(shared_state.clone()))
                    .merge(get_monitor_routes(shared_state.clone()))
                    .merge(get_instance_macro_routes(shared_state.clone()))
                    .merge(get_instance_fs_routes(shared_state.clone()))
                    .merge(get_global_fs_routes(shared_state.clone()))
                    .merge(get_global_settings_routes(shared_state.clone()))
                    .merge(get_gateway_routes(shared_state.clone()))
                    .merge(get_extension_routes(shared_state.clone()))
                    .merge(get_playitgg_routes(shared_state.clone()))
                    .layer(cors)
                    .layer(trace);
                let app = Router::new().nest("/api/v1", api_routes);
                #[allow(unused_variables, unused_mut)]
                let mut port = 16_662_u16;
                #[cfg(not(debug_assertions))]
                if port_scanner::scan_port(port) {
                    error!("Port {port} is already in use, exiting");
                    std::process::exit(1);
                }
                #[cfg(debug_assertions)]
                while port_scanner::scan_port(port) {
                    debug!("Port {port} is already in use, trying next port");
                    port += 1;
                }
                let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], port));
                let axum_server_handle = axum_server::Handle::new();
                tokio::spawn({
                    let axum_server_handle = axum_server_handle.clone();
                    async move {
                        match tls_config_result {
                            Ok(config) => {
                                info!("TLS enabled");
                                info!("Lodestone Core live on {addr}");
                                info!("Note that Lodestone Core does not host the web dashboard itself. Please visit https://www.lodestone.cc for setup instructions.");
                                axum_server::bind_rustls(addr, config)
                                    .handle(axum_server_handle)
                                    .serve(app.into_make_service())
                                    .await
                            }
                            Err(e) => {
                                warn!("Invalid TLS config : {e}, using HTTP");
                                info!("Lodestone Core live on {addr}");
                                info!("Note that Lodestone Core does not host the web dashboard itself. Please visit https://www.lodestone.cc for setup instructions.");
                                axum_server::bind(addr)
                                    .handle(axum_server_handle)
                                    .serve(app.into_make_service())
                                    .await
                            }
                        }
                        .unwrap();
                    }
                });
                // capture file into the move block
                let _lock_file = lock_file;
                select! {
                    _ = write_to_db_task => info!("Write to db task exited"),
                    _ = event_buffer_task => info!("Event buffer task exited"),
                    _ = monitor_report_task => info!("Monitor report task exited"),
                    _ = shutdown_rx => info!("Shutdown signal received"),
                    _ = tokio::signal::ctrl_c() => info!("Ctrl+C received"),
                }
                info!("Shutting down web server");
                axum_server_handle.shutdown();
                info!("Signalling all instances to stop");
                // cleanup
                let mut handles = vec![];
                shared_state.download_urls.lock().await.clear();
                let _ = tokio::fs::remove_dir_all(path_to_tmp()).await.map_err(|e| {
                    error!("Failed to remove tmp dir : {}", e);
                    e
                });
                for entry in shared_state.instances.iter() {
                    let instance = entry.value().clone();
                    match instance.state().await {
                        State::Starting => {
                            let handle = tokio::spawn({
                                let instance = instance.clone();
                                async move {
                                    info!(
                                        "Killing instance that is starting : {}",
                                        instance.uuid().await
                                    );
                                    if let Err(e) = instance.kill(CausedBy::System).await {
                                        error!(
                                        "Failed to stop instance {} : {}. Instance may need manual cleanup",
                                        instance.uuid().await,
                                        e
                                    );
                                    }
                                }
                            });
                            handles.push(handle);
                        }
                        State::Running => {
                            let handle = tokio::spawn({
                                let instance = instance.clone();
                                async move {
                                    if let Err(e) = instance.stop(CausedBy::System, false).await {
                                        error!(
                                        "Failed to stop instance {} : {}. Instance may need manual cleanup",
                                        instance.uuid().await,
                                        e
                                    );
                                    }
                                }
                            });
                            handles.push(handle);
                        }
                        State::Error | State::Stopped | State::Stopping => continue,
                    }
                }
                for handle in handles {
                    let _ = handle.await;
                }
                shared_state.instances.clear();
                shared_state.macro_executor.shutdown_all();
                // exit
                std::process::exit(0);
            }
        },
        shared_state,
        guard,
        shutdown_tx,
    ))
}
