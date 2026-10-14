# iPhone Stream

Workflow Alfred dengan pengendali Rust untuk memutar audio iPhone melalui USB ke output audio default Mac. Diadaptasi dari ekstensi Raycast `iphone-stream` lokal.

## Instalasi

Memerlukan macOS 14+, Alfred 5 dengan Powerpack, dan iPhone yang terhubung melalui USB.

1. Impor `dist/iPhone Stream.alfredworkflow` ke Alfred.
2. Hubungkan iPhone, buka kunci, lalu pilih **Trust This Computer** bila diminta.
3. Jalankan `imidi` untuk meminta Audio MIDI Mode aktif. Tunggu perangkat terhubung ulang. Alternatif: buka **Audio MIDI Setup**, pilih iPhone, lalu klik **Enable**.
4. Jalankan `istream`. Pada penggunaan pertama, izinkan akses Microphone untuk **iPhone Stream Helper**. Jika ditolak, periksa **System Settings → Privacy & Security → Microphone**, lalu coba lagi.

## Perintah

| Keyword | Fungsi |
| --- | --- |
| `iphone [pencarian]` | Menu dengan status stream dan volume |
| `istream` | Mulai/hentikan stream |
| `ivup` | Naikkan volume 10%, maksimum 100% |
| `ivdown` | Turunkan volume 10%, minimum 0% |
| `imidi` | Aktif/nonaktifkan Audio MIDI Mode |

Hotkey tersedia untuk keempat perintah; tetapkan tombolnya di editor workflow Alfred. External trigger: `toggle`, `volume-up`, `volume-down`, `idam-toggle`, misalnya `alfred://runtrigger/com.ariestwn.iphone-stream/toggle/`.

Volume hanya diubah saat stream aktif. Mematikan Audio MIDI Mode menghentikan stream terlebih dahulu. Menyalakan Audio MIDI Mode tidak otomatis memulai stream; tunggu iPhone muncul kembali, lalu jalankan `istream`.

## Build dan pemeriksaan

Memerlukan Xcode Command Line Tools, Python 3, dan Rust 1.85+.

```sh
./scripts/cargo.sh generate-lockfile --offline
./scripts/cargo.sh test --locked --offline
./build.sh
# Pilih target bila perlu; target Rust tersebut harus sudah terpasang:
TARGET=x86_64-apple-darwin ./build.sh
TARGET=aarch64-apple-darwin ./build.sh
```

Build menghasilkan satu arsip untuk arsitektur yang dipilih; ketiga executable memiliki arsitektur yang sama. Biner ditandatangani ad-hoc. `scripts/verify_workflow.py` memeriksa arsip serta koneksi workflow.

## Struktur

- `src/`: Rust CLI, Script Filter JSON, kontrol proses, volume, lock perintah, timeout helper, dan validasi PID.
- `swift-src/iphone-stream-daemon.swift`: helper AVFoundation untuk audio playthrough, pemantauan volume, dan penghentian saat iPhone terputus.
- `swift-src/idam-helper.swift`: akses layanan `com.apple.idamd` melalui private MobileDevice.framework, diadaptasi dari ekstensi asal. API privat bisa berubah pada pembaruan macOS/iOS; Audio MIDI Setup tetap menjadi alternatif manual.
- `scripts/package.py`: metadata workflow dan pengemasan `.alfredworkflow`.

Kode dasar dan pengendali workflow menggunakan Rust; dua helper akses platform masih Swift. Belum merupakan port seluruh helper ke Rust.

Data berada di `alfred_workflow_data`, dengan fallback `~/Library/Application Support/Alfred/Workflow Data/com.ariestwn.iphone-stream`. Berisi `state.json`, `daemon.pid`, `daemon.log`, `command.lock`, `startup.pid`, dan `startup.json`. Direktori ini terpisah dari ekstensi Raycast. Jangan menjalankan kedua stream audio sekaligus.

Helper diluncurkan sebagai aplikasi melalui LaunchServices (`open`), sehingga permintaan izin Microphone milik iPhone Stream Helper sendiri. Jangan menjalankan executable helper langsung dari Alfred: macOS dapat mengaitkan akses Microphone ke Alfred dan menghentikan helper karena metadata izin aplikasi asal tidak tersedia. Startup memiliki laporan kegagalan dan identitas percobaan; hasil peluncuran lama tidak dianggap sebagai hasil terbaru.

Untuk pengembangan, `IPHONE_STREAM_STATE_DIR` dapat menunjuk direktori uji. CLI mendukung `filter`, `status`, `start`, `stop`, serta empat perintah di atas. Jalankan CLI dari direktori workflow agar icon relatif dapat ditemukan oleh Alfred.

Hanya iPhone pertama yang ditemukan didukung. Audio saja, tanpa video atau perekaman. Perangkat audio dipilih melalui model ID CoreAudio iPhone; helper tidak memakai jalur capture `iPhone Mic` dari QuickTime. Pengujian playback, izin Microphone, cabut kabel, dan IDAM memerlukan iPhone fisik.
