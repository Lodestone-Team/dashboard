use color_eyre::eyre::{eyre, Context};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::Error;
use crate::implementations::minecraft::neoforge::get_neoforge_minecraft_versions;

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export)]
pub struct MinecraftVersions {
    pub old_alpha: Vec<String>,
    pub snapshot: Vec<String>,
    pub release: Vec<String>,
}

pub async fn get_vanilla_versions() -> Result<MinecraftVersions, Error> {
    let http = reqwest::Client::new();
    let response: Value = serde_json::from_str(
        http.get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
            .send()
            .await
            .context("Failed to get vanilla versions")?
            .text()
            .await
            .context("Failed to get vanilla versions")?
            .as_str(),
    )
    .context("Failed to get vanilla versions")?;

    let versions = response["versions"]
        .as_array()
        .ok_or_else(|| eyre!("Failed to get vanilla versions. Mojang API changed?"))?;

    #[derive(Serialize, Deserialize, Debug)]
    struct Version {
        id: String,
        r#type: String,
    }

    let mut ret = MinecraftVersions {
        old_alpha: Vec::new(),
        snapshot: Vec::new(),
        release: Vec::new(),
    };

    for version in versions.iter() {
        let version: Version = serde_json::from_value(version.to_owned())
            .context("Failed to get vanilla versions. Mojang API changed?")?;
        match version.r#type.as_str() {
            "old_alpha" => ret.old_alpha.push(version.id),
            "snapshot" => ret.snapshot.push(version.id),
            "release" => ret.release.push(version.id),
            _ => {}
        }
    }
    Ok(ret)
}

// Given an array of minecraft versions, groups them into old_alpha, snapshot, release and outputs a MinecraftVersions
pub async fn group_minecraft_versions(versions: &Vec<&str>) -> Result<MinecraftVersions, Error> {
    let vanilla_versions = get_vanilla_versions().await?;
    let mut ret = MinecraftVersions {
        release: Vec::new(),
        snapshot: Vec::new(),
        old_alpha: Vec::new(),
    };

    let release: Vec<String> = vanilla_versions
        .release
        .iter()
        .map(|s| s.replace('_', "-"))
        .collect();
    let snapshot: Vec<String> = vanilla_versions
        .snapshot
        .iter()
        .map(|s| s.replace('_', "-"))
        .collect();
    let old_alpha: Vec<String> = vanilla_versions
        .old_alpha
        .iter()
        .map(|s| s.replace('_', "-"))
        .collect();

    for version_str in versions {
        let version_standard = version_str.replace('_', "-");
        if release.contains(&version_standard) {
            ret.release.push(version_str.to_string());
        }
        if snapshot.contains(&version_standard) {
            ret.snapshot.push(version_str.to_string());
        }
        if old_alpha.contains(&version_standard) {
            ret.old_alpha.push(version_str.to_string());
        }
    }

    Ok(ret)
}

pub async fn get_fabric_versions() -> Result<MinecraftVersions, Error> {
    let http = reqwest::Client::new();

    let response: Value = serde_json::from_str(
        http.get("https://meta.fabricmc.net/v2/versions")
            .send()
            .await
            .context("Failed to get fabric versions")?
            .text()
            .await
            .context("Failed to get fabric versions")?
            .as_str(),
    )
    .context("Failed to get fabric versions")?;

    let versions = response["game"]
        .as_array()
        .ok_or_else(|| eyre!("Failed to get fabric versions. Game array is not an array"))?
        .iter()
        .map(|item| {
            item["version"].as_str().ok_or_else(|| {
                eyre!("Failed to get fabric versions. Version string is not a string").into()
            })
        })
        .collect::<Result<Vec<&str>, Error>>()?; // Rust converts Vec<Result<&str, Error>> to Result<Vec<&str>, Error>

    group_minecraft_versions(&versions).await
}

pub async fn get_paper_versions() -> Result<MinecraftVersions, Error> {
    let http = reqwest::Client::new();

    let response: Value = serde_json::from_str(
        http.get("https://api.papermc.io/v2/projects/paper")
            .send()
            .await
            .context("Failed to get paper versions")?
            .text()
            .await
            .context("Failed to get paper versions")?
            .as_str(),
    )
    .context("Failed to get paper versions")?;

    let mut versions = response["versions"]
        .as_array()
        .ok_or_else(|| eyre!("Failed to get paper versions. Versions array is not an array"))?
        .iter()
        .map(|item| {
            item.as_str().ok_or_else(|| {
                eyre!("Failed to get paper versions. Versions element is not a string").into()
            })
        })
        .collect::<Result<Vec<&str>, Error>>()?;

    versions.reverse();

    group_minecraft_versions(&versions).await
}

pub async fn get_neoforge_versions() -> Result<MinecraftVersions, Error> {
    let versions_vec = get_neoforge_minecraft_versions().await?;
    let versions = versions_vec
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();
    group_minecraft_versions(&versions).await
}

pub async fn get_forge_versions() -> Result<MinecraftVersions, Error> {
    let http = reqwest::Client::new();

    let response: Value = serde_json::from_str(
        http.get("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json")
            .send()
            .await
            .context("Failed to get forge versions")?
            .text()
            .await
            .context("Failed to get forge versions")?
            .as_str(),
    )
    .context("Failed to get forge versions")?;

    let mut versions: Vec<&str> = response
        .as_object()
        .ok_or_else(|| eyre!("Failed to get forge versions. Metadata is not an object"))?
        .keys()
        .map(|s| s.as_str())
        .collect();

    versions.reverse();

    group_minecraft_versions(&versions).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_paper_versions() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(get_paper_versions()).unwrap();
    }

    #[test]
    fn test_neoforge_versions() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(get_neoforge_versions()).unwrap();
    }

    #[test]
    fn test_forge_versions() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(get_forge_versions()).unwrap();
    }
}
