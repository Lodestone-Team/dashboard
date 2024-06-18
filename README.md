[![Lodestone Screen Shot](/dashboard/public/assets/readmeRender.png)](https://www.lodestone.cc/)

[![Contributors][contributors-shield]][contributors-url]
[![Issues][issues-shield]][issues-url]
[![Build][workflow-shield]][workflow-url]


# Lodestone

A free, open source server hosting tool for Minecraft and other multiplayers games. Lodestone is designed to be easy to use, secure, and feature-rich. It is built with Rust, React, and TypeScript.

🔗 Get Started: [https://www.lodestone.cc/](https://www.lodestone.cc/)

## Features

- [x] Clean and intuitive UI
- [x] One-click installation and setup
- [x] Real-time server status
- [x] *(New!)* Beautiful and functional file manager (unzip, upload, download, copy, pastes, etc.)
- [x] Collaborative remote server and resource management
- [x] Priority on safety and security
- [x] User permission management
- [x] *(New!)* Extensions via macro (read more [here](https://github.com/Lodestone-Team/lodestone/wiki/Macro-and-Task))
- [x] *(New!)* Connect without port forward via playit.gg integration (read more [here](https://github.com/Lodestone-Team/lodestone/wiki/Playit.gg-Integration))
- [ ] *(New!)* **Manage Docker containers** (WIP 🚧, read more [here](https://github.com/Lodestone-Team/lodestone/wiki/Docker-Instance))

## Future Features
- [ ] Plugin and mod management
- [ ] Complete Docker integrations
- [ ] Event viewer

Have a feature request? Let us know by creating an issue!

## Supported Platforms and Architectures
- Windows (x86_64)
- Linux (x86_64 and ARM)
- MacOS (Apple Silicon)

We are deprecating support for Intel Macs due to the lack of hardware to test on. Get in contact with us if you would like to see additional platform supports.

## Lodestone CLI vs Lodestone Desktop

Lodestone CLI manages the installation and updates of Lodestone Core - the backend of Lodestone. It does not come with a UI (dashboard), so you must use a web dashboard, either hosted by us or yourself.

Lodestone Desktop integrates Lodestone Core and the dashboard into a single application. It is available for Windows only and is not considered stable yet.


## Installation

### Docker
See [here](https://github.com/Lodestone-Team/lodestone/wiki/Docker-Support)

### Windows
You can download Lodestone Desktop for Windows from the [releases page](https://github.com/Lodestone-Team/lodestone/releases). Although we recommend using the CLI, see below.


### Linux and MacOS (Apple Silicon) via CLI
Download and run [Lodestone CLI](https://github.com/Lodestone-Team/lodestone_cli).

If you would like to use the dashboard we hosts:
1. Use a chromium based browser (Chrome, Edge, Brave, etc.) and go to [https://www.lodestone.cc/](https://www.lodestone.cc/)
2. Follow this [guide](https://experienceleague.adobe.com/docs/target/using/experiences/vec/troubleshoot-composer/mixed-content.html?lang=en) to enable mixed content for the site.
3. If you have browser extensions such as HTTPS Everywhere, disable them for the site.

To see why step 2 and 3 are necessary and some possible solutions, see [here](https://github.com/Lodestone-Team/lodestone/wiki/FAQ#why-do-i-need-to-enable-mixedinsecure-content-and-disable-https-is-this-safe)

Alternatively, you can host the dashboard yourself.

## Safety & Security

Lodestone Core is written entirely in safe Rust, and uses`#![forbid(unsafe_code)]`. However **we can't guarantee the safety of the crates and binaries we link to**, as those may use unsafe rust.

Lodestone is created with security as a top priority. While most of the safety critical code such as login and permissions management have been tested thoroughly, **no formal security audit has been done for any part of Lodestone.**


## Contributing

Lodestone is still new and we have *a lot* of features planned for this year. Either way, we'd love to hear your feedback! If you have any suggestions, leave a GitHub issue or talk to us on our [Discord](https://discord.gg/PkHXRQXkf6).


## License

The Lodestone project uses the GNU Affero General Public License v3.0. See our `LICENSE` file for details.

Lodestone and all its components (dashboard, core, macros, etc.) are free for personal use, forever. 

You may use Lodestone for commercial purposes, but you must disclose the source code of any modifications you make to Lodestone. You must also disclose the source code of any software that uses Lodestone's API.

Lodestone team provides absolutely no warranty or guarantee for the software.

For an alternate (non-AGPL) license, contact us by either raising an issue or joining our [Discord](https://discord.gg/PkHXRQXkf6).


## Team

Lodestone wouldn't be here if it weren't for our contributors. Check out our [team](https://github.com/orgs/Lodestone-Team/people) here!

## Support Us

The Lodestone project is being maintained by a passionate team of University students with 0 profits. Consider [buying us a coffee](https://ko-fi.com/lodestone_team) to support our development, we would greatly appreciate it!

## Have questions?

Checkout our FAQ here for more info: https://github.com/Lodestone-Team/lodestone/wiki/FAQ

You can also join our [Discord](https://discord.gg/PkHXRQXkf6) to ask questions and help for setting up Lodestone.


<p align="right">(<a href="#top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->

[contributors-shield]: https://img.shields.io/github/contributors/Lodestone-Team/dashboard?style=for-the-badge
[contributors-url]: https://github.com/Lodestone-Team/dashboard/graphs/contributors

<!-- [forks-shield]: https://img.shields.io/github/forks/github_username/repo_name.svg?style=for-the-badge
[forks-url]: https://github.com/github_username/repo_name/network/members
[stars-shield]: https://img.shields.io/github/stars/github_username/repo_name.svg?style=for-the-badge
[stars-url]: https://github.com/github_username/repo_name/stargazers -->

[issues-shield]: https://img.shields.io/github/issues/Lodestone-Team/dashboard?style=for-the-badge
[issues-url]: https://github.com/Lodestone-Team/dashboard/issues
[workflow-shield]: https://img.shields.io/github/actions/workflow/status/Lodestone-Team/dashboard/desktop.yml?style=for-the-badge
[workflow-url]: https://github.com/Lodestone-Team/dashboard/actions
[license-shield]: https://img.shields.io/github/license/github_username/repo_name.svg?style=for-the-badge
[license-url]: https://github.com/github_username/repo_name/blob/master/LICENSE.txt
[product-screenshot]: images/screenshot.png
