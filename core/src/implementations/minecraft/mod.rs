pub mod configurable;
pub mod fabric;
mod forge;
mod line_parser;
pub mod r#macro;
mod paper;
pub mod player;
mod players_manager;
pub mod server;
pub mod util;
mod vanilla;
pub mod versions;
mod neoforge;

use color_eyre::eyre::{eyre, Context, ContextCompat};
use enum_kinds::EnumKind;
use indexmap::IndexMap;

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use sysinfo::SystemExt;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

use tokio::sync::Mutex;

use ::serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;

use tracing::error;

use tokio;
use ts_rs::TS;

use regex::Regex;

use crate::error::Error;
use crate::event_broadcaster::EventBroadcaster;
use crate::events::{Event, ProgressionEventID};
use crate::implementations::minecraft::neoforge::{get_neoforge_minecraft_versions, NeoforgeVersion};
use crate::macro_executor::{MacroExecutor, MacroPID};
use crate::prelude::path_to_binaries;
use crate::traits::t_configurable::PathBuf;

use crate::traits::t_configurable::manifest::{
    ConfigurableManifest, ConfigurableValue, ConfigurableValueType, SectionManifest,
    SettingManifest, SetupManifest, SetupValue,
};

use crate::traits::t_macro::TaskEntry;
use crate::traits::t_server::State;
use crate::traits::TInstance;
use crate::types::{DotLodestoneConfig, InstanceUuid};
use crate::util::{
    dont_spawn_terminal, download_file, format_byte, format_byte_download, unzip_file_async,
    UnzipOption,
};

use self::configurable::{CmdArgSetting, ServerPropertySetting};
use self::fabric::get_fabric_minecraft_versions;
use self::forge::get_forge_minecraft_versions;
use self::paper::get_paper_minecraft_versions;
use self::players_manager::PlayersManager;
use self::util::{get_jre_url, get_server_jar_url, read_properties_from_path};
use self::vanilla::get_vanilla_minecraft_versions;

#[derive(Debug, Clone, TS, Serialize, Deserialize, PartialEq)]
#[ts(export)]
pub struct FabricLoaderVersion(String);
#[derive(Debug, Clone, TS, Serialize, Deserialize, PartialEq)]
#[ts(export)]
pub struct FabricInstallerVersion(String);
#[derive(Debug, Clone, TS, Serialize, Deserialize, PartialEq)]
#[ts(export)]
pub struct PaperBuildVersion(i64);
#[derive(Debug, Clone, TS, Serialize, Deserialize, PartialEq)]
#[ts(export)]
pub struct ForgeBuildVersion(String);

/// A parameter for constructor of `MinecraftInstance`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, EnumKind)]
#[serde(rename_all = "snake_case")]
#[enum_kind(FlavourKind, derive(Serialize, Deserialize, TS))]
pub enum Flavour {
    Vanilla,
    Fabric {
        loader_version: Option<FabricLoaderVersion>,
        installer_version: Option<FabricInstallerVersion>,
    },
    Paper {
        build_version: Option<PaperBuildVersion>,
    },
    Spigot,
    Forge {
        build_version: Option<ForgeBuildVersion>,
    },
    Neoforge {
        build_version: Option<NeoforgeVersion>,
    }
}

impl From<FlavourKind> for Flavour {
    fn from(kind: FlavourKind) -> Self {
        match kind {
            FlavourKind::Vanilla => Flavour::Vanilla,
            FlavourKind::Fabric => Flavour::Fabric {
                loader_version: None,
                installer_version: None,
            },
            FlavourKind::Paper => Flavour::Paper {
                build_version: None,
            },
            FlavourKind::Spigot => Flavour::Spigot,
            FlavourKind::Forge => Flavour::Forge {
                build_version: None,
            },
            FlavourKind::Neoforge => Flavour::Neoforge {
                build_version: None,
            }
        }
    }
}

impl ToString for Flavour {
    fn to_string(&self) -> String {
        match self {
            Flavour::Vanilla => "vanilla".to_string(),
            Flavour::Fabric { .. } => "fabric".to_string(),
            Flavour::Paper { .. } => "paper".to_string(),
            Flavour::Spigot => "spigot".to_string(),
            Flavour::Forge { .. } => "forge".to_string(),
            Flavour::Neoforge { .. } => "neoforge".to_string(),
        }
    }
}

impl ToString for FlavourKind {
    fn to_string(&self) -> String {
        match self {
            FlavourKind::Vanilla => "vanilla".to_string(),
            FlavourKind::Fabric => "fabric".to_string(),
            FlavourKind::Paper => "paper".to_string(),
            FlavourKind::Spigot => "spigot".to_string(),
            FlavourKind::Forge => "forge".to_string(),
            FlavourKind::Neoforge => "neoforge".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupConfig {
    pub name: String,
    pub version: String,
    pub flavour: Flavour,
    pub port: u32,
    pub cmd_args: Vec<String>,
    pub description: Option<String>,
    pub min_ram: Option<u32>,
    pub max_ram: Option<u32>,
    pub auto_start: Option<bool>,
    pub restart_on_crash: Option<bool>,
    pub backup_period: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestoreConfig {
    pub name: String,
    pub version: String,
    pub flavour: Flavour,
    pub description: String,
    pub cmd_args: Vec<String>,
    pub java_cmd: Option<String>,
    pub port: u32,
    pub min_ram: u32,
    pub max_ram: u32,
    pub auto_start: bool,
    pub restart_on_crash: bool,
    pub backup_period: Option<u32>,
    pub jre_major_version: u64,
    pub has_started: bool,
}
#[allow(dead_code)]
#[derive(Clone)]
pub struct MinecraftInstance {
    config: Arc<Mutex<RestoreConfig>>,
    uuid: InstanceUuid,
    creation_time: i64,
    state: Arc<Mutex<State>>,
    event_broadcaster: EventBroadcaster,
    // file paths
    path_to_instance: PathBuf,
    path_to_config: PathBuf,
    path_to_properties: PathBuf,

    // directory paths
    path_to_macros: PathBuf,
    path_to_runtimes: PathBuf,

    // variables which can be changed at runtime
    auto_start: Arc<AtomicBool>,
    restart_on_crash: Arc<AtomicBool>,
    backup_period: Option<u32>,
    process: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    system: Arc<Mutex<sysinfo::System>>,
    players_manager: Arc<Mutex<PlayersManager>>,
    configurable_manifest: Arc<Mutex<ConfigurableManifest>>,
    macro_executor: MacroExecutor,
    rcon_conn: Arc<Mutex<Option<rcon::Connection<tokio::net::TcpStream>>>>,
    macro_name_to_last_run: Arc<Mutex<HashMap<String, i64>>>,
    pid_to_task_entry: Arc<Mutex<IndexMap<MacroPID, TaskEntry>>>,
}

#[tokio::test]
async fn test_setup_manifest() {
    let manifest = MinecraftInstance::setup_manifest(&FlavourKind::Fabric)
        .await
        .unwrap();
    let manifest_json_string = serde_json::to_string_pretty(&manifest).unwrap();
    println!("{manifest_json_string}");
}

impl MinecraftInstance {
    pub async fn setup_manifest(flavour: &FlavourKind) -> Result<SetupManifest, Error> {
        let versions = match flavour {
            FlavourKind::Vanilla => get_vanilla_minecraft_versions().await,
            FlavourKind::Fabric => get_fabric_minecraft_versions().await,
            FlavourKind::Paper => get_paper_minecraft_versions().await,
            FlavourKind::Spigot => todo!(),
            FlavourKind::Forge => get_forge_minecraft_versions().await,
            FlavourKind::Neoforge => get_neoforge_minecraft_versions().await,
        }
        .context("Failed to get minecraft versions")?;

        let version_setting = SettingManifest::new_value_with_type(
            "version".to_string(),
            "Version".to_string(),
            "The version of minecraft to use".to_string(),
            Some(ConfigurableValue::Enum(versions.first().unwrap().clone())),
            ConfigurableValueType::Enum { options: versions },
            None,
            false,
            true,
        );

        let port_setting = SettingManifest::new_value_with_type(
            "port".to_string(),
            "Port".to_string(),
            "The port to run the server on".to_string(),
            Some(ConfigurableValue::UnsignedInteger(25565)),
            ConfigurableValueType::UnsignedInteger {
                min: Some(0),
                max: Some(65535),
            },
            Some(ConfigurableValue::UnsignedInteger(25565)),
            false,
            true,
        );

        let min_ram_setting = SettingManifest::new_required_value(
            "min_ram".to_string(),
            "Minimum RAM".to_string(),
            "The minimum amount of RAM to allocate to the server".to_string(),
            ConfigurableValue::UnsignedInteger(1024),
            Some(ConfigurableValue::UnsignedInteger(1024)),
            false,
            true,
        );

        let max_ram_setting = SettingManifest::new_required_value(
            "max_ram".to_string(),
            "Maximum RAM".to_string(),
            "The maximum amount of RAM to allocate to the server".to_string(),
            ConfigurableValue::UnsignedInteger(2048),
            Some(ConfigurableValue::UnsignedInteger(2048)),
            false,
            true,
        );

        let command_line_args_setting = SettingManifest::new_optional_value(
            "cmd_args".to_string(),
            "Command Line Arguments".to_string(),
            "Command line arguments to pass to the server".to_string(),
            None,
            ConfigurableValueType::String { regex: None },
            None,
            false,
            true,
        );

        let mut section_1_map = IndexMap::new();

        section_1_map.insert("version".to_string(), version_setting);
        section_1_map.insert("port".to_string(), port_setting);

        let mut section_2_map = IndexMap::new();

        section_2_map.insert("min_ram".to_string(), min_ram_setting);

        section_2_map.insert("max_ram".to_string(), max_ram_setting);

        section_2_map.insert("cmd_args".to_string(), command_line_args_setting);

        let section_1 = SectionManifest::new(
            "section_1".to_string(),
            "Basic Settings".to_string(),
            "Basic settings for the server.".to_string(),
            section_1_map,
        );

        let section_2 = SectionManifest::new(
            "section_2".to_string(),
            "Advanced Settings".to_string(),
            "Advanced settings for your minecraft server.".to_string(),
            section_2_map,
        );

        let mut sections = IndexMap::new();

        sections.insert("section_1".to_string(), section_1);
        sections.insert("section_2".to_string(), section_2);

        Ok(SetupManifest {
            setting_sections: sections,
        })
    }

    pub async fn construct_setup_config(
        setup_value: SetupValue,
        flavour: FlavourKind,
    ) -> Result<SetupConfig, Error> {
        Self::setup_manifest(&flavour)
            .await?
            .validate_setup_value(&setup_value)?;

        // ALL of the following unwraps are safe because we just validated the manifest value
        let description = setup_value.description.clone();

        let name = setup_value.name.clone();

        let version = setup_value
            .get_unique_setting("version")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_enum()
            .unwrap();

        let port = setup_value
            .get_unique_setting("port")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_unsigned_integer()
            .unwrap();

        let min_ram = setup_value
            .get_unique_setting("min_ram")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_unsigned_integer()
            .unwrap();

        let max_ram = setup_value
            .get_unique_setting("max_ram")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_unsigned_integer()
            .unwrap();

        let cmd_args: Vec<String> = setup_value
            .get_unique_setting("cmd_args")
            .unwrap()
            .get_value()
            .map(|v| v.try_as_string().unwrap())
            .unwrap_or(&"".to_string())
            .split(' ')
            .map(|s| s.to_string())
            .collect();

        Ok(SetupConfig {
            name,
            description,
            version: version.clone(),
            port,
            min_ram: Some(min_ram),
            max_ram: Some(max_ram),
            cmd_args,
            flavour: flavour.into(),
            auto_start: Some(setup_value.auto_start),
            restart_on_crash: Some(setup_value.restart_on_crash),
            backup_period: None,
        })
    }

    fn init_configurable_manifest(
        restore_config: &RestoreConfig,
        java_cmd: String,
    ) -> ConfigurableManifest {
        let mut cmd_args_config_map = IndexMap::new();
        let cmd_args = CmdArgSetting::Args(restore_config.cmd_args.clone());
        cmd_args_config_map.insert(cmd_args.get_identifier().to_owned(), cmd_args.into());
        let min_ram = CmdArgSetting::MinRam(restore_config.min_ram);
        cmd_args_config_map.insert(min_ram.get_identifier().to_owned(), min_ram.into());
        let max_ram = CmdArgSetting::MaxRam(restore_config.max_ram);
        cmd_args_config_map.insert(max_ram.get_identifier().to_owned(), max_ram.into());
        let java_cmd = CmdArgSetting::JavaCmd(java_cmd);
        cmd_args_config_map.insert(java_cmd.get_identifier().to_owned(), java_cmd.into());

        let cmd_line_section_manifest = SectionManifest::new(
            CmdArgSetting::get_section_id().to_string(),
            "Command Line Settings".to_string(),
            "Settings are passed to the server and Java as command line arguments".to_string(),
            cmd_args_config_map,
        );

        let server_properties_section_manifest = SectionManifest::new(
            ServerPropertySetting::get_section_id().to_string(),
            "Server Properties Settings".to_string(),
            "All settings in the server.properties file can be configured here".to_string(),
            IndexMap::new(),
        );

        let mut setting_sections = IndexMap::new();

        setting_sections.insert(
            CmdArgSetting::get_section_id().to_string(),
            cmd_line_section_manifest,
        );

        setting_sections.insert(
            ServerPropertySetting::get_section_id().to_string(),
            server_properties_section_manifest,
        );

        ConfigurableManifest::new(false, false, setting_sections)
    }

    pub async fn new(
        config: SetupConfig,
        dot_lodestone_config: DotLodestoneConfig,
        path_to_instance: PathBuf,
        progression_event_id: &ProgressionEventID,
        event_broadcaster: EventBroadcaster,
        macro_executor: MacroExecutor,
    ) -> Result<MinecraftInstance, Error> {
        let path_to_config = path_to_instance.join(".lodestone_minecraft_config.json");
        let path_to_eula = path_to_instance.join("eula.txt");
        let path_to_macros = path_to_instance.join("macros");
        let path_to_resources = path_to_instance.join("resources");
        let path_to_properties = path_to_instance.join("server.properties");
        let path_to_runtimes = path_to_binaries().to_owned();

        // Step 1: Create Directories
        event_broadcaster.send(Event::new_progression_event_update(
            progression_event_id,
            "1/4: Creating directories",
            1.0,
        ));
        tokio::fs::create_dir_all(&path_to_instance)
            .await
            .and(tokio::fs::create_dir_all(&path_to_macros).await)
            .and(tokio::fs::create_dir_all(&path_to_resources.join("mods")).await)
            .and(tokio::fs::create_dir_all(&path_to_resources.join("worlds")).await)
            .and(tokio::fs::create_dir_all(&path_to_resources.join("defaults")).await)
            .and(tokio::fs::write(&path_to_eula, "#generated by Lodestone\neula=true").await)
            .and(
                tokio::fs::write(&path_to_properties, format!("server-port={}", config.port)).await,
            )
            .context("Could not create some files or directories for instance")
            .map_err(|e| {
                error!("{e}");
                e
            })?;

        let version_regex = Regex::new(r"^1\.\d+\.\d*0$").unwrap();

        let version = if version_regex.is_match(config.version.as_str()) {
            &config.version[..config.version.len() - 2]
        } else {
            config.version.as_str()
        };

        // Step 2: Download JRE
        let (url, jre_major_version) = get_jre_url(version)
            .await
            .context("Could not get JRE URL")?;
        if !path_to_runtimes
            .join("java")
            .join(format!("jre{}", jre_major_version))
            .exists()
        {
            let downloaded = download_file(
                &url,
                &path_to_runtimes.join("java"),
                None,
                {
                    let event_broadcaster = event_broadcaster.clone();
                    &move |dl| {
                        if let Some(total) = dl.total {
                            event_broadcaster.send(Event::new_progression_event_update(
                                progression_event_id,
                                format!(
                                    "2/4: Downloading JRE {}",
                                    format_byte_download(dl.downloaded, total)
                                ),
                                (dl.step as f64 / total as f64) * 4.0,
                            ));
                        }
                    }
                },
                true,
            )
            .await?;

            let unzipped_content = unzip_file_async(
                &downloaded,
                UnzipOption::ToDir(path_to_runtimes.join("java")),
            )
            .await?;
            if unzipped_content.len() != 1 {
                return Err(eyre!(
                    "Expected only one file in the JRE archive, got {}",
                    unzipped_content.len()
                )
                .into());
            }

            tokio::fs::remove_file(&downloaded).await.context(format!(
                "Could not remove downloaded JRE file {}",
                downloaded.display()
            ))?;

            tokio::fs::rename(
                unzipped_content.iter().last().unwrap(),
                path_to_runtimes
                    .join("java")
                    .join(format!("jre{}", jre_major_version)),
            )
            .await
            .context(format!(
                "Could not rename JRE directory {}",
                unzipped_content.iter().last().unwrap().display()
            ))?;
        } else {
            event_broadcaster.send(Event::new_progression_event_update(
                progression_event_id,
                "2/4: JRE already downloaded",
                4.0,
            ));
        }

        // Step 3: Download server.jar
        let flavour_name = config.flavour.to_string();
        let (jar_url, flavour) = get_server_jar_url(config.version.as_str(), &config.flavour)
            .await
            .ok_or_else({
                || {
                    eyre!(
                        "Could not find a {} server.jar for version {}",
                        flavour_name,
                        config.version
                    )
                }
            })?;
        let jar_name = match flavour {
            Flavour::Forge { .. } => "forge-installer.jar",
            Flavour::Neoforge { .. } => "neoforge-installer.jar",
            _ => "server.jar",
        };

        download_file(
            jar_url.as_str(),
            &path_to_instance,
            Some(jar_name),
            {
                let event_broadcaster = event_broadcaster.clone();
                &move |dl| {
                    if let Some(total) = dl.total {
                        event_broadcaster.send(Event::new_progression_event_update(
                            progression_event_id,
                            format!(
                                "3/4: Downloading {} {} {}",
                                flavour_name,
                                jar_name,
                                format_byte_download(dl.downloaded, total),
                            ),
                            (dl.step as f64 / total as f64) * 3.0,
                        ));
                    } else {
                        event_broadcaster.send(Event::new_progression_event_update(
                            progression_event_id,
                            format!(
                                "3/4: Downloading {} {} {}",
                                flavour_name,
                                jar_name,
                                format_byte(dl.downloaded),
                            ),
                            0.0,
                        ));
                    }
                }
            },
            true,
        )
        .await?;
        let jre = path_to_runtimes
            .join("java")
            .join(format!("jre{}", jre_major_version))
            .join(if std::env::consts::OS == "macos" {
                "Contents/Home/bin"
            } else {
                "bin"
            })
            .join("java");
        // Step 3 (part 2): Forge Setup
        if let Flavour::Forge { .. } = flavour.clone() {
            event_broadcaster.send(Event::new_progression_event_update(
                progression_event_id,
                "3/4: Installing Forge Server",
                1.0,
            ));

            if !dont_spawn_terminal(
                Command::new(&jre)
                    .arg("-jar")
                    .arg(&path_to_instance.join("forge-installer.jar"))
                    .arg("--installServer")
                    .arg(&path_to_instance)
                    .current_dir(&path_to_instance),
            )
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .context("Failed to start forge-installer.jar")?
            .wait()
            .await
            .context("forge-installer.jar failed")?
            .success()
            {
                return Err(eyre!("Failed to install forge server").into());
            }

            tokio::fs::write(
                &path_to_instance.join("user_jvm_args.txt"),
                "# Generated by Lodestone\n# This file is ignored by Lodestone\n# Please set arguments using Lodestone",
            )
            .await
            .context("Could not create user_jvm_args.txt")?;
        }

        if let Flavour::Neoforge { .. } = flavour.clone() {
            event_broadcaster.send(Event::new_progression_event_update(
                progression_event_id,
                "3/4: Installing Neoforge Server",
                1.0,
            ));

            if !dont_spawn_terminal(
                Command::new(&jre)
                    .arg("-jar")
                    .arg(&path_to_instance.join("neoforge-installer.jar"))
                    .arg("--installServer")
                    .arg(&path_to_instance)
                    .current_dir(&path_to_instance),
            )
                .stderr(Stdio::null())
                .stdout(Stdio::null())
                .stdin(Stdio::null())
                .spawn()
                .context("Failed to start neoforge-installer.jar")?
                .wait()
                .await
                .context("neoforge-installer.jar failed")?
                .success()
            {
                return Err(eyre!("Failed to install neoforge server").into());
            }

            tokio::fs::write(
                &path_to_instance.join("user_jvm_args.txt"),
                "# Generated by Lodestone\n# This file is ignored by Lodestone\n# Please set arguments using Lodestone",
            )
                .await
                .context("Could not create user_jvm_args.txt")?;
        }

        // Step 4: Finishing Up
        event_broadcaster.send(Event::new_progression_event_update(
            progression_event_id,
            "4/4: Finishing up",
            1.0,
        ));

        let restore_config = RestoreConfig {
            name: config.name,
            version: config.version,
            flavour,
            description: config.description.unwrap_or_default(),
            cmd_args: config.cmd_args,
            port: config.port,
            min_ram: config.min_ram.unwrap_or(2048),
            max_ram: config.max_ram.unwrap_or(4096),
            auto_start: config.auto_start.unwrap_or(false),
            restart_on_crash: config.restart_on_crash.unwrap_or(false),
            backup_period: config.backup_period,
            jre_major_version,
            has_started: false,
            java_cmd: Some(jre.to_string_lossy().to_string()),
        };
        // create config file
        tokio::fs::write(
            &path_to_config,
            to_string_pretty(&restore_config).context(
                "Failed to serialize config to string. This is a bug, please report it.",
            )?,
        )
        .await
        .context(format!(
            "Failed to write config file at {}",
            &path_to_config.display()
        ))?;
        MinecraftInstance::restore(
            path_to_instance,
            dot_lodestone_config,
            event_broadcaster,
            macro_executor,
        )
        .await
    }

    pub async fn restore(
        path_to_instance: PathBuf,
        dot_lodestone_config: DotLodestoneConfig,
        event_broadcaster: EventBroadcaster,
        macro_executor: MacroExecutor,
    ) -> Result<MinecraftInstance, Error> {
        let path_to_config = path_to_instance.join(".lodestone_minecraft_config.json");
        let restore_config: RestoreConfig =
            serde_json::from_reader(std::fs::File::open(&path_to_config).context(format!(
                "Failed to open config file at {}",
                &path_to_config.display()
            ))?)
            .context(
                "Failed to deserialize config from string. Was the config file modified manually?",
            )?;
        let path_to_macros = path_to_instance.join("macros");
        let path_to_properties = path_to_instance.join("server.properties");
        let path_to_runtimes = path_to_binaries().clone();
        // if the properties file doesn't exist, create it
        if !path_to_properties.exists() {
            tokio::fs::write(
                &path_to_properties,
                format!("server-port={}", restore_config.port),
            )
            .await
            .expect("failed to write to server.properties");
        };
        let java_path = path_to_runtimes
            .join("java")
            .join(format!("jre{}", restore_config.jre_major_version))
            .join(if std::env::consts::OS == "macos" {
                "Contents/Home/bin"
            } else {
                "bin"
            })
            .join("java");

        let configurable_manifest = Arc::new(Mutex::new(Self::init_configurable_manifest(
            &restore_config,
            java_path.to_string_lossy().to_string(),
        )));

        let instance = MinecraftInstance {
            state: Arc::new(Mutex::new(State::Stopped)),
            uuid: dot_lodestone_config.uuid().clone(),
            creation_time: dot_lodestone_config.creation_time(),
            auto_start: Arc::new(AtomicBool::new(restore_config.auto_start)),
            restart_on_crash: Arc::new(AtomicBool::new(restore_config.restart_on_crash)),
            backup_period: restore_config.backup_period,
            players_manager: Arc::new(Mutex::new(PlayersManager::new(
                event_broadcaster.clone(),
                dot_lodestone_config.uuid().clone(),
            ))),
            config: Arc::new(Mutex::new(restore_config)),
            path_to_instance,
            path_to_config,
            path_to_properties,
            path_to_macros,
            macro_executor,
            event_broadcaster,
            path_to_runtimes,
            process: Arc::new(Mutex::new(None)),
            system: Arc::new(Mutex::new(sysinfo::System::new_all())),
            stdin: Arc::new(Mutex::new(None)),
            rcon_conn: Arc::new(Mutex::new(None)),
            configurable_manifest,
            macro_name_to_last_run: Arc::new(Mutex::new(HashMap::new())),
            pid_to_task_entry: Arc::new(Mutex::new(IndexMap::new())),
        };
        instance
            .read_properties()
            .await
            .context("Failed to read properties")?;
        Ok(instance)
    }

    async fn write_config_to_file(&self) -> Result<(), Error> {
        tokio::fs::write(
            &self.path_to_config,
            to_string_pretty(&*self.config.lock().await)
                .context("Failed to serialize config to string, this is a bug, please report it")?,
        )
        .await
        .context(format!(
            "Failed to write config to file at {}",
            &self.path_to_config.display()
        ))?;
        Ok(())
    }

    async fn read_properties(&self) -> Result<(), Error> {
        let properties = read_properties_from_path(&self.path_to_properties).await?;
        let mut lock = self.configurable_manifest.lock().await;
        for (key, value) in properties.iter() {
            let _ = lock
                .set_setting(
                    ServerPropertySetting::get_section_id(),
                    match ServerPropertySetting::from_key_val(key, value) {
                        Ok(v) => v.into(),
                        Err(e) => {
                            error!(
                                "Failed to parse property {} with value {}: {}",
                                key, value, e
                            );
                            continue;
                        }
                    },
                )
                .map_err(|e| {
                    error!("Failed to set property {} to {}: {}", key, value, e);
                });
        }
        Ok(())
    }

    async fn write_properties_to_file(&self) -> Result<(), Error> {
        // open the file in write-only mode, returns `io::Result<File>`
        let mut file = tokio::fs::File::create(&self.path_to_properties)
            .await
            .context(format!(
                "Failed to open properties file at {}",
                &self.path_to_properties.display()
            ))?;
        let mut setting_str = "".to_string();
        for (key, value) in self
            .configurable_manifest
            .lock()
            .await
            .get_section(ServerPropertySetting::get_section_id())
            .unwrap()
            .all_settings()
            .iter()
        {
            // print the key and value separated by a =
            // println!("{}={}", key, value);
            setting_str.push_str(&format!(
                "{}={}\n",
                key,
                value
                    .get_value()
                    .expect("Programming error, value is not set")
                    .to_string()
            ));
        }
        file.write_all(setting_str.as_bytes())
            .await
            .context(format!(
                "Failed to write properties to file at {}",
                &self.path_to_properties.display()
            ))?;
        Ok(())
    }

    async fn sync_configurable_to_restore_config(&self) {
        let mut config_lock = self.config.lock().await;

        let configurable_map_lock = self.configurable_manifest.lock().await;
        config_lock.port = configurable_map_lock
            .get_unique_setting_key("server-port")
            .expect("Programming error, value is not set")
            .get_value()
            .expect("Programming error, value is not set")
            .try_as_unsigned_integer()
            .expect("Programming error, value is not a unsigned integer");
        let configurable_map = configurable_map_lock
            .get_section(CmdArgSetting::get_section_id())
            .unwrap()
            .all_settings();
        config_lock.cmd_args = configurable_map
            .get(CmdArgSetting::Args(Default::default()).get_identifier())
            .expect("Programming error, value is not set")
            .get_value()
            .expect("Programming error, value is not set")
            .clone()
            .try_as_string()
            .expect("Programming error, value is not a string")
            .split(' ')
            .map(|s| s.to_string())
            .collect();

        config_lock.max_ram = configurable_map
            .get(CmdArgSetting::MaxRam(Default::default()).get_identifier())
            .expect("Programming error, value is not set")
            .get_value()
            .expect("Programming error, value is not set")
            .clone()
            .try_as_unsigned_integer()
            .expect("Programming error, value is not an unsigned integer");

        config_lock.min_ram = configurable_map
            .get(CmdArgSetting::MinRam(Default::default()).get_identifier())
            .expect("Programming error, value is not set")
            .get_value()
            .expect("Programming error, value is not set")
            .clone()
            .try_as_unsigned_integer()
            .expect("Programming error, value is not an unsigned integer");

        config_lock.java_cmd = Some(
            configurable_map
                .get(CmdArgSetting::JavaCmd(Default::default()).get_identifier())
                .expect("Programming error, value is not set")
                .get_value()
                .expect("Programming error, value is not set")
                .clone()
                .try_as_string()
                .expect("Programming error, value is not a string")
                .to_owned(),
        );
    }

    pub fn get_rcon(&self) -> Arc<Mutex<Option<rcon::Connection<tokio::net::TcpStream>>>> {
        self.rcon_conn.clone()
    }

    pub async fn send_rcon(&self, cmd: &str) -> Result<String, Error> {
        let a = self
            .rcon_conn
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| {
                eyre!("Failed to send rcon command, rcon connection is not initialized")
            })?
            .cmd(cmd)
            .await
            .context("Failed to send rcon command")?;
        Ok(a)
    }
}

impl TInstance for MinecraftInstance {}
