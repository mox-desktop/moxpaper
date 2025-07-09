# Packaging Moxpaper
Moxpaper ships with two binaries:

* `daemon` – the notification daemon
* `ctl` – the control utility

For compatibility with [moxctl](https://github.com/unixpariah/moxctl), the `ctl` binary should be symlinked to `moxpaperctl`.

## Building and Renaming

```bash
cargo build --release --bin daemon
mv target/release/daemon target/release/moxpaperd
```

```bash
cargo build --release --bin ctl
mv target/release/ctl target/release/moxpaper
```

## Installation

Install both binaries to a standard location, such as `/usr/local/bin`:

```bash
install -Dm755 target/release/moxpaperd /usr/local/bin/moxpaperd
install -Dm755 target/release/moxpaper /usr/local/bin/moxpaper
ln -sf /usr/local/bin/moxpaper /usr/local/bin/moxpaperctl
```

## Systemd Integration

A systemd service file is provided in `contrib/systemd/moxpaper.service.in`.

Enable with:

```bash
systemctl --user enable --now moxpaper.service
```
