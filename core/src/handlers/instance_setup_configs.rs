use crate::error::Error;
use crate::error::ErrorKind;
use crate::implementations::generic;
use crate::implementations::minecraft;
use crate::minecraft::FlavourKind;
use crate::traits::t_configurable::manifest::SetupManifest;
use crate::traits::t_configurable::GameType;
use crate::AppState;
use axum::extract::Path;
use axum::routing::get;
use axum::routing::put;
use axum::Json;
use axum::Router;
use color_eyre::eyre::eyre;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[allow(clippy::enum_variant_names)]
#[derive(Serialize, Deserialize, TS, Clone, Copy)]
#[ts(export)]
pub enum HandlerGameType {
    MinecraftJavaVanilla,
    MinecraftFabric,
    MinecraftForge,
    MinecraftPaper,
    MinecraftBedrock,
    MinecraftNeoforge,
}

impl From<HandlerGameType> for GameType {
    fn from(value: HandlerGameType) -> Self {
        match value {
            HandlerGameType::MinecraftJavaVanilla => Self::MinecraftJava,
            HandlerGameType::MinecraftFabric => Self::MinecraftJava,
            HandlerGameType::MinecraftForge => Self::MinecraftJava,
            HandlerGameType::MinecraftPaper => Self::MinecraftJava,
            HandlerGameType::MinecraftBedrock => Self::MinecraftBedrock,
            HandlerGameType::MinecraftNeoforge => Self::MinecraftJava,
        }
    }
}

impl TryFrom<HandlerGameType> for FlavourKind {
    type Error = Error;

    fn try_from(value: HandlerGameType) -> Result<Self, Error> {
        Ok(match value {
            HandlerGameType::MinecraftJavaVanilla => Self::Vanilla,
            HandlerGameType::MinecraftFabric => Self::Fabric,
            HandlerGameType::MinecraftForge => Self::Forge,
            HandlerGameType::MinecraftPaper => Self::Paper,
            HandlerGameType::MinecraftNeoforge => Self::Neoforge,
            HandlerGameType::MinecraftBedrock => {
                return Err(Error {
                    kind: ErrorKind::BadRequest,
                    source: eyre!("Programmer error: tried to convert HandlerGameType::MinecraftBedrock to FlavourKind"),
                })
            }
        })
    }
}

pub async fn get_available_games() -> Json<Vec<HandlerGameType>> {
    Json(vec![
        HandlerGameType::MinecraftJavaVanilla,
        HandlerGameType::MinecraftFabric,
        HandlerGameType::MinecraftForge,
        HandlerGameType::MinecraftPaper,
        HandlerGameType::MinecraftNeoforge,
    ])
}

pub async fn get_setup_manifest(
    Path(game_type): Path<HandlerGameType>,
) -> Result<Json<SetupManifest>, Error> {
    minecraft::MinecraftInstance::setup_manifest(&game_type.try_into()?)
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct GenericSetupManifestBody {
    pub url: String,
}

pub async fn get_generic_setup_manifest(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<GenericSetupManifestBody>,
) -> Result<Json<SetupManifest>, Error> {
    state
        .docker_bridge
        .add_to_watch_list(body.url.clone())
        .await;
    return Ok(Json(SetupManifest {
        setting_sections: Default::default(),
    }));
}

pub fn get_instance_setup_config_routes(appstate: AppState) -> Router {
    Router::new()
        .route("/games", get(get_available_games))
        .route("/setup_manifest/:game_type", get(get_setup_manifest))
        .route("/generic_setup_manifest", put(get_generic_setup_manifest))
        .with_state(appstate)
}
