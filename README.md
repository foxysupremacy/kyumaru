# Kyumaru (きゅーまる)
### Zero-Latency Hachimi Plugin & Direct Browser Sync for AlmondEyeDB

Kyumaru is an ultra-lightweight **Hachimi plugin** for *Umamusume: Pretty Derby* (DMM PC) designed to extract player inventory (owned support cards, limit break levels, character unlocks) and Hall of Fame veterans, saving them locally to disk and syncing them directly to the **AlmondEyeDB** web application without network proxies, backend servers, or in-game performance degradation.

---

## 1. The Core Idea

### Problems with Existing Sniffers (e.g. CarrotJuicer)
* **High Latency & CPU Overhead**: Tools like CarrotJuicer run an HTTP/HTTPS MITM proxy that intercepts, decrypts, and re-encrypts every single packet passing between the game and Cygames servers.
* **Complex Setup**: Requires custom root SSL certificates, system proxy redirection, and external dependencies.
* **Always Active**: Continues inspecting network traffic even during races and live gameplay where latency can cause lag or connection drops.

### The Kyumaru Solution
* **Hook Transport Decompression (`libnative.dll`)**: Hooks `LZ4_decompress_safe_ext` from Cygames' native communication DLL (`libnative.dll`) directly, intercepting raw unencrypted MessagePack responses before IL2CPP deserialization. This bypasses IL2CPP type guessing and `ObscuredInt` XOR obfuscation completely.
* **Fast Byte-Slice Filtering (< 5µs)**: Scans decompressed packets for `b"support_card_list"`. Normal packets (99.99%) return immediately without running any deserializer or causing frame drops.
* **Zero-Latency Offloading**: When inventory arrives (`load/index`), the decompressed buffer is handed off to a background thread to parse MessagePack (`rmpv`), save files, and prepare the sync URL. The game hook returns in `<0.05ms`.
* **Dual Persistence**:
  1. **Automatic Local Disk Saves**: Writes full uncompressed JSON files to `%USERPROFILE%\Documents\Kyumaru\`:
     * `inventory.json`: Complete support card inventory and character roster.
     * `veterans.json`: Hall of Fame character records with lineage, skills, and factors.
  2. **One-Click Browser Sync**: Compactly packs `{ cards: { [id]: lb }, umas: { [id]: [rarity, talent] } }` with Deflate + Base64url into a URL hash fragment (`#data=...`).
* **In-Game Overlay UI**: Renders a clean status widget and `[ Open in Browser & Sync ]` button inside Hachimi's native in-game overlay menu.
* **100% Client-Side Privacy**: Browsers **never** send URL hash fragments to web servers. The web app reads the hash locally, writes directly to `window.localStorage`, and wipes the hash from the address bar.

---

## 2. Architecture & Data Flow

```
┌────────────────────────────────────────────────────────────────────────┐
│                        In-Game (Kyumaru Plugin)                        │
├────────────────────────────────────────────────────────────────────────┤
│ 1. Game Session Start (libnative.dll -> LZ4_decompress_safe_ext)       │
│    └─ Hook intercepts decompressed packet buffer                       │
│    └─ Fast byte check: contains b"support_card_list"?                  │
│    └─ Offloads to background worker thread (<0.05ms return time)       │
│    └─ Worker unpacks MessagePack with rmpv (352 cards, 43 umas)        │
│    └─ Auto-saves %USERPROFILE%\Documents\Kyumaru\inventory.json        │
│    └─ Auto-saves %USERPROFILE%\Documents\Kyumaru\veterans.json         │
│    └─ Compresses with Deflate -> Base64URL (~1.2 KB)                   │
│    └─ In-Game Toast: "Kyumaru: Captured 352 support cards, 43 umas!"   │
│                                                                        │
│ 2. In-Game Hachimi Overlay Menu                                        │
│    └─ Shows: "Status: Ready (352 cards, 43 Umas)"                      │
│    └─ Shows: "Saved: Documents/Kyumaru/inventory.json"                 │
│    └─ User Clicks: [ Open in Browser & Sync ]                          │
│         Spawns worker thread -> Launches default browser via           │
│         rundll32 url.dll,FileProtocolHandler                           │
│         "https://database.almond-eye.tech/import/#data=<payload>"      │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Browser Launch (URL Hash)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                      Browser (AlmondEyeDB Web App)                     │
├────────────────────────────────────────────────────────────────────────┤
│ 4. Page Mount / Hash Event                                             │
│    └─ Reads window.location.hash (#data=...)                           │
│    └─ Decompresses payload via native browser DecompressionStream      │
│    └─ Writes directly to window.localStorage                           │
│         • localStorage.setItem("almondeye_owned_cards", ...)           │
│         • localStorage.setItem("almondeye_owned_umas", ...)            │
│    └─ Cleans address bar: history.replaceState(null, "", pathname)     │
│    └─ UI Banner: "Successfully imported 352 support cards!"            │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Payload Size Benchmark

A common concern with URL-based transfers is browser URL length limits (typically **32,768 characters** in Chrome and Edge).

Using real production data dumped from a live account (`352 support cards` and `43 owned characters`):

| Payload Format | Raw Size | Compressed (Deflate + Base64url) | Fits in Browser URL? |
|---|---|---|---|
| **352 Support Cards** (`{ card_id: limit_break }`) | 3,521 chars | **1,024 chars (~1.0 KB)** | **Yes** (< 4% of URL limit) |
| **352 Cards + 43 Characters** (`{ cards, umas }`) | 4,185 chars | **1,260 chars (~1.2 KB)** | **Yes** (< 4% of URL limit) |

Even with an endgame account with 800+ cards, the entire payload remains well under **2.5 KB**, easily fitting within standard URL limits.

---

## 4. Building & Installing `kyumaru.dll`

### Cross-Compiling on macOS (using `cargo-zigbuild`)

You can compile a native Windows 64-bit DLL directly on macOS without a Windows machine or Visual Studio:

```bash
# 1. Add Windows target (one-time setup)
rustup target add x86_64-pc-windows-gnu

# 2. Build release DLL
cargo zigbuild --manifest-path kyumaru/Cargo.toml --target x86_64-pc-windows-gnu --release
```

The output DLL will be generated at:
```
kyumaru/target/x86_64-pc-windows-gnu/release/kyumaru.dll
```

### Installation into the Game

1. Copy `kyumaru.dll` into your Umamusume game root directory (where `umamusume.exe` lives).
2. Open `hachimi/config.json` in your game folder and add `kyumaru.dll` to `load_libraries`:
   ```json
   {
     "load_libraries": [
       "kyumaru.dll"
     ]
   }
   ```
3. Launch the game. Kyumaru will initialize, create `hachimi/kyumaruConfig.json`, and register its menu section in the Hachimi overlay.

---

## 5. Configuration (`hachimi/kyumaruConfig.json`)

On first launch, Kyumaru generates its configuration file:

```json
{
  "outputPath": "%USERPROFILE%\\Documents\\Kyumaru",
  "targetUrl": "https://database.almond-eye.tech/import/"
}
```

* **`outputPath`**: Base directory where `inventory.json` and `veterans.json` will be saved.
* **`targetUrl`**: The web application URL opened by the browser sync button. Can be changed to `http://localhost:3000/import/` during local frontend development.

---

## 6. Frontend Integration

For full details on implementing the `/import` route and integrating owned card indicators into the AlmondEyeDB deck builder, see:

👉 **[FRONTEND_HANDOFF.md](file:///Users/fubuki/Documents/AlmondEyeDB/kyumaru/FRONTEND_HANDOFF.md)**

---

## 7. Directory Structure & Reference Files

```
Kyumaru/
├── Cargo.toml               # Crate configuration (cdylib, stripped release profile)
├── src/
│   ├── lib.rs               # Plugin entrypoint (hachimi_init & hachimi_init_v3 exports)
│   ├── hooks.rs             # One-shot load/index hook, dormancy, and overlay UI
│   ├── plugin_api.rs        # Hachimi v2/v3 vtable bindings & GUI helpers
│   ├── il2cpp.rs            # IL2CPP runtime bindings & assembly scanning
│   ├── reflection.rs        # Dynamic reflection & ObscuredInt XOR decryption
│   ├── sync.rs              # Deflate + Base64url compression & browser invocation
│   ├── persistence.rs       # Local JSON saving (inventory.json, veterans.json)
│   └── config.rs            # Path handling (%USERPROFILE%\Documents\Kyumaru) & logging
├── README.md                # This architectural & implementation specification
├── FRONTEND_HANDOFF.md      # Web app guide for app/import/page.tsx
├── hachimi_plugin_dev.md    # Hachimi Edge plugin development guide and API reference
```
