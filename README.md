# mocp (Music On Console Player) - Rust Rewrite

Sebuah penulisan ulang (rewrite) modern dari **MOC** (Music On Console - `mocp`) ke dalam bahasa pemrograman **Rust**.

Proyek ini mempertahankan arsitektur **Client-Server daemon** klasik khas MOC, antarmuka visual TUI split-pane (Directory Browser & Playlist), serta antarmuka baris perintah (CLI) yang kompatibel 100% dengan skrip status bar (seperti Polybar, Waybar, i3blocks, tmux, dan conky).

---

## ✨ Fitur Utama

- **Music Library Explorer (Fitur Baru!)**:
  - Pengindeksan koleksi musik dari direktori musik (`~/Music` atau parameter `--music-dir`).
  - **Tampilan 3 Kolom**: **Artists** $\rightarrow$ **Albums** $\rightarrow$ **Tracks**.
  - **Live Search & Filter (`/`)**: Cari lagu, artis, atau album secara instan secara real-time.
  - **Caching Otomatis**: Menyimpan cache indeks perpustakaan (`~/.moc/library_cache.json`) sehingga aplikasi terbuka seketika tanpa perlu scan ulang setiap kali dibuka.
  - **Rescan Instan (`u` atau `F5`)**: Memperbarui indeks perpustakaan dari piringan/disk kapan saja.
- **Sistem Multi-View TUI**:
  - **View 1 (`1` atau `F1`)**: **Library** (Koleksi Artis, Album, dan Lagu).
  - **View 2 (`2` atau `F2`)**: **Files** (File Manager & Playlist ganda klasik MOC).
  - **View 3 (`3` atau `F3`)**: **Playlist Queue** (Daftar antrean pemutaran lagu).
- **Arsitektur Client-Server Daemon**:
  - Server audio berjalan di latar belakang (daemon) dan berkomunikasi via Unix Domain Socket (`~/.moc/socket`).
  - Menutup antarmuka TUI dengan tombol `q` tidak akan mematikan musik (detach mode). Anda dapat membuka kembali kapan saja dengan menjalankan `mocp`.
  - Keluar dan mematikan server secara menyeluruh cukup dengan menekan tombol `Q` atau `mocp -x`.
- **Dukungan Audio & Decoding Modern**:
  - Ditenagai oleh **Symphonia** (pure Rust) dan **CPAL**: mendukung berbagai format audio populer seperti MP3, FLAC, WAV, OGG/Vorbis, M4A, AAC, ALAC, dsb.
  - Ekstraksi metadata ID3v1, ID3v2, Vorbis Comments, durasi, sample rate, bitrate, dan channel.
  - Linear resampling & multi-channel remixing otomatis.
- **Antarmuka Terminal Interaktif (TUI)**:
  - Dibuat menggunakan **Ratatui** dan **Crossterm**.
  - Tampilan panel ganda (Left: File/Directory Browser, Right: Playlist).
  - Navigasi mulus, penambahan lagu/folder rekursif, penghapusan, dan pencarian direktori.
  - Panel status lengkap: visual playback state (`PLAY`, `PAUSE`, `STOP`), progress bar waktu lagu, volume level bar, serta toggle control (`[SHUFFLE]`, `[REPEAT]`, `[AUTONEXT]`).
  - Menu bantuan popup interaktif (`?` atau `h`).
- **Kompatibilitas CLI `mocp`**:
  - Perintah kontrol instan seperti `mocp -p` (play), `mocp -P` (pause), `mocp -G` (toggle pause), `mocp -s` (stop), `mocp -f` (next), `mocp -r` (prev).
  - Dukungan format info penuh `mocp -i` dan formatted string `mocp -Q <FORMAT>`.
  - Penyesuaian volume `mocp -v +5` / `mocp -v -5` / `mocp -v 80`.
  - Seeking `mocp -k +10` / `mocp -k -10` dan jump `mocp -j 50%` / `mocp -j 120s`.

---

## 🛠️ Kompilasi & Instalasi

Pastikan Anda memiliki Rust compiler dan Cargo terpasang di sistem Linux Anda.

```bash
# Clone atau masuk ke direktori proyek
cd rewrite-mocp

# Build versi release
cargo build --release

# Binary hasil kompilasi akan berada di:
./target/release/mocp

# (Opsional) Salin binary ke PATH sistem
sudo cp ./target/release/mocp /usr/local/bin/mocp
```

---

## ⌨️ Pintasan Tombol TUI (Keybindings)

| Tombol | Aksi |
|---|---|
| `1` / `F1` | Buka Tampilan **Library** (Artis, Album, Lagu) |
| `2` / `F2` | Buka Tampilan **Files** (File Manager & Playlist split) |
| `3` / `F3` | Buka Tampilan **Playlist Queue** penuh |
| `Tab` | Pindah kolom (**Artists** $\leftrightarrow$ **Albums** $\leftrightarrow$ **Tracks**) atau panel |
| `/` | Cari / Filter lagu di Library secara interaktif (*Live Search*) |
| `u` / `F5` | Pindai ulang (*Rescan*) folder Music Library |
| `Enter` | Putar lagu, album, artis, atau file yang dipilih |
| `Space` | Toggle Pause / Unpause |
| `s` | Stop pemutaran lagu |
| `n` / `>` | Lagu berikutnya (Next) |
| `b` / `<` | Lagu sebelumnya (Previous) |
| `Left` / `,` | Mundur 5 detik (Seek backward) |
| `Right` / `.` | Maju 5 detik (Seek forward) |
| `[` / `]` | Mundur / Maju 15 detik |
| `+` / `-` | Naikkan / Turunkan volume |
| `a` | Tambahkan item/album/artis/folder yang dipilih ke playlist |
| `d` / `Delete` | Hapus lagu dari playlist |
| `C` | Bersihkan (Clear) seluruh playlist |
| `S` | Toggle Shuffle |
| `R` | Toggle Repeat |
| `X` | Toggle AutoNext |
| `q` | Detach TUI (server musik tetap berjalan di latar belakang) |
| `Q` | Matikan server dan keluar dari TUI |
| `?` / `h` | Buka menu bantuan shortcuts |

---

## 💻 Penggunaan Baris Perintah (CLI Commands)

Anda dapat mengontrol pemutaran musik dari terminal lain, skrip bash, atau keyboard shortcut sistem (misal di i3, Sway, Hyprland, bspwm):

```bash
# Buka antarmuka TUI interaktif
mocp

# Putar file atau folder audio langsung
mocp /path/to/song.mp3
mocp /path/to/Music/

# Tambahkan lagu atau folder ke playlist di background
mocp -a /path/to/Music/

# Kontrol pemutaran
mocp -p            # Play
mocp -P            # Pause
mocp -U            # Unpause
mocp -G            # Toggle Pause
mocp -s            # Stop
mocp -f            # Next track
mocp -r            # Previous track

# Atur volume
mocp -v +5         # Naikkan volume 5%
mocp -v -5         # Turunkan volume 5%
mocp -v 70         # Set volume ke 70%

# Seeking / Jump
mocp -k +10        # Maju 10 detik
mocp -k -10        # Mundur 10 detik
mocp -j 50%        # Lompat ke menit 50% durasi lagu
mocp -j 60s        # Lompat ke detik ke-60

# Informasi lagu (kompatibel dengan polybar, waybar, dsb.)
mocp -i

# Format kustom
mocp -Q "%artist - %song [%tt]"

# Matikan server daemon
mocp -x
```

---

## 📂 Struktur Proyek

```text
rewrite-mocp/
├── Cargo.toml
├── src/
│   ├── audio.rs      # Audio engine: decoding (Symphonia), resampling, CPAL output
│   ├── cli.rs        # CLI argument parser (Clap) kompatibel dengan opsi MOC
│   ├── client.rs     # Client IPC Unix Domain Socket & auto-spawn daemon
│   ├── domain.rs     # Tipe data bersama, status playback, protokol request/response
│   ├── main.rs       # Entry point pengatur alur server/client/TUI
│   ├── playlist.rs   # Manajemen daftar putar, shuffle ordering, recursive scan
│   ├── server.rs     # Server daemon background & multi-client socket listener
│   ├── tags.rs       # Metadata extraction (ID3v1, ID3v2, Vorbis, format probing)
│   └── ui.rs         # TUI visual ganda (Ratatui & Crossterm)
└── README.md
```
