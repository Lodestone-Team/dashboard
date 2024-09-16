use std::str::FromStr;
use std::sync::atomic;

use async_trait::async_trait;
use color_eyre::eyre::{eyre, Context, ContextCompat};

use crate::error::{Error, ErrorKind};
use crate::prelude::path_to_tmp;
use crate::traits::t_configurable::manifest::{
    ConfigurableManifest, ConfigurableValue, ConfigurableValueType, SettingManifest,
};
use crate::traits::t_configurable::{Game, TConfigurable};
use crate::traits::t_server::State;

use crate::types::InstanceUuid;
use crate::util::download_file;

use super::util::{get_fabric_jar_url, get_paper_jar_url, get_vanilla_jar_url};
use super::MinecraftInstance;

#[async_trait]
impl TConfigurable for MinecraftInstance {
    async fn uuid(&self) -> InstanceUuid {
        self.uuid.clone()
    }

    async fn name(&self) -> String {
        self.config.lock().await.name.clone()
    }

    async fn game_type(&self) -> Game {
        self.config.lock().await.flavour.clone().into()
    }

    async fn version(&self) -> String {
        self.config.lock().await.version.clone()
    }

    async fn description(&self) -> String {
        self.config.lock().await.description.clone()
    }

    async fn port(&self) -> u32 {
        self.config.lock().await.port
    }

    async fn creation_time(&self) -> i64 {
        self.creation_time
    }

    async fn path(&self) -> std::path::PathBuf {
        self.path_to_instance.clone()
    }

    async fn auto_start(&self) -> bool {
        self.config.lock().await.auto_start
    }

    async fn restart_on_crash(&self) -> bool {
        self.config.lock().await.restart_on_crash
    }

    async fn set_name(&self, name: String) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Name cannot be empty"),
            });
        }
        if name.len() > 100 {
            return Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Name cannot be longer than 100 characters"),
            });
        }
        self.config.lock().await.name = name;
        self.write_config_to_file().await?;
        Ok(())
    }

    async fn set_description(&self, description: String) -> Result<(), Error> {
        self.config.lock().await.description = description;
        self.write_config_to_file().await?;
        Ok(())
    }

    async fn set_port(&self, port: u32) -> Result<(), Error> {
        self.configurable_manifest.lock().await.set_setting(
            ServerPropertySetting::get_section_id(),
            ServerPropertySetting::ServerPort(port as u16).into(),
        )?;
        self.config.lock().await.port = port;

        self.write_config_to_file()
            .await
            .and(self.write_properties_to_file().await)
    }

    async fn set_auto_start(&self, auto_start: bool) -> Result<(), Error> {
        self.config.lock().await.auto_start = auto_start;
        self.auto_start.store(auto_start, atomic::Ordering::Relaxed);
        self.write_config_to_file().await
    }

    async fn set_restart_on_crash(&self, restart_on_crash: bool) -> Result<(), Error> {
        self.config.lock().await.restart_on_crash = restart_on_crash;
        self.auto_start
            .store(restart_on_crash, atomic::Ordering::Relaxed);
        self.write_config_to_file().await
    }

    async fn change_version(&self, version: String) -> Result<(), Error> {
        if *self.state.lock().await != State::Stopped {
            return Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Cannot change version while server is running"),
            });
        }
        if version == self.config.lock().await.version {
            return Ok(());
        }
        let (url, _) = match self.config.lock().await.flavour {
            super::Flavour::Vanilla => get_vanilla_jar_url(&version).await.ok_or_else(|| {
                let error_msg =
                    format!("Cannot get the vanilla jar version for version {}", version);
                Error {
                    kind: ErrorKind::BadRequest,
                    source: eyre!(error_msg),
                }
            })?,
            super::Flavour::Fabric { .. } => get_fabric_jar_url(&version, &None, &None)
                .await
                .ok_or_else(|| {
                    let error_msg =
                        format!("Cannot get the fabric jar version for version {}", version);
                    Error {
                        kind: ErrorKind::BadRequest,
                        source: eyre!(error_msg),
                    }
                })?,
            super::Flavour::Paper { .. } => {
                get_paper_jar_url(&version, &None).await.ok_or_else(|| {
                    let error_msg =
                        format!("Cannot get the paper jar version for version {}", version);
                    Error {
                        kind: ErrorKind::BadRequest,
                        source: eyre!(error_msg),
                    }
                })?
            }
            super::Flavour::Spigot => todo!(),
            super::Flavour::Forge { .. } => {
                return Err(Error {
                    kind: ErrorKind::UnsupportedOperation,
                    source: eyre!("Changing versions is unsupported for forge servers"),
                })
            }
            super::Flavour::Neoforge { .. } => {
                return Err(Error {
                    kind: ErrorKind::UnsupportedOperation,
                    source: eyre!("Changing versions is unsupported for forge servers"), //TODO
                });
            }
        };
        let lodestone_tmp = path_to_tmp().clone();
        let temp_dir = tempfile::tempdir_in(lodestone_tmp).context("Failed to create temp dir")?;
        download_file(
            &url,
            temp_dir.path(),
            Some("server.jar"),
            &Box::new(|_| {}),
            true,
        )
        .await?;
        let jar_path = temp_dir.path().join("server.jar");
        crate::util::fs::rename(jar_path, self.path().await.join("server.jar")).await?;
        self.config.lock().await.version = version;
        self.write_config_to_file().await
    }

    async fn configurable_manifest(&self) -> ConfigurableManifest {
        self.configurable_manifest
            .lock()
            .await
            .clear_section(ServerPropertySetting::get_section_id());
        let _ = self.read_properties().await;
        self.configurable_manifest.lock().await.clone()
    }

    async fn update_configurable(
        &self,
        section_id: &str,
        setting_id: &str,
        value: ConfigurableValue,
    ) -> Result<(), Error> {
        let _ = self.read_properties().await;
        self.configurable_manifest
            .lock()
            .await
            .update_setting_value(section_id, setting_id, value.clone())?;
        self.sync_configurable_to_restore_config().await;
        self.write_config_to_file().await?;
        self.write_properties_to_file().await
    }
}

pub(super) enum InstanceSetting {
    CmdArg(CmdArgSetting),
    ServerProperty(ServerPropertySetting),
}

#[allow(dead_code)]
impl InstanceSetting {
    pub fn get_identifier(&self) -> String {
        match self {
            InstanceSetting::CmdArg(setting) => setting.get_identifier().to_owned(),
            InstanceSetting::ServerProperty(setting) => setting.get_identifier(),
        }
    }
    pub fn get_name(&self) -> String {
        match self {
            InstanceSetting::CmdArg(setting) => setting.get_name().to_owned(),
            InstanceSetting::ServerProperty(setting) => setting.get_name(),
        }
    }
    pub fn get_description(&self) -> String {
        match self {
            InstanceSetting::CmdArg(setting) => setting.get_description().to_owned(),
            InstanceSetting::ServerProperty(setting) => setting.get_description(),
        }
    }
    pub fn from_key_val(key: &str, value: &str) -> Result<Self, Error> {
        if CmdArgSetting::is_key_valid(key) {
            Ok(InstanceSetting::CmdArg(CmdArgSetting::from_key_val(
                key, value,
            )?))
        } else {
            Ok(InstanceSetting::ServerProperty(
                ServerPropertySetting::from_key_val(key, value)?,
            ))
        }
    }
}

impl From<CmdArgSetting> for InstanceSetting {
    fn from(setting: CmdArgSetting) -> Self {
        InstanceSetting::CmdArg(setting)
    }
}

impl From<ServerPropertySetting> for InstanceSetting {
    fn from(setting: ServerPropertySetting) -> Self {
        InstanceSetting::ServerProperty(setting)
    }
}

impl From<InstanceSetting> for SettingManifest {
    fn from(setting: InstanceSetting) -> Self {
        match setting {
            InstanceSetting::CmdArg(setting) => setting.into(),
            InstanceSetting::ServerProperty(setting) => setting.into(),
        }
    }
}

impl TryFrom<SettingManifest> for InstanceSetting {
    type Error = Error;

    fn try_from(setting: SettingManifest) -> Result<Self, Self::Error> {
        if CmdArgSetting::is_key_valid(setting.get_identifier()) {
            Ok(InstanceSetting::CmdArg(CmdArgSetting::try_from(setting)?))
        } else {
            Ok(InstanceSetting::ServerProperty(
                ServerPropertySetting::try_from(setting)?,
            ))
        }
    }
}

#[derive(Debug)]
pub(super) enum CmdArgSetting {
    MinRam(u32),
    MaxRam(u32),
    JavaCmd(String),
    Args(Vec<String>),
}

impl CmdArgSetting {
    pub fn get_section_id() -> &'static str {
        "cmd_args_section"
    }
    pub fn get_identifier(&self) -> &'static str {
        match self {
            CmdArgSetting::MinRam(_) => "min_ram",
            CmdArgSetting::MaxRam(_) => "max_ram",
            CmdArgSetting::JavaCmd(_) => "java_cmd",
            CmdArgSetting::Args(_) => "cmd_args",
        }
    }
    pub fn get_name(&self) -> &'static str {
        match self {
            CmdArgSetting::MinRam(_) => "Minimum RAM",
            CmdArgSetting::MaxRam(_) => "Maximum RAM",
            CmdArgSetting::JavaCmd(_) => "Java command",
            CmdArgSetting::Args(_) => "Command line arguments",
        }
    }
    pub fn get_description(&self) -> &'static str {
        match self {
            CmdArgSetting::MinRam(_) => {
                "The minimum amount of RAM to allocate to the server instance"
            }
            CmdArgSetting::MaxRam(_) => {
                "The maximum amount of RAM to allocate to the server instance"
            }
            CmdArgSetting::JavaCmd(_) => "The command to use to run the java executable",
            CmdArgSetting::Args(_) => "The command line arguments to pass to the server",
        }
    }
    pub fn from_key_val(key: &str, val: &str) -> Result<Self, Error> {
        match key {
            "min_ram" => Ok(CmdArgSetting::MinRam(
                val.parse().context("Invalid value. Expected a u32")?,
            )),
            "max_ram" => Ok(CmdArgSetting::MaxRam(
                val.parse().context("Invalid value. Expected a u32")?,
            )),
            "java_cmd" => Ok(CmdArgSetting::JavaCmd(val.to_string())),
            "cmd_args" => Ok(CmdArgSetting::Args(
                val.split(' ').map(|s| s.to_string()).collect(),
            )),
            _ => Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Invalid key"),
            }),
        }
    }
    pub fn is_key_valid(key: &str) -> bool {
        matches!(key, "min_ram" | "max_ram" | "java_cmd" | "cmd_args")
    }
}

impl From<CmdArgSetting> for SettingManifest {
    fn from(value: CmdArgSetting) -> Self {
        match value {
            CmdArgSetting::MinRam(min_ram) => SettingManifest::new_optional_value(
                value.get_identifier().to_owned(),
                value.get_name().to_owned(),
                value.get_description().to_owned(),
                Some(ConfigurableValue::UnsignedInteger(min_ram)),
                ConfigurableValueType::UnsignedInteger {
                    min: Some(0),
                    max: None,
                },
                None,
                false,
                true,
            ),
            CmdArgSetting::MaxRam(max_ram) => SettingManifest::new_optional_value(
                value.get_identifier().to_owned(),
                value.get_name().to_owned(),
                value.get_description().to_owned(),
                Some(ConfigurableValue::UnsignedInteger(max_ram)),
                ConfigurableValueType::UnsignedInteger {
                    min: Some(0),
                    max: None,
                },
                None,
                false,
                true,
            ),
            CmdArgSetting::JavaCmd(ref java_cmd) => SettingManifest::new_optional_value(
                value.get_identifier().to_owned(),
                value.get_name().to_owned(),
                value.get_description().to_owned(),
                Some(ConfigurableValue::String(java_cmd.to_owned())),
                ConfigurableValueType::String { regex: None },
                None,
                false,
                true,
            ),
            CmdArgSetting::Args(ref args) => SettingManifest::new_optional_value(
                value.get_identifier().to_owned(),
                value.get_name().to_owned(),
                value.get_description().to_owned(),
                Some(ConfigurableValue::String(args.join(" "))),
                ConfigurableValueType::String { regex: None },
                None,
                false,
                true,
            ),
        }
    }
}

impl TryFrom<SettingManifest> for CmdArgSetting {
    type Error = Error;

    fn try_from(value: SettingManifest) -> Result<Self, Self::Error> {
        match value.get_identifier().as_str() {
            "min_ram" => Ok(CmdArgSetting::MinRam(
                value
                    .get_value()
                    .context("Expected a value")?
                    .try_as_integer()? as u32,
            )),
            "max_ram" => Ok(CmdArgSetting::MaxRam(
                value
                    .get_value()
                    .context("Expected a value")?
                    .try_as_integer()? as u32,
            )),
            "java_cmd" => Ok(CmdArgSetting::JavaCmd(
                value
                    .get_value()
                    .context("Expected a value")?
                    .try_as_string()?
                    .to_owned(),
            )),
            "cmd_args" => Ok(CmdArgSetting::Args(
                value
                    .get_value()
                    .context("Expected a value")?
                    .try_as_string()?
                    .split(' ')
                    .map(|s| s.to_string())
                    .collect(),
            )),
            _ => Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Invalid key"),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum Gamemode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl ToString for Gamemode {
    fn to_string(&self) -> String {
        match self {
            Gamemode::Survival => "survival",
            Gamemode::Creative => "creative",
            Gamemode::Adventure => "adventure",
            Gamemode::Spectator => "spectator",
        }
        .to_string()
    }
}

impl FromStr for Gamemode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "survival" => Ok(Gamemode::Survival),
            "creative" => Ok(Gamemode::Creative),
            "adventure" => Ok(Gamemode::Adventure),
            "spectator" => Ok(Gamemode::Spectator),
            _ => Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Invalid gamemode. The only valid gamemodes are: survival, creative, adventure, spectator"),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum Difficulty {
    #[default]
    Peaceful,
    Easy,
    Normal,
    Hard,
}

impl ToString for Difficulty {
    fn to_string(&self) -> String {
        match self {
            Difficulty::Peaceful => "peaceful".to_string(),
            Difficulty::Easy => "easy".to_string(),
            Difficulty::Normal => "normal".to_string(),
            Difficulty::Hard => "hard".to_string(),
        }
    }
}

impl FromStr for Difficulty {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "peaceful" => Ok(Difficulty::Peaceful),
            "easy" => Ok(Difficulty::Easy),
            "normal" => Ok(Difficulty::Normal),
            "hard" => Ok(Difficulty::Hard),
            _ => Err(Error {
                kind: ErrorKind::BadRequest,
                source: eyre!("Invalid difficulty. The only valid difficulties are: peaceful, easy, normal, hard"),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ServerPropertySetting {
    EnableJmxMonitoring(bool),
    RconPort(u16),
    LevelSeed(String),
    Gamemode(Gamemode),
    EnableCommandBlock(bool),
    EnableQuery(bool),
    GeneratorSettings(String),
    EnforceSecureProfile(bool),
    LevelName(String),
    Motd(String),
    QueryPort(u16),
    Pvp(bool),
    GenerateStructures(bool),
    MaxChainedNeighborUpdates(u32),
    Difficulty(Difficulty),
    NetworkCompressionThreshold(u32),
    RequireResourcePack(bool),
    MaxTickTime(u32),
    MaxPlayers(u32),
    UseNativeTransport(bool),
    OnlineMode(bool),
    EnableStatus(bool),
    AllowFlight(bool),
    InitialDisabledPacks(String),
    BroadcastRconToOps(bool),
    ViewDistance(u32),
    ResourcePackPrompt(String),
    ServerIp(String),
    AllowNether(bool),
    ServerPort(u16),
    EnableRcon(bool),
    SyncChunkWrites(bool),
    OpPermissionLevel(u32),
    PreventProxyConnections(bool),
    HideOnlinePlayers(bool),
    ResourcePack(String),
    EntityBroadcastRangePercentage(u32),
    SimulationDistance(u32),
    RconPassword(String),
    PlayerIdleTimeout(u32),
    ForceGamemode(bool),
    RateLimit(u32),
    Hardcore(bool),
    WhiteList(bool),
    BroadcastConsoleToOps(bool),
    PreviewsChat(bool),
    SpawnNpcs(bool),
    SpawnAnimals(bool),
    FunctionPermissionLevel(u32),
    InitialEnabledPacks(String),
    LevelType(String),
    TextFilteringConfig(String),
    SpawnMonsters(bool),
    EnforceWhitelist(bool),
    SpawnProtection(u32),
    ResourcePackSha1(String),
    MaxWorldSize(u32),
    MaxBuildHeight(u32),
    Unknown(String, String),
}

impl From<ServerPropertySetting> for SettingManifest {
    fn from(value: ServerPropertySetting) -> Self {
        match value {
            ServerPropertySetting::EnableJmxMonitoring(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::RconPort(inner_val) => Self::new_value_with_type(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                Some(ConfigurableValue::UnsignedInteger(inner_val as u32)),
                ConfigurableValueType::UnsignedInteger {
                    min: Some(0),
                    max: Some(65535),
                },
                None,
                false,
                true,
            ),
            ServerPropertySetting::LevelSeed(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::Gamemode(ref inner_val) => Self::new_value_with_type(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                Some(ConfigurableValue::Enum(inner_val.to_string())),
                ConfigurableValueType::Enum {
                    options: vec![
                        "survival".to_string(),
                        "creative".to_string(),
                        "adventure".to_string(),
                        "spectator".to_string(),
                    ],
                },
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnableCommandBlock(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnableQuery(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::GeneratorSettings(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnforceSecureProfile(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::LevelName(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::Motd(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::QueryPort(inner_val) => Self::new_value_with_type(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                Some(ConfigurableValue::UnsignedInteger(inner_val as u32)),
                ConfigurableValueType::UnsignedInteger {
                    min: Some(0),
                    max: Some(65535),
                },
                None,
                false,
                true,
            ),
            ServerPropertySetting::Pvp(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::GenerateStructures(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::MaxChainedNeighborUpdates(inner_val) => {
                Self::new_required_value(
                    value.get_identifier(),
                    value.get_name(),
                    value.get_description(),
                    ConfigurableValue::UnsignedInteger(inner_val),
                    None,
                    false,
                    true,
                )
            }
            ServerPropertySetting::MaxTickTime(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::UseNativeTransport(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::MaxWorldSize(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ServerIp(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::MaxPlayers(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::NetworkCompressionThreshold(inner_val) => {
                Self::new_required_value(
                    value.get_identifier(),
                    value.get_name(),
                    value.get_description(),
                    ConfigurableValue::UnsignedInteger(inner_val),
                    None,
                    false,
                    true,
                )
            }
            ServerPropertySetting::ResourcePackSha1(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::SpawnAnimals(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::SpawnNpcs(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::AllowFlight(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::LevelType(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ViewDistance(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ResourcePack(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::SpawnMonsters(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::OnlineMode(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::AllowNether(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::Difficulty(ref inner_val) => Self::new_value_with_type(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                Some(ConfigurableValue::Enum(inner_val.to_string())),
                ConfigurableValueType::Enum {
                    options: vec![
                        "peaceful".to_string(),
                        "easy".to_string(),
                        "normal".to_string(),
                        "hard".to_string(),
                    ],
                },
                None,
                false,
                true,
            ),
            ServerPropertySetting::RequireResourcePack(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnableStatus(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::InitialDisabledPacks(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::BroadcastRconToOps(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ResourcePackPrompt(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ServerPort(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val as u32),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnableRcon(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::SyncChunkWrites(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::OpPermissionLevel(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::PreventProxyConnections(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::HideOnlinePlayers(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EntityBroadcastRangePercentage(inner_val) => {
                Self::new_required_value(
                    value.get_identifier(),
                    value.get_name(),
                    value.get_description(),
                    ConfigurableValue::UnsignedInteger(inner_val),
                    None,
                    false,
                    true,
                )
            }
            ServerPropertySetting::SimulationDistance(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::RconPassword(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                true,
                true,
            ),
            ServerPropertySetting::PlayerIdleTimeout(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::ForceGamemode(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::RateLimit(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::Hardcore(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::WhiteList(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::BroadcastConsoleToOps(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::PreviewsChat(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::FunctionPermissionLevel(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::InitialEnabledPacks(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::TextFilteringConfig(ref inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(inner_val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::EnforceWhitelist(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::Boolean(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::SpawnProtection(inner_val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(inner_val),
                None,
                false,
                true,
            ),
            ServerPropertySetting::Unknown(_, ref val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::String(val.clone()),
                None,
                false,
                true,
            ),
            ServerPropertySetting::MaxBuildHeight(val) => Self::new_required_value(
                value.get_identifier(),
                value.get_name(),
                value.get_description(),
                ConfigurableValue::UnsignedInteger(val),
                None,
                false,
                true,
            ),
        }
    }
}

impl TryFrom<SettingManifest> for ServerPropertySetting {
    type Error = Error;

    fn try_from(value: SettingManifest) -> Result<Self, Self::Error> {
        let err_msg = "Internal error: value is not set";
        match value.get_identifier().as_str() {
            "enable-jmx-monitoring" => Ok(ServerPropertySetting::EnableJmxMonitoring(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "rcon.port" => Ok(ServerPropertySetting::RconPort(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?
                    .try_into()
                    .context("Invalid value")?,
            )),
            "level-seed" => Ok(ServerPropertySetting::LevelSeed(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "gamemode" => Ok(ServerPropertySetting::Gamemode(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string()
                    .parse()
                    .context("Invalid value")?,
            )),
            "enable-command-block" => Ok(ServerPropertySetting::EnableCommandBlock(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "enable-query" => Ok(ServerPropertySetting::EnableQuery(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "generator-settings" => Ok(ServerPropertySetting::GeneratorSettings(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "enforce-secure-profile" => Ok(ServerPropertySetting::EnforceSecureProfile(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "level-name" => Ok(ServerPropertySetting::LevelName(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "motd" => Ok(ServerPropertySetting::Motd(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "query.port" => Ok(ServerPropertySetting::QueryPort(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?
                    .try_into()
                    .context("Invalid value")?,
            )),
            "pvp" => Ok(ServerPropertySetting::Pvp(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "generate-structures" => Ok(ServerPropertySetting::GenerateStructures(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "max-chained-neighbor-updates" => Ok(ServerPropertySetting::MaxChainedNeighborUpdates(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_integer()?
                    .try_into()
                    .context("Invalid value")?,
            )),
            "max-tick-time" => Ok(ServerPropertySetting::MaxTickTime(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "max-players" => Ok(ServerPropertySetting::MaxPlayers(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "use-native-transport" => Ok(ServerPropertySetting::UseNativeTransport(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "online-mode" => Ok(ServerPropertySetting::OnlineMode(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "enable-status" => Ok(ServerPropertySetting::EnableStatus(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "allow-flight" => Ok(ServerPropertySetting::AllowFlight(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "initial-disabled-packs" => Ok(ServerPropertySetting::InitialDisabledPacks(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "broadcast-rcon-to-ops" => Ok(ServerPropertySetting::BroadcastRconToOps(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "view-distance" => Ok(ServerPropertySetting::ViewDistance(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "resource-pack-prompt" => Ok(ServerPropertySetting::ResourcePackPrompt(
                value.get_value().context(err_msg)?.try_as_string()?.clone(),
            )),
            "server-ip" => Ok(ServerPropertySetting::ServerIp(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "allow-nether" => Ok(ServerPropertySetting::AllowNether(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "server-port" => Ok(ServerPropertySetting::ServerPort(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?
                    .try_into()
                    .context("Invalid value")?,
            )),
            "enable-rcon" => Ok(ServerPropertySetting::EnableRcon(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "sync-chunk-writes" => Ok(ServerPropertySetting::SyncChunkWrites(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "op-permission-level" => Ok(ServerPropertySetting::OpPermissionLevel(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "prevent-proxy-connections" => Ok(ServerPropertySetting::PreventProxyConnections(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "hide-online-players" => Ok(ServerPropertySetting::HideOnlinePlayers(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "resource-pack" => Ok(ServerPropertySetting::ResourcePack(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "entity-broadcast-range-percentage" => {
                Ok(ServerPropertySetting::EntityBroadcastRangePercentage(
                    value
                        .get_value()
                        .context(err_msg)?
                        .try_as_unsigned_integer()?,
                ))
            }
            "simulation-distance" => Ok(ServerPropertySetting::SimulationDistance(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "rcon.password" => Ok(ServerPropertySetting::RconPassword(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "rate-limit" => Ok(ServerPropertySetting::RateLimit(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "hardcore" => Ok(ServerPropertySetting::Hardcore(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "white-list" => Ok(ServerPropertySetting::WhiteList(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "broadcast-console-to-ops" => Ok(ServerPropertySetting::BroadcastConsoleToOps(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "previews-chat" => Ok(ServerPropertySetting::PreviewsChat(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "spawn-npcs" => Ok(ServerPropertySetting::SpawnNpcs(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "spawn-animals" => Ok(ServerPropertySetting::SpawnAnimals(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),
            "function-permission-level" => Ok(ServerPropertySetting::FunctionPermissionLevel(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "initial-enabled-packs" => Ok(ServerPropertySetting::InitialEnabledPacks(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "level-type" => Ok(ServerPropertySetting::LevelType(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "text-filtering-config" => Ok(ServerPropertySetting::TextFilteringConfig(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "spawn-monsters" => Ok(ServerPropertySetting::SpawnMonsters(
                value.get_value().context(err_msg)?.try_as_boolean()?,
            )),

            "spawn-protection" => Ok(ServerPropertySetting::SpawnProtection(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "resource-pack-sha1" => Ok(ServerPropertySetting::ResourcePackSha1(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_string()?
                    .to_string(),
            )),
            "max-world-size" => Ok(ServerPropertySetting::MaxWorldSize(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            "max-build-height" => Ok(ServerPropertySetting::MaxBuildHeight(
                value
                    .get_value()
                    .context(err_msg)?
                    .try_as_unsigned_integer()?,
            )),
            _ => Ok(ServerPropertySetting::Unknown(
                value.get_identifier().to_string(),
                value.get_value().context(err_msg)?.to_string(),
            )),
        }
    }
}

impl ServerPropertySetting {
    pub fn get_section_id() -> &'static str {
        "server_properties_section"
    }

    pub fn get_identifier(&self) -> String {
        match self {
            Self::EnableJmxMonitoring(_) => "enable-jmx-monitoring",
            Self::RconPort(_) => "rcon.port",
            Self::LevelSeed(_) => "level-seed",
            Self::Gamemode(_) => "gamemode",
            Self::EnableCommandBlock(_) => "enable-command-block",
            Self::EnableQuery(_) => "enable-query",
            Self::GeneratorSettings(_) => "generator-settings",
            Self::EnforceSecureProfile(_) => "enforce-secure-profile",
            Self::LevelName(_) => "level-name",
            Self::Motd(_) => "motd",
            Self::QueryPort(_) => "query.port",
            Self::Pvp(_) => "pvp",
            Self::GenerateStructures(_) => "generate-structures",
            Self::MaxChainedNeighborUpdates(_) => "max-chained-neighbor-updates",
            Self::Difficulty(_) => "difficulty",
            Self::NetworkCompressionThreshold(_) => "network-compression-threshold",
            Self::RequireResourcePack(_) => "require-resource-pack",
            Self::MaxTickTime(_) => "max-tick-time",
            Self::MaxPlayers(_) => "max-players",
            Self::UseNativeTransport(_) => "use-native-transport",
            Self::OnlineMode(_) => "online-mode",
            Self::EnableStatus(_) => "enable-status",
            Self::AllowFlight(_) => "allow-flight",
            Self::InitialDisabledPacks(_) => "initial-disabled-packs",
            Self::BroadcastRconToOps(_) => "broadcast-rcon-to-ops",
            Self::ViewDistance(_) => "view-distance",
            Self::ResourcePackPrompt(_) => "resource-pack-prompt",
            Self::ServerIp(_) => "server-ip",
            Self::AllowNether(_) => "allow-nether",
            Self::ServerPort(_) => "server-port",
            Self::EnableRcon(_) => "enable-rcon",
            Self::SyncChunkWrites(_) => "sync-chunk-writes",
            Self::OpPermissionLevel(_) => "op-permission-level",
            Self::PreventProxyConnections(_) => "prevent-proxy-connections",
            Self::HideOnlinePlayers(_) => "hide-online-players",
            Self::ResourcePack(_) => "resource-pack",
            Self::EntityBroadcastRangePercentage(_) => "entity-broadcast-range-percentage",
            Self::SimulationDistance(_) => "simulation-distance",
            Self::RconPassword(_) => "rcon.password",
            Self::PlayerIdleTimeout(_) => "player-idle-timeout",
            Self::ForceGamemode(_) => "force-gamemode",
            Self::RateLimit(_) => "rate-limit",
            Self::Hardcore(_) => "hardcore",
            Self::WhiteList(_) => "white-list",
            Self::BroadcastConsoleToOps(_) => "broadcast-console-to-ops",
            Self::PreviewsChat(_) => "previews-chat",
            Self::SpawnNpcs(_) => "spawn-npcs",
            Self::SpawnAnimals(_) => "spawn-animals",
            Self::FunctionPermissionLevel(_) => "function-permission-level",
            Self::InitialEnabledPacks(_) => "initial-enabled-packs",
            Self::LevelType(_) => "level-type",
            Self::TextFilteringConfig(_) => "text-filtering-config",
            Self::SpawnMonsters(_) => "spawn-monsters",
            Self::EnforceWhitelist(_) => "enforce-whitelist",
            Self::SpawnProtection(_) => "spawn-protection",
            Self::ResourcePackSha1(_) => "resource-pack-sha1",
            Self::MaxWorldSize(_) => "max-world-size",
            Self::MaxBuildHeight(_) => "max-build-height",
            Self::Unknown(key, _) => key,
        }
        .to_string()
    }

    // name to be displayed in the UI
    fn get_name(&self) -> String {
        if let Self::Unknown(key, _) = self {
            // capitalize the first letter of the key
            let mut chars = key.chars();
            let first = chars.next().unwrap().to_uppercase();
            let rest = chars.as_str();
            return format!("{}{}", first, rest);
        };
        match self {
            Self::EnableJmxMonitoring(_) => "Enable JMX Monitoring",
            Self::RconPort(_) => "Rcon Port",
            Self::LevelSeed(_) => "Level Seed",
            Self::Gamemode(_) => "Gamemode",
            Self::EnableCommandBlock(_) => "Enable Command Block",
            Self::EnableQuery(_) => "Enable Query",
            Self::GeneratorSettings(_) => "Generator Settings",
            Self::EnforceSecureProfile(_) => "Enforce Secure Profile",
            Self::LevelName(_) => "Level Name",
            Self::Motd(_) => "Motd",
            Self::QueryPort(_) => "Query Port",
            Self::Pvp(_) => "Pvp",
            Self::GenerateStructures(_) => "Generate Structures",
            Self::MaxChainedNeighborUpdates(_) => "Max Chained Neighbor Updates",
            Self::Difficulty(_) => "Difficulty",
            Self::NetworkCompressionThreshold(_) => "Network Compression Threshold",
            Self::RequireResourcePack(_) => "Require Resource Pack",
            Self::MaxTickTime(_) => "Max Tick Time",
            Self::MaxPlayers(_) => "Max Players",
            Self::UseNativeTransport(_) => "Use Native Transport",
            Self::OnlineMode(_) => "Online Mode",
            Self::EnableStatus(_) => "Enable Status",
            Self::AllowFlight(_) => "Allow Flight",
            Self::InitialDisabledPacks(_) => "Initial Disabled Packs",
            Self::BroadcastRconToOps(_) => "Broadcast Rcon To Ops",
            Self::ViewDistance(_) => "View Distance",
            Self::ResourcePackPrompt(_) => "Resource Pack Prompt",
            Self::ServerIp(_) => "Server Ip",
            Self::AllowNether(_) => "Allow Nether",
            Self::ServerPort(_) => "Server Port",
            Self::EnableRcon(_) => "Enable Rcon",
            Self::SyncChunkWrites(_) => "Sync Chunk Writes",
            Self::OpPermissionLevel(_) => "Op Permission Level",
            Self::PreventProxyConnections(_) => "Prevent Proxy Connections",
            Self::HideOnlinePlayers(_) => "Hide Online Players",
            Self::ResourcePack(_) => "Resource Pack",
            Self::EntityBroadcastRangePercentage(_) => "Entity Broadcast Range Percentage",
            Self::SimulationDistance(_) => "Simulation Distance",
            Self::RconPassword(_) => "Rcon Password",
            Self::PlayerIdleTimeout(_) => "Player Idle Timeout",
            Self::ForceGamemode(_) => "Force Gamemode",
            Self::RateLimit(_) => "Rate Limit",
            Self::Hardcore(_) => "Hardcore",
            Self::WhiteList(_) => "White List",
            Self::BroadcastConsoleToOps(_) => "Broadcast Console To Ops",
            Self::PreviewsChat(_) => "Previews Chat",
            Self::SpawnNpcs(_) => "Spawn Npcs",
            Self::SpawnAnimals(_) => "Spawn Animals",
            Self::FunctionPermissionLevel(_) => "Function Permission Level",
            Self::InitialEnabledPacks(_) => "Initial Enabled Packs",
            Self::LevelType(_) => "Level Type",
            Self::TextFilteringConfig(_) => "Text Filtering Config",
            Self::SpawnMonsters(_) => "Spawn Monsters",
            Self::EnforceWhitelist(_) => "Enforce Whitelist",
            Self::SpawnProtection(_) => "Spawn Protection",
            Self::ResourcePackSha1(_) => "Resource Pack Sha1",
            Self::MaxWorldSize(_) => "Max World Size",
            Self::MaxBuildHeight(_) => "Max Build Height",
            Self::Unknown(_, _) => unreachable!("Handled above"),
        }
        .to_string()
    }

    // a short description of the property
    fn get_description(&self) -> String {
        if let Self::Unknown(key, val) = self {
            return format!(
                "Unknown property: {key} = {val}. Please report this to the developers."
            );
        };
        match self {
            Self::EnableJmxMonitoring(_) => "Expose JMX metrics for monitoring.",
            Self::RconPort(_) => "Port for RCON connections.",
            Self::LevelSeed(_) => "Seed for the level. If left blank, a random seed will be used.",
            Self::Gamemode(_) => "Gamemode for the server. Applied when the player joined the server for the first time. Changing this value will not change the gamemode of existing players.",
            Self::EnableCommandBlock(_) => "Enable command blocks.",
            Self::EnableQuery(_) => "Enables GameSpy4 protocol server listener. Used to get information about server.",
            Self::GeneratorSettings(_) => "A JSON string that will be used to configure the world generator. If left blank, the default generator will be used.",
            Self::EnforceSecureProfile(_) => "Enforce the use of secure authentication.",
            Self::LevelName(_) => "The folder name for the world. Defaults to world.",
            Self::Motd(_) => "The message of the day.",
            Self::QueryPort(_) => "Port for GameSpy4 protocol server listener.",
            Self::Pvp(_) => "Enable PVP.",
            Self::GenerateStructures(_) => "Enable generation of structures.",
            Self::MaxChainedNeighborUpdates(_) => "Maximum number of chained neighbor updates.",
            Self::Difficulty(_) => "Difficulty of the server.",
            Self::NetworkCompressionThreshold(_) => "A threshold for network compression. If the size of a packet exceeds this value, it will be compressed.",
            Self::RequireResourcePack(_) => "Require clients to download resource packs.",
            Self::MaxTickTime(_) => "Maximum time in milliseconds that a single tick may take before the server watchdog stops the server with a timeout error.",
            Self::MaxPlayers(_) => "Maximum number of players that can be connected to the server at the same time.",
            Self::UseNativeTransport(_) => "Use native transport.",
            Self::OnlineMode(_) => "Enable online mode.",
            Self::EnableStatus(_) => "Enable status.",
            Self::AllowFlight(_) => "Allow flight.",
            Self::InitialDisabledPacks(_) => "A list of resource packs that will be disabled by default.",
            Self::BroadcastRconToOps(_) => "Send rcon console command outputs to all online operators.",
            Self::ViewDistance(_) => "Sets the amount of world data the server sends the client, measured in chunks in each direction of the player (radius, not diameter). It determines the server-side viewing distance.",
            Self::ResourcePackPrompt(_) => "Prompt clients to download resource packs.",
            Self::ServerIp(_) => "The IP address of the server. If left blank, the server will listen on all addresses.",
            Self::AllowNether(_) => "Allow nether.",
            Self::ServerPort(_) => "The port of the server.",
            Self::EnableRcon(_) => "Enables remote access to the server console. Not recommended to expose RCON to the internet.",
            Self::SyncChunkWrites(_) => "Sync chunk writes.",
            Self::OpPermissionLevel(_) => "The permission level for operators.",
            Self::PreventProxyConnections(_) => "Prevent connections from proxy servers.",
            Self::HideOnlinePlayers(_) => "Hide online players.",
            Self::ResourcePack(_) => "The URL of the resource pack that will be used by default.",
            Self::EntityBroadcastRangePercentage(_) => "The percentage of the world that entities can be broadcasted to.",
            Self::SimulationDistance(_) => "The distance in blocks that players will be able to interact with entities.",
            Self::RconPassword(_) => "The password for RCON connections.",
            Self::PlayerIdleTimeout(_) => "The number of minutes before a player is kicked for being idle.",
            Self::ForceGamemode(_) => "Force players to join in the specified gamemode.",
            Self::RateLimit(_) => "The maximum number of packets per second that the server will accept from a single IP address.",
            Self::Hardcore(_) => "Enable hardcore mode.",
            Self::WhiteList(_) => "Enable whitelist.",
            Self::BroadcastConsoleToOps(_) => "Send console command outputs to all online operators.",
            Self::PreviewsChat(_) => "Enable chat.",
            Self::SpawnNpcs(_) => "Enable spawning of NPCs.",
            Self::SpawnAnimals(_) => "Enable spawning of animals.",
            Self::FunctionPermissionLevel(_) => "The permission level for functions.",
            Self::InitialEnabledPacks(_) => "A list of resource packs that will be enabled by default.",
            Self::LevelType(_) => "The type of world to generate.",
            Self::TextFilteringConfig(_) => "The path to the text filtering configuration file.",
            Self::SpawnMonsters(_) => "Enable spawning of hostile mobs.",
            Self::EnforceWhitelist(_) => "Enforce the use of the whitelist.",
            Self::SpawnProtection(_) => "The radius of the spawn protection area.",
            Self::ResourcePackSha1(_) => "The SHA1 hash of the resource pack that will be used by default.",
            Self::MaxWorldSize(_) => "The maximum size of the world in blocks.",
            Self::MaxBuildHeight(_) => "The maximum height of the world in blocks.",
            Self::Unknown(_, _) => unreachable!("Already handled above.")
        }.to_string()
    }

    pub fn from_key_val(key: &str, value: &str) -> Result<Self, Error> {
        match key {
            "enable-jmx-monitoring" => Ok(Self::EnableJmxMonitoring(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value for \"enable-jmx-monitoring\": {value}, expected bool"))?,
            )),
            "rcon.port" => {
                Ok(Self::RconPort(value.parse::<u16>().with_context(|| {
                    eyre!("Invalid value: {value} for \"rcon.port\", expected u16")
                })?))
            }
            "level-seed" => Ok(Self::LevelSeed(value.to_string())),
            "gamemode" => {
                Ok(Self::Gamemode(value.parse::<Gamemode>().with_context(
                    || eyre!("Invalid value: {value} for \"gamemode\", expected Gamemode"),
                )?))
            }
            "enable-command-block" => Ok(Self::EnableCommandBlock(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"enable-command-block\", expected bool"))?,
            )),
            "enable-query" => {
                Ok(Self::EnableQuery(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"enable-query\", expected bool"),
                )?))
            }
            "generator-settings" => {
                Ok(Self::GeneratorSettings(value.parse().with_context(
                    || eyre!("Invalid value: {value} for \"generator-settings\", expected string"),
                )?))
            }
            "enforce-secure-profile" => Ok(Self::EnforceSecureProfile(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"enforce-secure-profile\", expected bool"))?,
            )),
            "level-name" => {
                Ok(Self::LevelName(value.to_string()))
            }
            "motd" => {
                Ok(Self::Motd(value.to_string()))
            }
            "query.port" => {
                Ok(Self::QueryPort(value.parse::<u16>().with_context(
                    || eyre!("Invalid value: {value} for \"query.port\", expected u16"),
                )?))
            }
            "pvp" => {
                Ok(Self::Pvp(value.parse::<bool>().with_context(|| {
                    eyre!("Invalid value: {value} for \"pvp\", expected bool")
                })?))
            }
            "generate-structures" => Ok(Self::GenerateStructures(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"generate-structure\", expected bool"))?,
            )),
            "max-chained-neighbor-updates" => Ok(Self::MaxChainedNeighborUpdates(
                value
                    .parse::<u32>()
                    .with_context(|| eyre!("Invalid value: {value} for \"max-chained-neighbor-updates\", expected u32"))?,
            )),
            "difficulty" => {
                Ok(Self::Difficulty(value.parse::<Difficulty>().with_context(
                    || eyre!("Invalid value: {value} for \"difficulty\", expected Difficulty."),
                )?))
            }
            "network-compression-threshold" => Ok(Self::NetworkCompressionThreshold(
                value
                    .parse::<u32>()
                    .with_context(|| eyre!("Invalid value: {value} for \"network-compression-threshold\", expected u32"))?,
            )),
            "require-resource-pack" => Ok(Self::RequireResourcePack(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"require-resource-pack\", expected bool"))?,
            )),
            "max-tick-time" => {
                Ok(Self::MaxTickTime(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"max-tick-time\", expected u32"),
                )?))
            }
            "use-native-transport" => Ok(Self::UseNativeTransport(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"use-native-transport\", expected bool"))?,
            )),
            "max-players" => {
                Ok(Self::MaxPlayers(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"max-players\", expected u8"),
                )?))
            }

            "online-mode" => {
                Ok(Self::OnlineMode(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"online-mode\", expected bool"),
                )?))
            }
            "enable-status" => {
                Ok(Self::EnableStatus(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"online-status\", expected bool"),
                )?))
            }
            "allow-flight" => {
                Ok(Self::AllowFlight(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"allow-flight\", expected bool"),
                )?))
            }
            "initial-disabled-packs" => Ok(Self::InitialDisabledPacks(value.to_string())),
            "broadcast-rcon-to-ops" => Ok(Self::BroadcastRconToOps(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"broadcast-rcon-to-ops\", expected bool"))?,
            )),
            "view-distance" => {
                Ok(Self::ViewDistance(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"view-distance\", expected u32"),
                )?))
            }
            "resource-pack-prompt" => Ok(Self::ResourcePackPrompt(value.to_string())),
            "server-ip" => Ok(Self::ServerIp(value.to_string())),
            "allow-nether" => {
                Ok(Self::AllowNether(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"allow-nether\", expected bool"),
                )?))
            }
            "server-port" => {
                Ok(Self::ServerPort(value.parse::<u16>().with_context(
                    || eyre!("Invalid value: {value} for \"server-port\", expected u16"),
                )?))
            }
            "enable-rcon" => {
                Ok(Self::EnableRcon(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"enable-rcon\", expected bool"),
                )?))
            }
            "sync-chunk-writes" => {
                Ok(Self::SyncChunkWrites(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"sync-chunk-writes\", expected bool"),
                )?))
            }
            "op-permission-level" => {
                Ok(Self::OpPermissionLevel(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"op-permission-level\", expected u32"),
                )?))
            }
            "prevent-proxy-connections" => Ok(Self::PreventProxyConnections(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} \"prevent-proxy-connections\", expected bool"))?,
            )),
            "hide-online-players" => Ok(Self::HideOnlinePlayers(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} \"hide-online-players\", expected bool"))?,
            )),
            "resource-pack" => Ok(Self::ResourcePack(value.to_string())),
            "entity-broadcast-range-percentage" => Ok(Self::EntityBroadcastRangePercentage(
                value
                    .parse::<u32>()
                    .with_context(|| eyre!("Invalid value: {value} for \"entity-broadcast-range-percentage\", expected u32"))?,
            )),
            "simulation-distance" => Ok(Self::SimulationDistance(
                value
                    .parse::<u32>()
                    .with_context(|| eyre!("Invalid value: {value} for \"simulation-distance\", expected u32"))?,
            )),
            "rcon.password" => Ok(Self::RconPassword(value.to_string())),
            "player-idle-timeout" => {
                Ok(Self::PlayerIdleTimeout(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"player-idle-timeout\", expected u32"),
                )?))
            }
            "force-gamemode" => {
                Ok(Self::ForceGamemode(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"force-gamemode\", expected bool"),
                )?))
            }
            "rate-limit" => {
                Ok(Self::RateLimit(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"rate-limit\", expected u32"),
                )?))
            }
            "hardcore" => {
                Ok(Self::Hardcore(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"hard-core\", expected bool"),
                )?))
            }
            "white-list" => {
                Ok(Self::WhiteList(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"white-list\", expected bool"),
                )?))
            }
            "broadcast-console-to-ops" => Ok(Self::BroadcastConsoleToOps(
                value
                    .parse::<bool>()
                    .with_context(|| eyre!("Invalid value: {value} for \"broadcast-console-to-ops\", expected bool"))?,
            )),
            "previews-chat" => {
                Ok(Self::PreviewsChat(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"previews-chat\", expected bool"),
                )?))
            }
            "spawn-npcs" => {
                Ok(Self::SpawnNpcs(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"spawn-npcs\", expected bool"),
                )?))
            }
            "spawn-animals" => {
                Ok(Self::SpawnAnimals(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"spawn-animals\", expected bool"),
                )?))
            }
            "function-permission-level" => Ok(Self::FunctionPermissionLevel(
                value
                    .parse::<u32>()
                    .with_context(|| eyre!("Invalid value: {value} for \"function-permission-level\", expected u32"))?,
            )),
            "initial-enabled-packs" => Ok(Self::InitialEnabledPacks(value.to_string())),
            "level-type" => Ok(Self::LevelType(value.to_string())),
            "text-filtering-config" => Ok(Self::TextFilteringConfig(value.to_string())),
            "spawn-monsters" => {
                Ok(Self::SpawnMonsters(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"spawn-monsters\", expected bool"),
                )?))
            }

            "enforce-whitelist" => {
                Ok(Self::EnforceWhitelist(value.parse::<bool>().with_context(
                    || eyre!("Invalid value: {value} for \"enforce-whitelist\", expected bool"),
                )?))
            }
            "spawn-protection" => {
                Ok(Self::SpawnProtection(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} \"spawn-protection\", expected u32"),
                )?))
            }
            "resource-pack-sha1" => Ok(Self::ResourcePackSha1(value.to_string())),
            "max-world-size" => {
                Ok(Self::MaxWorldSize(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"max-world-size\", expected u32"),
                )?))
            }
            "max-build-height" => {
                Ok(Self::MaxBuildHeight(value.parse::<u32>().with_context(
                    || eyre!("Invalid value: {value} for \"max-build-height\", expected u32"),
                )?))
            }
            _ => Ok(Self::Unknown(key.to_string(), value.to_string())),
        }
    }
    #[allow(dead_code)]
    pub fn to_line(&self) -> String {
        match self {
            Self::EnableJmxMonitoring(v) => format!("{}={}", self.get_identifier(), v),
            Self::RconPort(v) => format!("{}={}", self.get_identifier(), v),
            Self::LevelSeed(v) => format!("{}={}", self.get_identifier(), v),
            Self::Gamemode(v) => format!("{}={}", self.get_identifier(), v.to_string()),
            Self::EnableCommandBlock(v) => format!("{}={}", self.get_identifier(), v),
            Self::EnableQuery(v) => format!("{}={}", self.get_identifier(), v),
            Self::GeneratorSettings(v) => format!("{}={}", self.get_identifier(), v),
            Self::EnforceSecureProfile(v) => format!("{}={}", self.get_identifier(), v),
            Self::LevelName(v) => format!("{}={}", self.get_identifier(), v),
            Self::Motd(v) => format!("{}={}", self.get_identifier(), v),
            Self::QueryPort(v) => format!("{}={}", self.get_identifier(), v),
            Self::Pvp(v) => format!("{}={}", self.get_identifier(), v),
            Self::GenerateStructures(v) => format!("{}={}", self.get_identifier(), v),
            Self::MaxChainedNeighborUpdates(v) => format!("{}={}", self.get_identifier(), v),
            Self::Difficulty(v) => format!("{}={}", self.get_identifier(), v.to_string()),
            Self::NetworkCompressionThreshold(v) => format!("{}={}", self.get_identifier(), v),
            Self::RequireResourcePack(v) => format!("{}={}", self.get_identifier(), v),
            Self::MaxTickTime(v) => format!("{}={}", self.get_identifier(), v),
            Self::MaxPlayers(v) => format!("{}={}", self.get_identifier(), v),
            Self::UseNativeTransport(v) => format!("{}={}", self.get_identifier(), v),
            Self::OnlineMode(v) => format!("{}={}", self.get_identifier(), v),
            Self::EnableStatus(v) => format!("{}={}", self.get_identifier(), v),
            Self::AllowFlight(v) => format!("{}={}", self.get_identifier(), v),
            Self::InitialDisabledPacks(v) => format!("{}={}", self.get_identifier(), v),
            Self::BroadcastRconToOps(v) => format!("{}={}", self.get_identifier(), v),
            Self::ViewDistance(v) => format!("{}={}", self.get_identifier(), v),
            Self::ResourcePackPrompt(v) => format!("{}={}", self.get_identifier(), v),
            Self::ServerIp(v) => format!("{}={}", self.get_identifier(), v),
            Self::AllowNether(v) => format!("{}={}", self.get_identifier(), v),
            Self::ServerPort(v) => format!("{}={}", self.get_identifier(), v),
            Self::EnableRcon(v) => format!("{}={}", self.get_identifier(), v),
            Self::SyncChunkWrites(v) => format!("{}={}", self.get_identifier(), v),
            Self::OpPermissionLevel(v) => format!("{}={}", self.get_identifier(), v),
            Self::PreventProxyConnections(v) => format!("{}={}", self.get_identifier(), v),
            Self::HideOnlinePlayers(v) => format!("{}={}", self.get_identifier(), v),
            Self::ResourcePack(v) => format!("{}={}", self.get_identifier(), v),
            Self::EntityBroadcastRangePercentage(v) => format!("{}={}", self.get_identifier(), v),
            Self::SimulationDistance(v) => format!("{}={}", self.get_identifier(), v),
            Self::RconPassword(v) => format!("{}={}", self.get_identifier(), v),
            Self::PlayerIdleTimeout(v) => format!("{}={}", self.get_identifier(), v),
            Self::ForceGamemode(v) => format!("{}={}", self.get_identifier(), v),
            Self::RateLimit(v) => format!("{}={}", self.get_identifier(), v),
            Self::Hardcore(v) => format!("{}={}", self.get_identifier(), v),
            Self::WhiteList(v) => format!("{}={}", self.get_identifier(), v),
            Self::BroadcastConsoleToOps(v) => format!("{}={}", self.get_identifier(), v),
            Self::PreviewsChat(v) => format!("{}={}", self.get_identifier(), v),
            Self::SpawnNpcs(v) => format!("{}={}", self.get_identifier(), v),
            Self::SpawnAnimals(v) => format!("{}={}", self.get_identifier(), v),
            Self::FunctionPermissionLevel(v) => format!("{}={}", self.get_identifier(), v),
            Self::InitialEnabledPacks(v) => format!("{}={}", self.get_identifier(), v),
            Self::LevelType(v) => format!("{}={}", self.get_identifier(), v),
            Self::TextFilteringConfig(v) => format!("{}={}", self.get_identifier(), v),
            Self::SpawnMonsters(v) => format!("{}={}", self.get_identifier(), v),
            Self::EnforceWhitelist(v) => format!("{}={}", self.get_identifier(), v),
            Self::SpawnProtection(v) => format!("{}={}", self.get_identifier(), v),
            Self::ResourcePackSha1(v) => format!("{}={}", self.get_identifier(), v),
            Self::MaxWorldSize(v) => format!("{}={}", self.get_identifier(), v),
            Self::MaxBuildHeight(v) => format!("{}={}", self.get_identifier(), v),
            Self::Unknown(_k, v) => format!("{}={}", self.get_identifier(), v),
        }
    }
}

impl FromStr for ServerPropertySetting {
    type Err = Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut split = line.split('=');
        let key = split
            .next()
            .with_context(|| eyre!("Invalid line, no key: {}", line))?;
        let value = split
            .next()
            .with_context(|| eyre!("Invalid line, no value: {}", line))?;

        Self::from_key_val(key, value)
    }
}

#[cfg(test)]
mod test {
    use std::io::BufRead;

    use crate::traits::t_configurable::manifest::SectionManifest;

    use super::*;

    #[test]
    fn test_parse_server_properties() {
        let properties =
            "enable-jmx-monitoring=false\nrcon.port=25575\nlevel-seed=\ndifficulty=easy";

        let mut res: Vec<ServerPropertySetting> = Vec::new();
        for (line_num, line) in properties.lines().enumerate() {
            if let Ok(entry) = ServerPropertySetting::from_str(line) {
                res.push(entry);
            } else {
                panic!("Failed to parse line: {} at {line_num}", line);
            }
        }

        assert_eq!(res[0], ServerPropertySetting::EnableJmxMonitoring(false));

        assert_eq!(res[1], ServerPropertySetting::RconPort(25575));

        assert_eq!(res[2], ServerPropertySetting::LevelSeed("".to_string()));

        assert_eq!(res[3], ServerPropertySetting::Difficulty(Difficulty::Easy));
    }

    #[test]
    fn test_exhausiveness() {
        let properties_file = std::io::BufReader::new(
            std::fs::File::open("testdata/sample_server.properties")
                .expect("Failed to open server.properties"),
        );
        let mut config_section = SectionManifest::new(
            String::from("server_properties"),
            String::from("Server Properties Test"),
            Default::default(),
            Default::default(),
        );

        for line in properties_file.lines() {
            let line = line.expect("Failed to read line");
            match ServerPropertySetting::from_str(&line) {
                Ok(v) => {
                    if let ServerPropertySetting::Unknown(_, _) = v {
                        panic!("Unknown property: {}", line);
                    }

                    config_section.add_setting(v.into()).unwrap();
                }
                Err(e) => panic!("Failed to parse line: {} with error: {}", line, e),
            }
        }

        assert!(!config_section
            .get_setting("enable-jmx-monitoring")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_boolean()
            .unwrap());

        let property: ServerPropertySetting = config_section
            .get_setting("enable-jmx-monitoring")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(property, ServerPropertySetting::EnableJmxMonitoring(false));
        assert_eq!(
            property.to_line(),
            "enable-jmx-monitoring=false".to_string()
        );

        assert_eq!(
            config_section
                .get_setting("rcon.port")
                .unwrap()
                .get_value()
                .unwrap()
                .try_as_unsigned_integer()
                .unwrap(),
            25575
        );

        let property: ServerPropertySetting = config_section
            .get_setting("rcon.port")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();

        assert_eq!(property, ServerPropertySetting::RconPort(25575));
        assert_eq!(property.to_line(), "rcon.port=25575".to_string());

        assert!(config_section
            .get_setting("resource-pack")
            .unwrap()
            .get_value()
            .unwrap()
            .try_as_string()
            .unwrap()
            .is_empty());

        let property: ServerPropertySetting = config_section
            .get_setting("resource-pack")
            .unwrap()
            .clone()
            .try_into()
            .unwrap();

        assert_eq!(
            property,
            ServerPropertySetting::ResourcePack("".to_string())
        );

        assert_eq!(property.to_line(), "resource-pack=".to_string());
    }
}
