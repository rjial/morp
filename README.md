# morp (Music On Rust Player)

A modern rewrite of the classic **mocp** (Music On Console Player) in **Rust**.

The project preserves the classic **client-server daemon** architecture, TUI split-pane interface (Directory Browser & Playlist), and a CLI fully compatible with status bar scripts (Polybar, Waybar, i3blocks, tmux, conky, etc.).

---

## ✨ Features

- **Desktop Integration (MPRIS & Notifications)**:
  - Registers as an **MPRIS** service (`org.mpris.MediaPlayer2.morp`) on the DBus session.
  - Multimedia keyboard buttons (Play/Pause/Next/Prev/Seek) work globally out of the box, no extra configuration needed.
  - Song status, metadata, and progress bar appear automatically in any desktop panel that supports MPRIS (GNOME Shell, KDE Plasma, Waybar, i3status-rust, etc.).
  - Sends **desktop notifications** (toast) automatically on every track change, with title and artist.
- **Music Library Explorer**:
  - Indexes your music collection from a music directory (`~/Music` or `--music-dir`).
  - **3-Column View**: **Artists** → **Albums** → **Tracks**.
  - **Fuzzy Search & Filter (`/`)**: Search songs, artists, or albums instantly with fuzzy matching (like VS Code/fzf) — typing "rdhead okc" still finds "Radiohead - OK Computer".
  - **Auto-caching**: Saves a library index cache (`~/.morp/library_cache.json`) so the app opens instantly without rescanning on every launch.
  - **Instant Rescan (`u` or `F5`)**: Refresh the library index from disk at any time.
- **Real-time Synced Lyrics**:
  - Press `L` to display synced lyrics for the currently playing song, automatically synchronized to the playback position.
  - Supports the standard `.lrc` format (e.g., `song.mp3` paired with `song.lrc` in the same folder).
  - The active line is highlighted and auto-scrolls to follow the song in real time.
  - See `example.lrc` in the project root for the LRC format example.
- **Full Mouse Support & Custom Themes**:
  - **Mouse**: Click to switch tabs/columns/rows, scroll wheel to navigate lists or adjust volume, click the progress bar to seek to any position.
  - **Custom Themes**: Copy `theme.example.toml` to `~/.morp/theme.toml` to change the entire TUI color scheme (borders, text, highlights, progress bar) without recompiling. Supports color names, hex (`#ff5733`), and `rgb(r, g, b)` format.
- **Multi-View TUI**:
  - **View 1 (`1` or `F1`)**: **Library** (Artists, Albums, and Tracks collection).
  - **View 2 (`2` or `F2`)**: **Files** (File Manager & Playlist split-pane, classic mocp layout).
  - **View 3 (`3` or `F3`)**: **Playlist Queue** (full playback queue).
- **Client-Server Daemon Architecture**:
  - The audio server runs in the background (daemon) and communicates via Unix Domain Socket (`~/.morp/socket`).
  - Closing the TUI with `q` does not stop the music (detach mode). Reopen it at any time by running `morp`.
  - Fully quit and shut down the server with `Q` or `morp -x`.
- **Modern Audio Decoding**:
  - Powered by **Symphonia** (pure Rust) and **CPAL**: supports MP3, FLAC, WAV, OGG/Vorbis, M4A, AAC, ALAC, and more.
  - Extracts ID3v1, ID3v2, Vorbis Comments metadata, duration, sample rate, bitrate, and channel info.
  - Automatic linear resampling & multi-channel remixing.
- **Interactive Terminal UI (TUI)**:
  - Built with **Ratatui** and **Crossterm**.
  - Dual-panel layout (Left: File/Directory Browser, Right: Playlist).
  - Smooth navigation, recursive directory/folder adding, deletion, and directory search.
  - Full status panel: visual playback state (`PLAY`, `PAUSE`, `STOP`), song progress bar, volume bar, and toggle controls (`[SHUFFLE]`, `[REPEAT]`, `[AUTONEXT]`).
  - Interactive popup help menu (`?` or `h`).
- **Last.fm / Libre.fm Scrobbling**:
  - Automatically submits played tracks to **Last.fm** or **Libre.fm** per Audioscrobbler 2.0 rules (submitted after 50% of duration or 4 minutes, minimum 30 seconds track length).
  - Browser-based authentication (web auth flow + polling) — no username/password stored in the app.
  - API key can be embedded at compile time (`LASTFM_API_KEY=xxx cargo build --release`) so binary users need no manual configuration.
  - Use `morp --lastfm-auth` or `morp --librefm-auth` to authenticate.
- **Localization (i18n)**:
  - TUI interface, status messages, and CLI output available in **English** and **Bahasa Indonesia**.
  - Auto-detects language from `MORP_LANG` or the system `LANG` environment variable.
  - Easy to add new languages by creating a YAML file in the `locales/` directory.
- **Full CLI Compatibility**:
  - Instant control commands: `morp -p` (play), `morp -P` (pause), `morp -G` (toggle pause), `morp -s` (stop), `morp -f` (next), `morp -r` (prev).
  - Full info output `morp -i` and formatted string `morp -Q <FORMAT>`.
  - Volume adjustment: `morp -v +5` / `morp -v -5` / `morp -v 80`.
  - Seeking: `morp -k +10` / `morp -k -10` and jump `morp -j 50%` / `morp -j 120s`.

---

## 🛠️ Build & Installation

Requires Rust and Cargo installed on a Linux system.

```bash
# Enter the project directory
cd rewrite-mocp

# Build release binary
cargo build --release

# Binary will be at:
./target/release/morp

# (Optional) Install to system PATH
sudo cp ./target/release/morp /usr/local/bin/morp
```

---

## ⌨️ TUI Keybindings

| Key | Action |
|---|---|
| `1` / `F1` | Switch to **Library** view (Artists, Albums, Tracks) |
| `2` / `F2` | Switch to **Files** view (File Manager & Playlist split) |
| `3` / `F3` | Switch to full **Playlist Queue** view |
| `L` | Toggle **synced Lyrics** view (`.lrc` sidecar file) |
| `Tab` | Switch column (**Artists** ↔ **Albums** ↔ **Tracks**) or panel |
| `/` | Fuzzy Search / Filter tracks in Library |
| `u` / `F5` | Rescan Music Library from disk |
| **Mouse Click** | Select tab, list item, or seek via progress bar click |
| **Mouse Scroll** | Navigate focused list, or adjust volume when outside a list |
| `Enter` | Play selected track, album, artist, or file |
| `Space` | Toggle Pause / Unpause |
| `s` | Stop playback |
| `n` / `>` | Next track |
| `b` / `<` | Previous track |
| `Left` / `,` | Seek backward 5s |
| `Right` / `.` | Seek forward 5s |
| `[` / `]` | Seek backward / forward 15s |
| `+` / `-` | Volume Up / Down |
| `a` | Add selected item/album/artist/folder to playlist |
| `d` / `Delete` | Remove track from playlist |
| `C` | Clear entire playlist |
| `S` | Toggle Shuffle |
| `R` | Toggle Repeat |
| `X` | Toggle AutoNext |
| `q` | Detach TUI (music server keeps playing in the background) |
| `Q` | Shutdown server and quit |
| `?` / `h` | Open keyboard shortcuts help |

---

## 💻 CLI Commands

Control playback from another terminal, a bash script, or system keyboard shortcuts (i3, Sway, Hyprland, bspwm, etc.):

```bash
# Open interactive TUI
morp

# Play an audio file or directory directly
morp /path/to/song.mp3
morp /path/to/Music/

# Add a song or folder to the playlist in the background
morp -a /path/to/Music/

# Playback control
morp -p            # Play
morp -P            # Pause
morp -U            # Unpause
morp -G            # Toggle Pause
morp -s            # Stop
morp -f            # Next track
morp -r            # Previous track

# Volume
morp -v +5         # Volume up 5%
morp -v -5         # Volume down 5%
morp -v 70         # Set volume to 70%

# Seek / Jump
morp -k +10        # Seek forward 10s
morp -k -10        # Seek backward 10s
morp -j 50%        # Jump to 50% of track duration
morp -j 60s        # Jump to second 60

# Song info (compatible with polybar, waybar, etc.)
morp -i

# Custom format output
morp -Q "%artist - %song [%tt]"

# Shut down the daemon server
morp -x
```

---

## 🖥️ Desktop Integration (MPRIS & Notifications)

When the `morp` server is running, it automatically registers on the DBus session as `org.mpris.MediaPlayer2.morp`. Verify with `playerctl` or `busctl`:

```bash
# Using playerctl (if installed)
playerctl -p morp status
playerctl -p morp metadata
playerctl -p morp play-pause

# Using busctl directly
busctl --user get-property org.mpris.MediaPlayer2.org.mpris.MediaPlayer2.morp \
  /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlaybackStatus
```

Multimedia keyboard buttons (Play/Pause/Next/Prev) will automatically control `morp` with no manual binding required, as long as your desktop environment (GNOME/KDE/Waybar/etc.) supports MPRIS. Desktop notifications appear automatically on every track change.

---

## 🎤 Synced Lyrics (`.lrc`)

To display lyrics, place a `.lrc` file with the exact same name as the song file in the same folder:

```text
Music/
├── Radiohead/
│   ├── OK Computer/
│   │   ├── 01 - Airbag.mp3
│   │   └── 01 - Airbag.lrc   <-- auto-detected
```

Press `L` in the TUI to open the lyrics view. The active line is highlighted and scrolls automatically to follow the playback position in real time. See `example.lrc` in the project root for the standard LRC format (`[mm:ss.xx]Lyric text`).

---

## 🎨 Custom Themes

Copy the example file to the theme config location:

```bash
mkdir -p ~/.morp
cp theme.example.toml ~/.morp/theme.toml
```

Edit `~/.morp/theme.toml` to your preference. The theme is reloaded every time `morp` is opened. If the file does not exist, the default theme (cyan/yellow/green) is used.

---

## 🌐 Localization (i18n)

morp supports multiple languages via YAML locale files in the `locales/` directory. Currently available:

| Code | Language |
|------|----------|
| `en` | English (default) |
| `id` | Bahasa Indonesia |

### Setting the Language

**Option 1 — `MORP_LANG` environment variable** (highest priority):
```bash
MORP_LANG=id morp       # Force Bahasa Indonesia
MORP_LANG=en morp       # Force English
```

**Option 2 — System `LANG` variable** (auto-detected):
```bash
# If LANG starts with "id" (e.g. id_ID.UTF-8), morp uses Bahasa Indonesia automatically
export LANG=id_ID.UTF-8
morp
```

**Default**: If neither variable is set, morp uses **English**.

### Adding a New Language

Create a `locales/<code>.yml` file following the same structure as `locales/en.yml`, then recompile:
```bash
cargo build --release
```

---

## 🎵 Last.fm / Libre.fm Scrobbling

morp supports scrobbling to **Last.fm** and **Libre.fm** per Audioscrobbler 2.0 rules (track submitted after 50% of duration or 4 minutes, minimum track length 30 seconds).

### Authentication Setup

```bash
# Last.fm
morp --lastfm-auth

# Libre.fm
morp --librefm-auth
```

This opens a browser for you to log in and grant access. The session key is saved to `~/.morp/scrobble.toml` after successful authentication.

### Build with Embedded API Key (Binary Distribution)

```bash
LASTFM_API_KEY=xxx LASTFM_API_SECRET=yyy cargo build --release
```

Users of the resulting binary do not need to configure API keys manually — they just run `morp --lastfm-auth` to log in.

---

## 📂 Project Structure

```text
rewrite-mocp/
├── Cargo.toml
├── theme.example.toml  # Custom theme example (copy to ~/.morp/theme.toml)
├── example.lrc         # Synced lyrics format example
├── locales/
│   ├── en.yml          # English locale (default)
│   └── id.yml          # Bahasa Indonesia locale
├── src/
│   ├── audio.rs      # Audio engine: decoding (Symphonia), resampling, CPAL output
│   ├── cli.rs        # CLI argument parser (Clap), compatible with classic mocp flags
│   ├── client.rs     # Unix Domain Socket client & auto-spawn daemon
│   ├── domain.rs     # Shared data types, playback status, request/response protocol
│   ├── library.rs    # Music Library: scan, cache, fuzzy search (Artists/Albums/Tracks)
│   ├── lyrics.rs     # Synced lyrics parser (.lrc format)
│   ├── main.rs       # Entry point: server/client/TUI routing + i18n init
│   ├── playlist.rs   # Playlist management, shuffle ordering, recursive scan
│   ├── scrobble.rs   # Last.fm/Libre.fm scrobbling (Audioscrobbler 2.0)
│   ├── server.rs     # Daemon server, MPRIS, desktop notifications, socket listener
│   ├── tags.rs       # Metadata extraction (ID3v1, ID3v2, Vorbis, format probing)
│   ├── theme.rs      # Custom theme loader (TOML) & color parser (name/hex/rgb)
│   └── ui.rs         # TUI (Ratatui & Crossterm) with mouse support
└── README.md
```
