# mocp (Music On Console Player) - Rust Rewrite

Sebuah penulisan ulang (rewrite) modern dari **MOC** (Music On Console - `mocp`) ke dalam bahasa pemrograman **Rust**.

Proyek ini mempertahankan arsitektur **Client-Server daemon** klasik khas MOC, antarmuka visual TUI split-pane (Directory Browser & Playlist), serta antarmuka baris perintah (CLI) yang kompatibel 100% dengan skrip status bar (seperti Polybar, Waybar, i3blocks, tmux, dan conky).

---

## ✨ Fitur Utama

- **Integrasi Sistem Desktop (MPRIS & Notifikasi, Fitur Baru!)**:
  - Mendaftarkan diri sebagai service **MPRIS** (`org.mpris.MediaPlayer2.mocp`) di DBus session Anda.
  - Tombol multimedia keyboard (Play/Pause/Next/Prev/Seek) akan otomatis berfungsi secara global, tanpa konfigurasi tambahan.
  - Status lagu, metadata, dan progress bar otomatis muncul di panel sistem desktop manapun yang mendukung MPRIS (GNOME Shell, KDE Plasma, Waybar, i3status-rust, dll).
  - Mengirimkan **notifikasi desktop** (toast) otomatis setiap kali lagu berganti, lengkap dengan judul dan artis.
- **Music Library Explorer**:
  - Pengindeksan koleksi musik dari direktori musik (`~/Music` atau parameter `--music-dir`).
  - **Tampilan 3 Kolom**: **Artists** $\rightarrow$ **Albums** $\rightarrow$ **Tracks**.
  - **Fuzzy Search & Filter (`/`, Ditingkatkan!)**: Cari lagu, artis, atau album secara instan dengan algoritma *fuzzy matching* (mirip VS Code/fzf) — ketik "rdhead okc" tetap menemukan "Radiohead - OK Computer" walau tidak berurutan/lengkap.
  - **Caching Otomatis**: Menyimpan cache indeks perpustakaan (`~/.moc/library_cache.json`) sehingga aplikasi terbuka seketika tanpa perlu scan ulang setiap kali dibuka.
  - **Rescan Instan (`u` atau `F5`)**: Memperbarui indeks perpustakaan dari piringan/disk kapan saja.
- **Lirik Tersinkron Real-time (Fitur Baru!)**:
  - Tekan `L` untuk menampilkan lirik lagu yang sedang diputar, tersinkron otomatis dengan posisi waktu pemutaran.
  - Mendukung format standar `.lrc` (contoh: `lagu.mp3` berpasangan dengan `lagu.lrc` di folder yang sama).
  - Baris yang sedang dinyanyikan akan otomatis disorot dan scroll-tracking mengikuti waktu lagu.
  - Lihat `example.lrc` di root proyek untuk contoh format.
- **Dukungan Mouse Penuh & Tema Kustom (Fitur Baru!)**:
  - **Mouse**: Klik untuk pindah tab/kolom/baris, scroll wheel untuk navigasi daftar atau volume, klik progress bar untuk melompat (*seek*) ke posisi tertentu di lagu.
  - **Tema Kustom**: Salin `theme.example.toml` ke `~/.moc/theme.toml` untuk mengubah seluruh skema warna TUI (border, teks, highlight, progress bar) tanpa perlu kompilasi ulang. Mendukung nama warna, hex (`#ff5733`), dan format `rgb(r, g, b)`.
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
| `L` | Tampilkan/sembunyikan **Lirik tersinkron** (`.lrc`) dari lagu yang sedang diputar |
| `Tab` | Pindah kolom (**Artists** $\leftrightarrow$ **Albums** $\leftrightarrow$ **Tracks**) atau panel |
| `/` | Cari / Filter lagu di Library secara interaktif dengan **Fuzzy Search** |
| `u` / `F5` | Pindai ulang (*Rescan*) folder Music Library |
| **Mouse Klik** | Pilih tab, item daftar, atau lompat ke posisi di progress bar |
| **Mouse Scroll** | Navigasi daftar yang sedang fokus, atau atur volume jika kursor di luar daftar |
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

## 🖥️ Integrasi Desktop (MPRIS & Notifikasi)

Saat server `mocp` berjalan, ia otomatis terdaftar di DBus session sebagai `org.mpris.MediaPlayer2.mocp`. Anda bisa memverifikasinya dengan `playerctl` atau `busctl`:

```bash
# Menggunakan playerctl (jika terpasang)
playerctl -p mocp status
playerctl -p mocp metadata
playerctl -p mocp play-pause

# Menggunakan busctl langsung
busctl --user get-property org.mpris.MediaPlayer2.org.mpris.MediaPlayer2.mocp \
  /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player PlaybackStatus
```

Dengan ini, tombol multimedia keyboard (Play/Pause/Next/Prev) pada laptop/keyboard Anda akan otomatis mengontrol `mocp` tanpa perlu binding manual, selama desktop environment (GNOME/KDE/Waybar/dll) mendukung MPRIS. Notifikasi desktop juga akan muncul otomatis setiap kali lagu berganti.

---

## 🎤 Lirik Tersinkron (`.lrc`)

Untuk menampilkan lirik, cukup letakkan file `.lrc` dengan nama yang sama persis dengan file lagu di folder yang sama:

```text
Music/
├── Radiohead/
│   ├── OK Computer/
│   │   ├── 01 - Airbag.mp3
│   │   └── 01 - Airbag.lrc   <-- otomatis terdeteksi
```

Tekan `L` di TUI untuk membuka tampilan lirik. Baris yang sedang dinyanyikan akan otomatis disorot hijau dan mengikuti posisi waktu pemutaran secara real-time. Lihat `example.lrc` di root proyek untuk contoh format standar LRC (`[mm:ss.xx]Teks lirik`).

---

## 🎨 Kustomisasi Tema

Salin file contoh ke lokasi konfigurasi tema:

```bash
mkdir -p ~/.moc
cp theme.example.toml ~/.moc/theme.toml
```

Edit `~/.moc/theme.toml` sesuai selera. Setiap kali `mocp` dibuka, tema akan otomatis dimuat ulang dari file ini. Jika file tidak ada, tema bawaan (cyan/yellow/green) akan digunakan.

---

## 📂 Struktur Proyek

```text
rewrite-mocp/
├── Cargo.toml
├── theme.example.toml  # Contoh tema kustom (salin ke ~/.moc/theme.toml)
├── example.lrc         # Contoh format file lirik tersinkron
├── src/
│   ├── audio.rs      # Audio engine: decoding (Symphonia), resampling, CPAL output
│   ├── cli.rs        # CLI argument parser (Clap) kompatibel dengan opsi MOC
│   ├── client.rs     # Client IPC Unix Domain Socket & auto-spawn daemon
│   ├── domain.rs     # Tipe data bersama, status playback, protokol request/response
│   ├── library.rs    # Music Library: scan, cache, fuzzy search (Artists/Albums/Tracks)
│   ├── lyrics.rs     # Parser lirik tersinkron format .lrc
│   ├── main.rs       # Entry point pengatur alur server/client/TUI
│   ├── playlist.rs   # Manajemen daftar putar, shuffle ordering, recursive scan
│   ├── server.rs     # Server daemon, MPRIS, notifikasi desktop, socket listener
│   ├── tags.rs       # Metadata extraction (ID3v1, ID3v2, Vorbis, format probing)
│   ├── theme.rs      # Loader tema kustom (TOML) & parser warna (nama/hex/rgb)
│   └── ui.rs         # TUI visual ganda (Ratatui & Crossterm) + dukungan mouse
└── README.md
```
