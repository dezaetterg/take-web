**English** | [Русский](ARCHITECTURE.ru.md)

# TakeWeb Architecture

This document describes the internal architecture, process isolation model, graphics pipeline, and resource management algorithms of TakeWeb.

## 1. Process Model and Privilege Separation

TakeWeb is built on top of WebKitGTK 6.0's multi-process architecture with strict privilege separation and isolated address spaces.

```
+------------------------------------------------------------------------+
|                      UI Process (takeweb)                             |
|  - GTK4 / Libadwaita main thread                                       |
|  - Window, tab, sidebar, and omnibox orchestration                    |
|  - MemoryManager resource scheduler                                    |
|  - Dedicated asynchronous database worker (AsyncDb)                   |
+------------------------------------------------------------------------+
         |                                           |
         | IPC (Unix Domain Sockets)                 | IPC (Unix Domain Sockets)
         v                                           v
+-----------------------------+       +----------------------------------+
|   WebKitNetworkProcess      |       |      WebKitWebProcess            |
|  - Network stack, TLS       |       |  (Sandboxed via Bubblewrap/bwrap)|
|  - HTTP disk cache (LRU)    |       |  - HTML / CSS layout engine      |
|  - Cookie and HSTS storage  |       |  - JavaScriptCore engine         |
|  - WebKit Content Filter    |       |  - Skia multi-threaded rasterizer|
+-----------------------------+       +----------------------------------+
```

### 1.1. UI Process (`takeweb`)
Acts as the central coordinator of the application:
* Renders all desktop chrome using GTK4 and Libadwaita.
* Routes user inputs, keyboard shortcuts, and events between tabs.
* Manages persistent browser state via an embedded SQLite database. All database write operations are offloaded from the UI thread to a dedicated background `AsyncDb` worker thread via an unbounded `std::sync::mpsc` channel.

### 1.2. Network Process (`WebKitNetworkProcess`)
Handles all protocol-level network operations separately from web document parsing:
* Manages DNS cache, TLS sessions, and HTTP disk cache.
* Enforces a hard limit of 500 MB on the HTTP disk cache with an asynchronous background LRU rotation worker.
* Executes compiled tracker- and ad-blocking bytecode rules (`WebKitUserContentFilterStore`). Network filtering occurs before response payloads reach the rendering pipeline.

### 1.3. Web Content Process (`WebKitWebProcess`)
Executes untrusted web code within a Bubblewrap (`bwrap`) Linux sandbox:
* Filesystem access is strictly restricted. The sandbox only exposes necessary system read paths (system fonts, icons, themes, DRI/OpenGL drivers).
* Direct navigation from remote websites to local `file://` URIs is rejected by the engine.
* Internal browser protocol requests (`take://*`) require a cryptographically secure session token, preventing spoofing of internal UI and settings pages by external scripts.

---

## 2. Memory Management and Resource Lifecycle

TakeWeb implements an aggressive resource scheduling model designed to keep RAM footprint low even during prolonged browsing sessions with dozens of open tabs.

### 2.1. MemoryManager Algorithm
The lifecycle of every tab is tracked through the `TrackedTab` state machine:
* **Active tab:** Visible to the user; prioritized with full compositor rendering.
* **Background tab:** When a tab loses focus, its transition timestamp is recorded with `Instant::now()`.
* **Polling schedule:** Every 5 seconds, the scheduler evaluates the idle duration of background tabs:
  * **Default timeout:** 30 seconds of inactivity.
  * **Emergency timeout:** 10 seconds of inactivity (triggered automatically if system free RAM falls below 15% or if more than 4 background tabs are open).

When a background tab exceeds its timeout threshold, `suspend_tab_internal` is executed:
1. `view.set_visible(false)` disables frame rendering passes in the compositor.
2. WebKit decodes and font caches are purged via `WebKitWebsiteDataManager::clear(WebsiteDataTypes::MEMORY_CACHE)`.
3. Garbage collection is triggered in the JavaScriptCore heap (`window.gc()`).
4. Active audio/video playback elements are paused while preserving their media playback timestamps.

When the user selects the suspended tab again, `resume_tab_internal` immediately wakes the renderer without triggering a network reload.

### 2.2. WebKit Memory Pressure Calibration
During startup, `detect_memory_pressure_plan` inspects `/proc/meminfo` and tunes WebKit's native memory pressure thresholds based on available physical RAM:

| System Physical RAM | WebKit Memory Limit | Conservative Warning | Strict Pressure Threshold | Critical Process Kill |
| :--- | :--- | :--- | :--- | :--- |
| Up to 4 GB | 768 MB | 60% | 75% | 90% |
| 4 GB to 8 GB | 1024 MB | 65% | 80% | 92% |
| Over 8 GB | 1536 MB | 70% | 85% | 95% |

---

## 3. Graphics Pipeline and Hardware Acceleration

Graphics subsystem configuration is finalized prior to GTK initialization in `src/browser/performance.rs`:

```
[takeweb startup]
       |
       v
[GPU & Session Detection] ---> (NVIDIA + Wayland + Display Server)
       |
       +---> Compute environment overrides (GDK_BACKEND, GSK_RENDERER, DMA-BUF)
       |
       +---> Needs process re-exec?
                 |
                 +---[Yes]---> execve() with configured environment
                 |
                 +---[No]----> Initialize GTK4 and WebKit
```

### 3.1. Zero-Copy DMA-BUF Rendering
WebKitGTK defaults to software rendering when proprietary NVIDIA drivers are detected. TakeWeb overrides this policy with `WEBKIT_FORCE_DMABUF_RENDERER=1`, exposing access to `/dev/dri/renderD128` and allowing `WebKitWebProcess` to pass rendered framebuffers directly to the GPU display memory without intermediate CPU copying.

### 3.2. Mutter / Wayland Synchronization Workaround
Under Wayland sessions with proprietary NVIDIA drivers, buffer swapping between WebKitGTK and GNOME's Mutter compositor can hit synchronization lockups (GNOME Issue #3144), capping framerates at 20–30 FPS. TakeWeb resolves this by routing the window through the Xwayland bridge (`GDK_BACKEND=x11`, `GSK_RENDERER=gl`) while disabling client-side vblank stalls (`__GL_SYNC_TO_VBLANK=0`). The Wayland compositor handles final presentation, delivering a smooth 60–144+ FPS.

### 3.3. Multi-Threaded Rasterization
* `WEBKIT_SKIA_CPU_PAINTING_THREADS=4`: Parallel CPU tile rasterization and image decoding across worker cores.
* `WEBKIT_SKIA_GPU_PAINTING_THREADS=2`: Dedicated GPU texture upload threads.
* `WEBKIT_LAYERS_TILE_SIZE=512`: Fixed 512px tile size reducing context switches on 2K and 4K displays.
* `WEBKIT_DISABLE_FRAME_RATE_LIMIT=1`: Unlocks the WebKit frame rate ceiling for high-refresh-rate displays (120, 144, 240 Hz).

---

## 4. Storage Subsystem

User data is stored locally in an embedded SQLite database:
* **Path:** `~/.local/share/takeweb/browser.db`.
* **Journal Mode:** WAL (Write-Ahead Logging), allowing concurrent reader connections without blocking write operations.
* **Synchronization:** `PRAGMA synchronous = NORMAL`, balancing durability against filesystem flush latency.
* **Full-Text Search:** Dedicated `omnibox_fts` virtual table using SQLite FTS5 with automated synchronization triggers for bookmarks and browsing history. Omnibox searches complete in low single-digit milliseconds without keeping history in memory.

---

## 5. Cookie Storage and Site Data Isolation

* **Physical Storage:** Cookies, `localStorage`, `IndexedDB`, and HSTS cache are isolated within `~/.local/share/takeweb/` (in `cookies.db`) via `WebKitWebsiteDataManager`.
* **Zero-Repo Isolation:** User data lives strictly under the standard XDG data directory, never inside the source repository. Database files, caches, and session dumps are excluded in `.gitignore`.
* **Private Browsing (RAM-Only):** Private tabs use an ephemeral context (`WebKitWebsiteDataManager(is_ephemeral = true)`). State resides entirely in memory and is discarded upon tab closure.
* **Tracker Blocking Policy:** The browser enforces `WebKitCookieAcceptPolicy::NoThirdParty` by default, mitigating cross-site tracking (Intelligent Tracking Prevention).

---

## 6. Offline Neural Translation (Bergamot WASM)

* **Zero-Cloud:** Translation runs 100% locally on the user's machine without transmitting text to external services (Google Translate, DeepL, Yandex).
* **WebWorker Architecture:** Page translation and selected text translation execute within an isolated WebWorker running the Mozilla Bergamot WASM neural engine.
* **Model Storage:** Translation model weights are downloaded on demand to `~/.local/share/takeweb/models/bergamot/` and served locally via the registered `take://models/` custom scheme.

---

## 7. Additional Subsystems

* **Reader Mode:** Custom DOM extraction engine removes ads, navigation menus, and trackers, formatting articles into clean typography with light/dark theme support.
* **Split View (`F3` / `Ctrl+\`):** Vector workspace division using `GtkPaned` with independent web processes and tab selection controls.
* **PWA / App Mode (`--app=<URL>`):** Runs web applications in standalone chromeless windows and generates desktop entries in `~/.local/share/applications/`.
* **Localization (i18n):** Embedded `en.json` and `ru.json` translation files with compile-time parity validation via unit tests and automatic system language detection.
* **Built-in TakeWeb Icon Theme:** 63 symbolic SVG icons embedded directly into the binary and automatically unpacked/registered for GTK4, ensuring identical, crisp icons across any desktop environment or window manager (Hyprland, Sway, KDE Plasma, i3, XFCE, GNOME).
* **MPRIS D-Bus Integration:** Standard media controls registered at `org.mpris.MediaPlayer2.takeweb`, integrating with Waybar, KDE media widgets, and `playerctl`.
