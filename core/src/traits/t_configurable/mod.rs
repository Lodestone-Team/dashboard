pub mod manifest;
pub use std::path::PathBuf;

use async_trait::async_trait;
use color_eyre::eyre::eyre;
use enum_kinds::EnumKind;
pub use serde::{Deserialize, Serialize};
pub use serde_json;
use ts_rs::TS;

use self::manifest::ConfigurableManifest;
use self::manifest::ConfigurableValue;
use crate::error::Error;
use crate::error::ErrorKind;
use crate::implementations::minecraft::Flavour;
use crate::traits::GameInstance;
use crate::traits::GenericInstance;
use crate::traits::MinecraftInstance;

use crate::types::InstanceUuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum MinecraftVariant {
    Vanilla,
    Forge,
    Fabric,
    Paper,
    Spigot,
    Neoforge,
    Other { name: String },
}

/// The type of game this instance is
///
/// Meant to be consumed by frontend to display the correct icon
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, EnumKind)]
#[enum_kind(GameType, derive(Serialize, Deserialize, TS))]
#[serde(tag = "type")]
#[ts(export)]
pub enum Game {
    MinecraftJava {
        variant: MinecraftVariant,
    },
    MinecraftBedrock,
    Generic {
        game_name: GameType,       //used for identifying the "game" ("Minecraft")
        game_display_name: String, //displaying to the user what on earth this is ("MinecraftGlowstone")
    },
}

#[test]
fn export_game_type() {
    let _ = GameType::export();
}

impl From<Flavour> for Game {
    fn from(value: Flavour) -> Self {
        match value {
            Flavour::Vanilla => Self::MinecraftJava {
                variant: MinecraftVariant::Vanilla,
            },
            Flavour::Fabric { .. } => Self::MinecraftJava {
                variant: MinecraftVariant::Fabric,
            },
            Flavour::Paper { .. } => Self::MinecraftJava {
                variant: MinecraftVariant::Paper,
            },
            Flavour::Spigot => Self::MinecraftJava {
                variant: MinecraftVariant::Spigot,
            },
            Flavour::Forge { .. } => Self::MinecraftJava {
                variant: MinecraftVariant::Forge,
            },
            Flavour::Neoforge { .. } => Self::MinecraftJava {
                variant: MinecraftVariant::Neoforge,
            }
        }
    }
}

#[async_trait]
#[enum_dispatch::enum_dispatch]
pub trait TConfigurable {
    // getters
    async fn uuid(&self) -> InstanceUuid;
    async fn name(&self) -> String;
    async fn game_type(&self) -> Game;
    async fn version(&self) -> String;
    async fn description(&self) -> String;
    async fn port(&self) -> u32;
    async fn creation_time(&self) -> i64;
    async fn path(&self) -> PathBuf;
    /// does start when lodestone starts
    async fn auto_start(&self) -> bool;
    async fn restart_on_crash(&self) -> bool;
    // setters
    async fn set_name(&self, name: String) -> Result<(), Error>;
    async fn set_description(&self, description: String) -> Result<(), Error>;
    async fn set_port(&self, _port: u32) -> Result<(), Error> {
        Err(Error {
            kind: ErrorKind::UnsupportedOperation,
            source: eyre!("This instance does not support setting port"),
        })
    }
    async fn set_auto_start(&self, _auto_start: bool) -> Result<(), Error> {
        Err(Error {
            kind: ErrorKind::UnsupportedOperation,
            source: eyre!("This instance does not support setting auto start"),
        })
    }
    async fn set_restart_on_crash(&self, _restart_on_crash: bool) -> Result<(), Error> {
        Err(Error {
            kind: ErrorKind::UnsupportedOperation,
            source: eyre!("This instance does not support setting restart on crash"),
        })
    }
    async fn set_backup_period(&self, _backup_period: Option<u32>) -> Result<(), Error> {
        Err(Error {
            kind: ErrorKind::UnsupportedOperation,
            source: eyre!("This instance does not support setting backup period"),
        })
    }

    async fn change_version(&self, _version: String) -> Result<(), Error> {
        Err(Error {
            kind: ErrorKind::UnsupportedOperation,
            source: eyre!("This instance does not support changing version"),
        })
    }

    async fn configurable_manifest(&self) -> ConfigurableManifest;

    async fn update_configurable(
        &self,
        section_id: &str,
        setting_id: &str,
        value: ConfigurableValue,
    ) -> Result<(), Error>;
}
