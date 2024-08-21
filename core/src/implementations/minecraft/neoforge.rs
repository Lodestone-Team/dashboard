use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;
use color_eyre::eyre::{eyre, Context};
use enum_kinds::EnumKind;
use serde_json::Value;
use ts_rs::TS;
use crate::error::Error;
use crate::traits::t_configurable::{Deserialize, Serialize};

pub async fn get_neoforge_minecraft_versions() -> Result<Vec<String>, Error> {
    let versions = request_neoforge_versions().await?;
    let mut minecraft_versions: Vec<String> = versions
        .iter()
        .map(|version| {
            format!("1.{}.{}", version.major, version.minor)
        })
        .collect();

    minecraft_versions.dedup();
    minecraft_versions.sort();

    Ok(minecraft_versions)
}

pub async fn get_neoforge_builds(minecraft_version: Option<&str>) -> Result<Vec<NeoforgeVersion>, Error> {
    let versions = request_neoforge_versions().await?;
    let build_versions = versions
        .iter()
        .filter(|version| {
            minecraft_version.is_none() || format!("1.{}.{}", version.major, version.minor) == minecraft_version.clone().unwrap()
        })
        .cloned()
        .collect::<Vec<NeoforgeVersion>>();

    Ok(build_versions)
}

pub async fn get_neoforge_latest_build(minecraft_version: Option<&str>) -> Result<NeoforgeVersion, Error> {
    let version_builds = get_neoforge_builds(minecraft_version).await?;

    let latest_version = version_builds
        .iter()
        .max_by_key(|v| v.patch())
        .expect("Failed to find latest Neoforge version.");

    Ok(latest_version.clone())
}

async fn request_neoforge_versions() -> Result<Vec<NeoforgeVersion>, Error> {
    let http = reqwest::Client::new();

    let legacy_response: Value = serde_json::from_str(
        http.get("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge")
            .send()
            .await
            .context("Failed to get legacy neoforge versions")?
            .text()
            .await
            .context("Failed to get legacy neoforge versions")?
            .as_str()
    ).context("Failed to get legacy neoforge versions")?;

    let legacy_versions = legacy_response["versions"]
        .as_array()
        .ok_or_else(|| eyre!("Failed to get legacy neoforge versions. Version array is not an array"))?
        .iter()
        .filter(|v| v.as_str().unwrap().contains("-"))
        .map(|v| {
            NeoforgeVersion::from_str(v.as_str().unwrap()).unwrap()
        })
        .collect::<Vec<NeoforgeVersion>>();

    let response: Value = serde_json::from_str(
        http.get("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge")
            .send()
            .await
            .context("Failed to get neoforge versions")?
            .text()
            .await
            .context("Failed to get neoforge versions")?
            .as_str(),
    ).context("Failed to get neoforge versions")?;

    let main_versions = response["versions"]
        .as_array()
        .ok_or_else(|| eyre!("Failed to get neoforge versions. Versions array is not an array"))?
        .iter()
        .map(|v| {
            NeoforgeVersion::from_str(v.as_str().unwrap()).unwrap()
        })
        .collect::<Vec<NeoforgeVersion>>();

    let versions = [main_versions, legacy_versions].concat();
    Ok(versions)
}

fn split_neoforge_version(version: &str) -> Result<(String, String, (String, Option<String>)), VersionError> {
    let mut split = version.split('.');

    let major_version = split.next().ok_or(VersionError::InvalidFormat)?.to_string();
    let minor_version = split.next().ok_or(VersionError::InvalidFormat)?.to_string();
    let patch_version = split.next().ok_or(VersionError::InvalidFormat)?.to_string();

    let (patch, channel) = if let Some((patch, channel)) = patch_version.split_once('-') {
        (patch.to_string(), Some(channel.to_string()))
    } else {
        (patch_version, None)
    };

    Ok((major_version, minor_version, (patch, channel)))
}

#[derive(Debug, Serialize, Deserialize, TS, Clone)]
#[ts(export)]
pub struct NeoforgeVersion {
    pub legacy: bool,
    pub major: i32,
    pub minor: i32,
    pub patch: String,
}

impl NeoforgeVersion {
    pub fn new(major: i32, minor: i32, patch: String, legacy: bool) -> NeoforgeVersion {
        NeoforgeVersion {
            major,
            minor,
            patch,
            legacy,
        }
    }
}

impl FromStr for NeoforgeVersion {
    type Err = VersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let is_legacy = value.starts_with('1');
        let value = if is_legacy {
            value.replace("1.", "").replace("-", ".").replace("47.", "")
        } else {
            value.to_string()
        };

        let (major_str, minor_str, (patch_str, _)) = split_neoforge_version(&value)?;

        let major = major_str.parse::<i32>().map_err(|_| VersionError::InvalidNumber)?;
        let minor = minor_str.parse::<i32>().map_err(|_| VersionError::InvalidNumber)?;

        Ok(NeoforgeVersion::new(major, minor, patch_str, is_legacy))
    }
}

impl PartialEq for NeoforgeVersion {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.legacy == other.legacy && self.minor == other.minor && self.patch == other.patch
    }
}

impl NeoforgeVersion {
    pub fn installer_url(&self) -> String {
        if self.legacy {
            return format!("https://maven.neoforged.net/releases/net/neoforged/forge/{}/forge-{}-installer.jar", self.version(), self.version());
        }
        format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar", self.version(), self.version())
    }

    pub fn version(&self) -> String {
        if self.legacy {
            return format!("1.{}.{}-47.1.{}", self.major, self.minor, self.patch());
        }
        format!("{}.{}.{}", self.major, self.minor, self.patch())
    }

    pub fn patch(&self) -> i32 {
        let patch_str = if self.legacy {
            self.patch.replace("47.", "")
        } else {
            self.patch.clone()
        };
        patch_str.parse::<i32>().unwrap_or(0)
    }
}

#[derive(Debug, TS)]
pub enum VersionError {
    InvalidFormat,
    InvalidNumber
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::InvalidFormat => write!(f, "Invalid version format."),
            VersionError::InvalidNumber => write!(f, "Invalid number in version."),
            _ => write!(f, "How did we get here?")
        }
    }
}

impl std::error::Error for VersionError {}
#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_get_neoforge_minecraft_versions() {
        let versions = get_neoforge_minecraft_versions().await.unwrap();

        assert!(versions.contains(&"1.20.1".to_string()));
        assert!(versions.contains(&"1.20.2".to_string()));
        assert!(versions.contains(&"1.21.0".to_string()));
        assert_eq!(versions.contains(&"1.20.2asd".to_string()), false);
    }

    #[tokio::test]
    async fn test_get_neoforge_latest_build() {
        assert_eq!(
            get_neoforge_latest_build(Some("1.20.2")).await.unwrap().version(),
            "20.2.88".to_string()
        );
        assert_eq!(
            get_neoforge_latest_build(Some("1.20.1")).await.unwrap().version(),
            "1.20.1-47.1.106".to_string()
        );
    }

    #[tokio::test]
    async fn test_get_neoforge_build_versions() {
        let versions = get_neoforge_builds(Some("1.20.2")).await.unwrap();
        assert!(versions.contains(&NeoforgeVersion::from_str("20.2.88").unwrap()));
    }
}