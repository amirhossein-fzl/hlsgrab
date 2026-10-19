# m3u8-dl

`m3u8-dl` is a command-line HLS playlist downloader written in Rust. It accepts
an M3U8 URL, lets you choose a video quality (and an audio rendition when the
playlist provides alternatives), and downloads the playlist and its media
resources to a local directory.

## Install

### Download a release

Prebuilt archives are published on the
[GitHub Releases page](https://github.com/amirhossein-fzl/m3u8-dl/releases).
Choose the archive matching your operating system and architecture, extract it,
and run the `m3u8-dl` executable (on Windows, `m3u8-dl.exe`).

Release archives are provided for:

| Operating system | Architectures |
| ---------------- | ------------- |
| Linux            | x86_64, ARM64 |
| macOS            | x86_64, ARM64 |
| Windows          | x86_64, ARM64 |

### Build from source

Install the stable Rust toolchain, then build the project:

```sh
git clone https://github.com/amirhossein-fzl/m3u8-dl.git
cd m3u8-dl
cargo build --release
```

The executable is written to `target/release/m3u8-dl` (or
`target/release/m3u8-dl.exe` on Windows).

## Usage

```sh
m3u8-dl download "https://example.com/path/playlist.m3u8"
```

For a master playlist, choose a video variant interactively. If the selected
variant has alternate audio renditions, the program downloads the audio track
as well and prompts you to choose one when there are multiple choices.

### Options

| Option                              | Description                                                                                 |
| ----------------------------------- | ------------------------------------------------------------------------------------------- |
| `-o, --output-directory <PATH>`     | Directory to write the download into. Defaults to the current directory.                    |
| `-c, --continue-download`           | Skip segment files that already exist in the output.                                        |
| `--header <HEADER>`                 | Add an HTTP header. May be repeated, for example `--header "Referer: https://example.com"`. |
| `--cookies <COOKIES>`               | Set the `Cookie` header value.                                                              |
| `-p, --proxy <URL>`                 | Use a proxy, for example `http://proxy.example:8080` or `socks5://proxy.example:1080`.      |
| `-t, --timeout <SECONDS>`           | Set the connection timeout in seconds. Defaults to `30`.                                    |
| `--max-concurrent-download <COUNT>` | Currently has no effect; segment downloads run sequentially. Defaults to `1`.               |
| `-h, --help`                        | Show help.                                                                                  |

For example, save to a named directory and send a custom header:

```sh
m3u8-dl download "https://example.com/path/playlist.m3u8" \
  --output-directory ./downloads \
  --header "Referer: https://example.com"
```

## Output

Downloads are written under `<output-directory>/media/`. A media playlist is
saved as `playlist.m3u8` alongside its segment and other playlist resources.
