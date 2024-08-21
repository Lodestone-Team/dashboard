use color_eyre::eyre::{eyre, Context, ContextCompat};
use indexmap::IndexMap;
use semver::Op;
use serde_json::{self, Value};
use std::{collections::BTreeMap, path::Path, str::FromStr};
use tokio::io::AsyncBufReadExt;

use super::{
    FabricInstallerVersion, FabricLoaderVersion, Flavour, ForgeBuildVersion, PaperBuildVersion,
};
use crate::error::Error;
use crate::implementations::minecraft::neoforge::{get_neoforge_latest_build, NeoforgeVersion};

pub async fn read_properties_from_path(
    path_to_properties: &Path,
) -> Result<IndexMap<String, String>, Error> {
    let properties_file = tokio::fs::File::open(path_to_properties)
        .await
        .context(format!(
            "Failed to open properties file at {}",
            path_to_properties.display()
        ))?;
    let buf_reader = tokio::io::BufReader::new(properties_file);
    let mut stream = buf_reader.lines();
    let mut ret = IndexMap::new();

    while let Some(line) = stream
        .next_line()
        .await
        .context("Failed to read line from properties file")?
    {
        if line.is_empty() {
            continue;
        }
        // if a line starts with '#', it is a comment, skip it
        if line.starts_with('#') {
            continue;
        }
        // split the line into key and value
        let mut split = line.split('=');
        let key = split
            .next()
            .ok_or_else(|| eyre!("Failed to read key from properties file"))?
            .trim();
        let value = split
            .next()
            .ok_or_else(|| eyre!("Failed to read value from properties file for key {}", key))?
            .trim();

        ret.insert(key.to_string(), value.to_string());
    }
    Ok(ret)
}

// Returns the jar url and the updated flavour with version information
pub async fn get_server_jar_url(version: &str, flavour: &Flavour) -> Option<(String, Flavour)> {
    match flavour {
        Flavour::Vanilla => get_vanilla_jar_url(version).await,
        Flavour::Fabric {
            loader_version,
            installer_version,
        } => get_fabric_jar_url(version, loader_version, installer_version).await,
        Flavour::Paper { build_version } => get_paper_jar_url(version, build_version).await,
        Flavour::Neoforge { build_version } => get_neoforge_jar_url(version, build_version).await,
        Flavour::Forge { build_version } => get_forge_jar_url(version, build_version).await.ok(),
        Flavour::Spigot => todo!(),
    }
}

pub async fn get_neoforge_jar_url(
    version: &str,
    neoforge_build_version: &Option<NeoforgeVersion>,
) -> Option<(String, Flavour)> {
    let latest_build = get_neoforge_latest_build(Some(version)).await.ok()?;

    let build = neoforge_build_version.as_ref().unwrap_or(&latest_build);

    Some((
        build.installer_url(),
        Flavour::Neoforge {
            build_version: Some(build.clone()),
        },
    ))
}

pub async fn get_vanilla_jar_url(version: &str) -> Option<(String, Flavour)> {
    let client = reqwest::Client::new();
    let response_text = client
        .get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let response: serde_json::Value = serde_json::from_str(&response_text).ok()?;

    let url = response
        .get("versions")?
        .as_array()?
        .iter()
        .find(|version_json| {
            version_json
                .get("id")
                .unwrap()
                .as_str()
                .unwrap()
                .eq(version)
        })?
        .get("url")?
        .as_str()?;
    let response: serde_json::Value =
        serde_json::from_str(&client.get(url).send().await.ok()?.text().await.ok()?).ok()?;
    if response["downloads"]["server"]["url"] == serde_json::Value::Null {
        return None;
    }

    Some((
        response["downloads"]["server"]["url"]
            .to_string()
            .replace('\"', ""),
        Flavour::Vanilla,
    ))
}

pub async fn get_fabric_jar_url(
    version: &str,
    fabric_loader_version: &Option<FabricLoaderVersion>,
    fabric_installer_version: &Option<FabricInstallerVersion>,
) -> Option<(String, Flavour)> {
    let mut loader_version = String::new();
    let mut installer_version = String::new();
    let client = reqwest::Client::new();

    if let (Some(FabricLoaderVersion(l)), Some(FabricInstallerVersion(i))) =
        (fabric_loader_version, fabric_installer_version)
    {
        loader_version = l.to_string();
        installer_version = i.to_string();
        return Some((
            format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
                version, loader_version, installer_version
            ),
            Flavour::Fabric {
                loader_version: Some(FabricLoaderVersion(loader_version)),
                installer_version: Some(FabricInstallerVersion(installer_version)),
            },
        ));
    }

    if fabric_loader_version.is_none() {
        loader_version = serde_json::Value::from_str(
            client
                .get(format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{}",
                    version
                ))
                .send()
                .await
                .ok()?
                .text()
                .await
                .ok()?
                .as_str(),
        )
        .ok()?
        .as_array()?
        .iter()
        .filter(|v| {
            v.get("loader")
                .unwrap()
                .get("stable")
                .unwrap()
                .as_bool()
                .unwrap()
                && v.get("intermediary")
                    .unwrap()
                    .get("stable")
                    .unwrap()
                    .as_bool()
                    .unwrap()
        })
        .max_by(|a, b| {
            let a_version = a
                .get("loader")
                .unwrap()
                .get("version")
                .unwrap()
                .as_str()
                .unwrap()
                .split('.')
                .collect::<Vec<&str>>();
            let b_version = b
                .get("loader")
                .unwrap()
                .get("version")
                .unwrap()
                .as_str()
                .unwrap()
                .split('.')
                .collect::<Vec<&str>>();
            for (a_part, b_part) in a_version.iter().zip(b_version.iter()) {
                if a_part.parse::<i32>().unwrap() > b_part.parse::<i32>().unwrap() {
                    return std::cmp::Ordering::Greater;
                } else if a_part.parse::<i32>().unwrap() < b_part.parse::<i32>().unwrap() {
                    return std::cmp::Ordering::Less;
                }
            }
            std::cmp::Ordering::Equal
        })?
        .get("loader")?
        .get("version")?
        .as_str()?
        .to_string();
    }

    if fabric_installer_version.is_none() {
        installer_version = serde_json::Value::from_str(
            client
                .get("https://meta.fabricmc.net/v2/versions/installer")
                .send()
                .await
                .ok()?
                .text()
                .await
                .ok()?
                .as_str(),
        )
        .ok()?
        .as_array()?
        .iter()
        .filter(|v| v.get("stable").unwrap().as_bool().unwrap())
        .max_by(|a, b| {
            // sort the version string in the form of "1.2.3"
            let a_version = a
                .get("loader")
                .unwrap()
                .get("version")
                .unwrap()
                .as_str()
                .unwrap()
                .split('.')
                .collect::<Vec<&str>>();
            let b_version = b
                .get("loader")
                .unwrap()
                .get("version")
                .unwrap()
                .as_str()
                .unwrap()
                .split('.')
                .collect::<Vec<&str>>();
            for (a_part, b_part) in a_version.iter().zip(b_version.iter()) {
                if a_part.parse::<i32>().unwrap() > b_part.parse::<i32>().unwrap() {
                    return std::cmp::Ordering::Greater;
                } else if a_part.parse::<i32>().unwrap() < b_part.parse::<i32>().unwrap() {
                    return std::cmp::Ordering::Less;
                }
            }
            std::cmp::Ordering::Equal
        })?
        .get("version")?
        .as_str()?
        .to_string();
    }
    Some((
        format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
            version, loader_version, installer_version
        ),
        Flavour::Fabric {
            loader_version: Some(FabricLoaderVersion(loader_version)),
            installer_version: Some(FabricInstallerVersion(installer_version)),
        },
    ))
}

pub async fn get_paper_jar_url(
    version: &str,
    paper_build_version: &Option<PaperBuildVersion>,
) -> Option<(String, Flavour)> {
    let client = reqwest::Client::new();

    let builds_text = client
        .get(format!(
            "https://api.papermc.io/v2/projects/paper/versions/{}/builds/",
            version
        ))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let builds: serde_json::Value = serde_json::from_str(&builds_text).ok()?;
    let mut builds = builds.get("builds")?.as_array()?.iter();

    let build = if let Some(PaperBuildVersion(b)) = paper_build_version {
        builds.find(|build| build.get("build").unwrap().as_i64().unwrap().eq(b))?
    } else {
        builds
            .filter(|build| {
                build
                    .get("channel")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
                    .eq("default")
            })
            .max_by(|a, b| {
                let a = a.get("build").unwrap().as_i64().unwrap();
                let b = b.get("build").unwrap().as_i64().unwrap();
                a.cmp(&b)
            })?
    };
    let build_version = build.get("build")?.as_i64()?;

    Some((
        format!(
            "https://api.papermc.io/v2/projects/paper/versions/{}/builds/{}/downloads/{}",
            version,
            build_version,
            build
                .get("downloads")?
                .get("application")?
                .get("name")?
                .as_str()?,
        ),
        Flavour::Paper {
            build_version: Some(PaperBuildVersion(build_version)),
        },
    ))
}

pub async fn get_forge_jar_url(
    version: &str,
    forge_build_version: &Option<ForgeBuildVersion>,
) -> Result<(String, Flavour), Error> {
    let client = reqwest::Client::new();

    let response: BTreeMap<String, Vec<String>> = serde_json::from_str(
        client
            .get("https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json")
            .send()
            .await
            .context("Failed to get forge versions, http request failed")?
            .text()
            .await
            .context("Failed to get forge versions, text conversion failed")?
            .as_str(),
    )
    .context("Failed to get forge versions, json is not a map")?;

    let build = if let Some(ForgeBuildVersion(b)) = forge_build_version {
        b
    } else {
        response
            .get(version)
            .context("Failed to get forge versions, version not found")?
            .last()
            .context("Failed to get forge versions, no builds found")?
    };

    Ok((
        format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}/forge-{}-installer.jar",
            build, build
        ),
        Flavour::Forge {
            build_version: Some(ForgeBuildVersion(build.to_string())),
        },
    ))
}

pub async fn get_jre_url(version: &str) -> Option<(String, u64)> {
    let client = reqwest::Client::new();
    let os = if std::env::consts::OS == "macos" {
        "mac"
    } else {
        std::env::consts::OS
    };
    let arch = if std::env::consts::ARCH == "x86_64" {
        "x64"
    } else {
        std::env::consts::ARCH
    };

    let major_java_version = {
        let val = match serde_json::Value::from_str(
            client
                .get(
                    serde_json::Value::from_str(
                        client
                            .get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
                            .send()
                            .await
                            .ok()?
                            .text()
                            .await
                            .ok()?
                            .as_str(),
                    )
                    .ok()?
                    .get("versions")?
                    .as_array()?
                    .iter()
                    .find(|v| v.get("id").unwrap().as_str().unwrap().eq(version))?
                    .get("url")?
                    .as_str()?,
                )
                .send()
                .await
                .ok()?
                .text()
                .await
                .ok()?
                .as_str(),
        )
        .ok()?
        .get("javaVersion")
        {
            Some(java_version) => java_version.get("majorVersion")?.as_u64()?,
            None => 8,
        };
        // Ddoptium won't provide java 16 for some reason
        // updateing to 17 should be safe, and 17 is preferred since its LTS
        if val == 16 {
            17
        } else {
            val
        }
    };

    Some((
        format!(
            "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jre/hotspot/normal/eclipse",
            major_java_version, os, arch
        ),
        major_java_version,
    ))
}

pub async fn name_to_uuid(name: impl AsRef<str>) -> Option<String> {
    // GET https://api.mojang.com/users/profiles/minecraft/<username>
    let client = reqwest::Client::new();
    let res: Value = client
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{}",
            name.as_ref()
        ))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    Some(res["id"].as_str()?.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::implementations::minecraft::neoforge::NeoforgeVersion;
    use crate::minecraft::{
        util::{get_forge_jar_url, get_server_jar_url},
        FabricInstallerVersion, FabricLoaderVersion, Flavour, ForgeBuildVersion, PaperBuildVersion,
    };
    use std::str::FromStr;
    use tokio;

    #[tokio::test]
    async fn test_get_vanilla_jar_url() {
        assert_eq!(super::get_vanilla_jar_url("1.18.2").await, Some(("https://piston-data.mojang.com/v1/objects/c8f83c5655308435b3dcf03c06d9fe8740a77469/server.jar".to_string(), Flavour::Vanilla)));
        assert_eq!(super::get_vanilla_jar_url("21w44a").await, Some(("https://piston-data.mojang.com/v1/objects/ae583fd57a8c07f2d6fbadce1ce1e1379bf4b32d/server.jar".to_string(), Flavour::Vanilla)));
        assert_eq!(super::get_vanilla_jar_url("1.8.4").await, Some(("https://launcher.mojang.com/v1/objects/dd4b5eba1c79500390e0b0f45162fa70d38f8a3d/server.jar".to_string(), Flavour::Vanilla)));

        assert_eq!(super::get_vanilla_jar_url("1.8.4asdasd").await, None);
    }
    #[tokio::test]
    async fn test_get_jre_url() {
        // let os_str = if std::env::consts::OS == "macos" {
        //     "mac"
        // } else {
        //     std::env::consts::OS
        // };
        // TODO: Make this test more robust
        // assert_eq!(super::get_jre_url("1.18.2").await, Some((format!("https://api.adoptium.net/v3/binary/latest/17/ga/{os_str}/x64/jre/hotspot/normal/eclipse"), 17)));
        // assert_eq!(super::get_jre_url("21w44a").await, Some((format!("https://api.adoptium.net/v3/binary/latest/17/ga/{os_str}/x64/jre/hotspot/normal/eclipse"), 17)));
        // assert_eq!(super::get_jre_url("1.8.4").await, Some((format!("https://api.adoptium.net/v3/binary/latest/8/ga/{os_str}/x64/jre/hotspot/normal/eclipse"), 8)));

        assert_eq!(super::get_jre_url("1.8.4asdasd").await, None);
    }

    /// Test subject to fail if fabric updates their installer or loader
    #[tokio::test]
    async fn test_get_fabric_jar_url() {
        assert_eq!(
            super::get_fabric_jar_url(
                "1.19",
                &Some(FabricLoaderVersion("0.14.8".to_string())),
                &Some(FabricInstallerVersion("0.11.0".to_string()))
            )
            .await,
            Some((
                "https://meta.fabricmc.net/v2/versions/loader/1.19/0.14.8/0.11.0/server/jar"
                    .to_string(),
                Flavour::Fabric {
                    loader_version: Some(FabricLoaderVersion("0.14.8".to_string())),
                    installer_version: Some(FabricInstallerVersion("0.11.0".to_string()))
                }
            ))
        );
        assert!(super::get_fabric_jar_url("21w44a", &None, &None)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn test_get_neoforge_jar_url() {
        assert_eq!(
            super::get_neoforge_jar_url(
                "1.20.2",
                &Some(NeoforgeVersion::from_str("20.2.88")).unwrap().ok()).await,
            Some((
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/20.2.88/neoforge-20.2.88-installer.jar".to_string(),
                Flavour::Neoforge {
                    build_version: Some(super::NeoforgeVersion::from_str("20.2.88").unwrap())
                }
            ))
        );

        assert_eq!(
            super::get_neoforge_jar_url("1.21.0", &None).await,
            Some(("https://maven.neoforged.net/releases/net/neoforged/neoforge/21.0.167/neoforge-21.0.167-installer.jar".to_string(),
                  Flavour::Neoforge { build_version: Some(super::NeoforgeVersion::from_str("21.0.167").unwrap()) }
            ))
        );

        assert_eq!(
            super::get_neoforge_jar_url("1.20.2", &None).await,
            Some((
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/20.2.88/neoforge-20.2.88-installer.jar".to_string(),
                Flavour::Neoforge { build_version: Some(super::NeoforgeVersion::from_str("20.2.88").unwrap()) }
            ))
        );

        assert_eq!(
            super::get_neoforge_jar_url("1.20.1", &None).await,
            Some((
                "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar".to_string(),
                Flavour::Neoforge { build_version: Some(super::NeoforgeVersion::from_str("1.20.1-47.1.106").unwrap()) }
            ))
        );
    }

    #[tokio::test]
    async fn test_get_paper_jar_url() {
        assert_eq!(super::get_paper_jar_url("1.19.3", &Some(PaperBuildVersion(308))).await, Some((
            "https://api.papermc.io/v2/projects/paper/versions/1.19.3/builds/308/downloads/paper-1.19.3-308.jar".to_string(),
            Flavour::Paper { build_version: Some(PaperBuildVersion(308)) }
        )));
        assert_eq!(super::get_paper_jar_url("1.13-pre7", &Some(PaperBuildVersion(1))).await, Some((
            "https://api.papermc.io/v2/projects/paper/versions/1.13-pre7/builds/1/downloads/paper-1.13-pre7-1.jar".to_string(),
            Flavour::Paper { build_version: Some(PaperBuildVersion(1)) }
        )));
        assert_eq!(super::get_paper_jar_url("1.19", &None).await, Some((
            "https://api.papermc.io/v2/projects/paper/versions/1.19/builds/81/downloads/paper-1.19-81.jar".to_string(),
            Flavour::Paper { build_version: Some(PaperBuildVersion(81)) }
        )));

        assert_eq!(super::get_paper_jar_url("1.19.3bruh", &None).await, None);
    }

    #[tokio::test]
    async fn test_get_forge_jar_url() {
        get_forge_jar_url("1.18.2", &None).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_server_jar_url() {
        assert_eq!(
            get_server_jar_url("1.7.10", &Flavour::Forge { build_version: None }).await,
            Some((
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar".to_string(),
                Flavour::Forge { build_version: Some(ForgeBuildVersion("1.7.10-10.13.4.1614-1.7.10".to_string())) }
            ))
        );
        assert_eq!(
            get_server_jar_url("1.7.10_pre4", &Flavour::Forge { build_version: None }).await,
            Some((
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10_pre4-10.12.2.1149-prerelease/forge-1.7.10_pre4-10.12.2.1149-prerelease-installer.jar".to_string(),
                Flavour::Forge { build_version: Some(ForgeBuildVersion("1.7.10_pre4-10.12.2.1149-prerelease".to_string())) }
            ))
        );
        assert_eq!(
            get_server_jar_url(
                "1.19.3bruh",
                &Flavour::Forge {
                    build_version: None
                }
            )
            .await,
            None
        );
    }
}
