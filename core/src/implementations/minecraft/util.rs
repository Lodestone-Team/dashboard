use color_eyre::eyre::{eyre, Context, ContextCompat};
use indexmap::IndexMap;
use serde_json::{self, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::{io::AsyncBufReadExt, process::Command};

use super::{
    FabricInstallerVersion, FabricLoaderVersion, Flavour, ForgeBuildVersion, PaperBuildVersion,
    RestoreConfig,
};
use crate::{error::Error, util::list_dir};

pub async fn create_java_launch_cmd(
    config: RestoreConfig,
    jre: PathBuf,
    path_to_instance: PathBuf,
) -> Result<Command, Error> {
    let mut server_start_command = Command::new(&jre);
    server_start_command
        .arg(format!("-Xmx{}M", config.max_ram))
        .arg(format!("-Xms{}M", config.min_ram))
        .args(
            &config
                .cmd_args
                .iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<&String>>(),
        );

    match &config.flavour {
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
                    path_to_instance
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
                let files = list_dir(&path_to_instance, Some(false))
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
                    .arg(&path_to_instance.join(forge_jar_name))
            } else {
                // 1.5 doesn't work due to JRE issues
                // 1.4 doesn't work since forge doesn't provide an installer
                let files = list_dir(&path_to_instance, Some(false))
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
                    .arg(&path_to_instance.join(server_jar_name))
            }
        }
        _ => server_start_command
            .arg("-jar")
            .arg(&path_to_instance.join("server.jar")),
    };

    server_start_command.arg("nogui");
    println!("{:?}", server_start_command);

    Ok(server_start_command)
}

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
        Flavour::Spigot => todo!(),
        Flavour::Forge { build_version } => get_forge_jar_url(version, build_version).await.ok(),
    }
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
    use crate::minecraft::{
        util::{get_forge_jar_url, get_server_jar_url},
        FabricInstallerVersion, FabricLoaderVersion, Flavour, ForgeBuildVersion, PaperBuildVersion,
    };
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
