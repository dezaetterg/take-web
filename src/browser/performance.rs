//! Graphics and environment tuning that must happen *before* GTK and WebKit start.
//!
//! On Linux systems with NVIDIA proprietary drivers under Wayland compositors (GNOME/Mutter),
//! WebKitGTK Wayland surface buffer exchanges trigger sync fence stalls capping frame rates at
//! ~20 FPS. Routing GTK through X11 (or Xwayland) with the GL renderer completely bypasses these
//! fence stalls and delivers solid 60+ FPS scrolling.

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuVendor {
    Intel,
    Amd,
    Nvidia,
}

// ============================================================================
// Layer 1: Pure decision functions (no I/O, no env reads, fully testable)
// ============================================================================

pub(crate) fn painting_threads_for(cpu_count: usize) -> u32 {
    cpu_count.saturating_sub(1).clamp(1, 4) as u32
}

pub(crate) fn should_disable_dmabuf(has_render_node: bool, is_vm: bool) -> bool {
    !has_render_node || is_vm
}

pub(crate) fn vaapi_enabled(has_render_node: bool, is_vm: bool, vendor: Option<GpuVendor>) -> bool {
    has_render_node && !is_vm && matches!(vendor, Some(GpuVendor::Intel) | Some(GpuVendor::Amd))
}

pub(crate) fn low_memory(ratio: f64) -> bool {
    ratio < 0.15
}

pub(crate) fn should_suspend_background(ratio: f64) -> bool {
    low_memory(ratio)
}

pub(crate) fn parse_meminfo_ratio(meminfo: &str) -> Option<f64> {
    let mut mem_total = None;
    let mut mem_available = None;

    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            if let Some(val_str) = rest.split_whitespace().next() {
                if let Ok(val) = val_str.parse::<u64>() {
                    mem_total = Some(val);
                }
            }
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            if let Some(val_str) = rest.split_whitespace().next() {
                if let Ok(val) = val_str.parse::<u64>() {
                    mem_available = Some(val);
                }
            }
        }
    }

    match (mem_total, mem_available) {
        (Some(total), Some(available)) if total > 0 => Some(available as f64 / total as f64),
        _ => None,
    }
}

pub(crate) fn parse_meminfo_total_mb(meminfo: &str) -> Option<u64> {
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            if let Some(val_str) = rest.split_whitespace().next() {
                if let Ok(val) = val_str.parse::<u64>() {
                    return Some(val / 1024);
                }
            }
        }
    }
    None
}

/// Dynamic WebKit memory pressure plan based on physical RAM and memory pressure:
/// Returns (limit_mb, conservative_threshold, strict_threshold, kill_threshold).
pub(crate) fn calculate_memory_pressure_plan(
    total_ram_mb: u64,
    is_low_mem: bool,
) -> (u32, f64, f64, f64) {
    if total_ram_mb <= 4096 || is_low_mem {
        // Systems with <= 4GB RAM or low available RAM: cap at 768 MB, trim aggressively
        (768, 0.60, 0.75, 0.90)
    } else if total_ram_mb <= 8192 {
        // Systems with <= 8GB RAM: cap at 1024 MB
        (1024, 0.65, 0.80, 0.92)
    } else {
        // High RAM systems (> 8GB): cap at 1536 MB
        (1536, 0.70, 0.85, 0.95)
    }
}

pub fn detect_memory_pressure_plan() -> (u32, f64, f64, f64) {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total_mb = parse_meminfo_total_mb(&meminfo).unwrap_or(8192);
    let ratio = parse_meminfo_ratio(&meminfo).unwrap_or(0.5);
    let is_low = low_memory(ratio);
    calculate_memory_pressure_plan(total_mb, is_low)
}

pub(crate) fn vendor_from_id(id: &str) -> Option<GpuVendor> {
    let trimmed = id.trim().to_lowercase();
    match trimmed.as_str() {
        "0x8086" => Some(GpuVendor::Intel),
        "0x1002" => Some(GpuVendor::Amd),
        "0x10de" => Some(GpuVendor::Nvidia),
        _ => None,
    }
}

pub(crate) fn vaapi_vendor_from_ids<I, S>(vendor_ids: I) -> Option<GpuVendor>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for id in vendor_ids {
        if let Some(v) = vendor_from_id(id.as_ref()) {
            if matches!(v, GpuVendor::Intel | GpuVendor::Amd) {
                return Some(v);
            }
        }
    }
    None
}

pub(crate) fn is_vm_string(haystack: &str) -> bool {
    let lower = haystack.to_lowercase();
    const VM_KEYWORDS: &[&str] = &[
        "virtualbox",
        "vmware",
        "qemu",
        "kvm",
        "innotek",
        "parallels",
        "hypervisor",
    ];
    VM_KEYWORDS.iter().any(|&keyword| lower.contains(keyword))
}

// ============================================================================
// Layer 2: System inspection & apply_tuning()
// ============================================================================

fn detect_is_vm() -> bool {
    // Primary path: /proc/cpuinfo containing hypervisor flag
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        if is_vm_string(&cpuinfo) {
            return true;
        }
    }

    // Secondary path: DMI product_name and sys_vendor
    let dmi_product = std::fs::read_to_string("/sys/class/dmi/id/product_name").unwrap_or_default();
    if is_vm_string(&dmi_product) {
        return true;
    }

    let dmi_vendor = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    if is_vm_string(&dmi_vendor) {
        return true;
    }

    false
}

fn detect_has_render_node() -> bool {
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("renderD"))
    } else {
        false
    }
}

fn detect_gpu_vendor() -> Option<GpuVendor> {
    if let Ok(v) = env::var("ECHO_GPU_VENDOR") {
        return vendor_from_id(&v);
    }

    if is_nvidia() {
        return Some(GpuVendor::Nvidia);
    }

    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        let mut fallback = None;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("card") && !name_str.contains('-') {
                let dev_path = entry.path().join("device");
                if let Ok(vendor_str) = std::fs::read_to_string(dev_path.join("vendor")) {
                    if let Some(vendor) = vendor_from_id(&vendor_str) {
                        let boot_vga = std::fs::read_to_string(dev_path.join("boot_vga"))
                            .map(|s| s.trim() == "1")
                            .unwrap_or(false);
                        if boot_vga {
                            return Some(vendor);
                        }
                        if fallback.is_none() {
                            fallback = Some(vendor);
                        }
                    }
                }
            }
        }
        if fallback.is_some() {
            return fallback;
        }
    }

    None
}

/// Specifically searches DRM devices for Intel (0x8086) or AMD (0x1002) GPUs
/// capable of hardware VA-API video decoding, independent of whether NVIDIA
/// is present (e.g. hybrid laptops with Intel/AMD iGPU + NVIDIA dGPU).
fn detect_vaapi_vendor() -> Option<GpuVendor> {
    if let Ok(v) = env::var("ECHO_VAAPI_VENDOR") {
        return vendor_from_id(&v);
    }

    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("card") && !name_str.contains('-') {
                let dev_path = entry.path().join("device");
                if let Ok(vendor_str) = std::fs::read_to_string(dev_path.join("vendor")) {
                    if let Some(vendor) = vendor_from_id(&vendor_str) {
                        if matches!(vendor, GpuVendor::Intel | GpuVendor::Amd) {
                            return Some(vendor);
                        }
                    }
                }
            }
        }
    }

    None
}

fn detect_meminfo_ratio() -> Option<f64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo_ratio(&meminfo)
}

/// Discovers hardware capabilities and applies system tuning before GTK/WebKit initialization.
///
/// Returns whether the system is currently under low available memory (<15%).
pub fn apply_tuning() -> bool {
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let threads = painting_threads_for(cpu_count);

    let is_vm = detect_is_vm();
    let has_render_node = detect_has_render_node();
    let vendor = detect_gpu_vendor();
    // Hybrid graphics: check for Intel/AMD iGPU for VA-API decoding independently of NVIDIA dGPU
    let vaapi_vendor = detect_vaapi_vendor().or(vendor);

    let force_software = env::var("LIBGL_ALWAYS_SOFTWARE")
        .map(|v| v == "1")
        .unwrap_or(false)
        || env::var("GALLIUM_DRIVER")
            .map(|v| v == "llvmpipe")
            .unwrap_or(false);

    let dmabuf_disabled = force_software || should_disable_dmabuf(has_render_node, is_vm);
    let vaapi = vaapi_enabled(has_render_node, is_vm, vaapi_vendor);
    let low_mem = detect_meminfo_ratio().map(low_memory).unwrap_or(false);

    // Logging: must be emitted once at startup
    log::info!(
        "performance: cpu={cpu_count} threads={threads} vendor={vendor:?} vaapi_vendor={vaapi_vendor:?} dmabuf_disabled={dmabuf_disabled} vaapi={vaapi} low_mem={low_mem}"
    );

    // Apply environment variables only if not explicitly set by the user
    if env::var_os("WEBKIT_SKIA_CPU_PAINTING_THREADS").is_none() {
        unsafe {
            env::set_var("WEBKIT_SKIA_CPU_PAINTING_THREADS", threads.to_string());
        }
    }

    if dmabuf_disabled && env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        unsafe {
            env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    if vaapi {
        let current_rank = env::var("GST_PLUGIN_FEATURE_RANK").unwrap_or_default();
        let vaapi_ranks = "vah264dec:MAX,vaapih264dec:MAX,vavp9dec:MAX,vaapivp9dec:MAX,vaav1dec:MAX,vaapidecodebin:MAX";
        let new_rank = if current_rank.is_empty() {
            vaapi_ranks.to_string()
        } else if !current_rank.contains("vaapih264dec") {
            format!("{vaapi_ranks},{current_rank}")
        } else {
            current_rank
        };
        unsafe {
            env::set_var("GST_PLUGIN_FEATURE_RANK", new_rank);
        }
    }

    low_mem
}

pub fn is_nvidia() -> bool {
    if let Ok(v) = env::var("ECHO_NVIDIA") {
        return v == "1" || v.eq_ignore_ascii_case("true");
    }

    if !std::path::Path::new("/proc/driver/nvidia/version").exists() {
        return false;
    }

    // If explicit prime offloading is requested, NVIDIA is the active renderer.
    if env::var("__NV_PRIME_RENDER_OFFLOAD")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return true;
    }

    // Check DRM sysfs to detect hybrid Optimus laptops where Intel/AMD iGPU is the primary boot display.
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        let mut non_nvidia_boot_vga = false;
        let mut nvidia_found = false;

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.starts_with("card") && !name.contains('-') {
                let dev_path = entry.path().join("device");
                let vendor = std::fs::read_to_string(dev_path.join("vendor"))
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();
                let boot_vga = std::fs::read_to_string(dev_path.join("boot_vga"))
                    .unwrap_or_default()
                    .trim()
                    == "1";

                if vendor == "0x10de" {
                    nvidia_found = true;
                    if boot_vga {
                        return true;
                    }
                } else if boot_vga {
                    non_nvidia_boot_vga = true;
                }
            }
        }

        // On hybrid Optimus systems where Intel/AMD drives the display and NVIDIA offload isn't set:
        if non_nvidia_boot_vga && nvidia_found {
            return false;
        }
    }

    true
}

pub fn is_wayland_session() -> bool {
    env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false)
        || env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Computes the desired graphics configuration for the current hardware and session.
pub fn calculate_desired_graphics_config() -> Vec<(String, String)> {
    if env::var("ECHO_GFX_TUNING")
        .map(|v| v == "0")
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let is_nvidia = is_nvidia();
    let has_x = env::var_os("DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let egl_is_wayland =
        env::var_os("EGL_PLATFORM").as_deref() == Some(std::ffi::OsStr::new("wayland"));

    calculate_desired_graphics_config_internal(is_nvidia, has_x, egl_is_wayland)
}

pub fn calculate_desired_graphics_config_internal(
    is_nvidia: bool,
    has_x: bool,
    egl_is_wayland: bool,
) -> Vec<(String, String)> {
    let mut config = Vec::new();

    // 1. GDK Backend:
    // User override takes highest priority.
    // If running on NVIDIA with an X server available (X11 or Xwayland), route through x11
    // to bypass Mutter/WebKitGTK fence stalls (GNOME #3144 / WebKit #261877).
    if let Ok(backend) = env::var("ECHO_GDK_BACKEND") {
        config.push(("GDK_BACKEND".into(), backend));
    } else if is_nvidia && has_x {
        config.push(("GDK_BACKEND".into(), "x11".into()));
    }

    // 2. GSK Renderer:
    // In modern GTK4, 'gl' is the stable default; 'ngl' is deprecated and 'vulkan'
    // encounters stalls with WebKitGTK DMA-BUF on NVIDIA drivers.
    if let Ok(renderer) = env::var("ECHO_GSK_RENDERER") {
        config.push(("GSK_RENDERER".into(), renderer));
    } else {
        config.push(("GSK_RENDERER".into(), "gl".into()));
    }

    // 3. WebKit DMA-BUF:
    // Only disable if explicitly requested via ECHO_WEBKIT_DMABUF=off or if software rendering is forced.
    // On NVIDIA with X11, WebKitGTK falls back to software rendering (Policy: never) unless
    // WEBKIT_FORCE_DMABUF_RENDERER=1 is set, which unlocks hardware DMABUF on /dev/dri/renderD128.
    let force_software = env::var("LIBGL_ALWAYS_SOFTWARE")
        .map(|v| v == "1")
        .unwrap_or(false)
        || env::var("GALLIUM_DRIVER")
            .map(|v| v == "llvmpipe")
            .unwrap_or(false);

    if env::var("ECHO_WEBKIT_DMABUF")
        .map(|v| v == "off")
        .unwrap_or(false)
        || force_software
        || env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some()
    {
        config.push(("WEBKIT_DISABLE_DMABUF_RENDERER".into(), "1".into()));
    } else if is_nvidia && has_x && env::var_os("WEBKIT_FORCE_DMABUF_RENDERER").is_none() {
        config.push(("WEBKIT_FORCE_DMABUF_RENDERER".into(), "1".into()));
    }

    // 4. NVIDIA VSync, EGL alignment & Layer Tiling on X11 / Xwayland:
    // Under Xwayland, Mutter manages final display presentation timing. Setting
    // __GL_SYNC_TO_VBLANK=0 prevents the client GLX swap buffers call from blocking on
    // vblank, eliminating double-vsync pipeline stalls and frame quantization on 240Hz displays.
    // Setting WEBKIT_LAYERS_TILE_SIZE=512 reduces tile count by 4x on 4K+ viewports.
    if is_nvidia && has_x {
        if egl_is_wayland {
            config.push(("EGL_PLATFORM".into(), "x11".into()));
        }
        if env::var_os("__GL_SYNC_TO_VBLANK").is_none() {
            config.push(("__GL_SYNC_TO_VBLANK".into(), "0".into()));
        }
        if env::var_os("WEBKIT_LAYERS_TILE_SIZE").is_none() {
            config.push(("WEBKIT_LAYERS_TILE_SIZE".into(), "512".into()));
        }
    }

    // 5. Skia Multithreaded Painting and Layer Compositing:
    // Enables parallel tile rasterization, CPU image decoding worker threads,
    // and dedicated GPU painting threads to avoid main WebProcess event loop stalls.
    if env::var_os("WEBKIT_SKIA_CPU_PAINTING_THREADS").is_none() {
        config.push(("WEBKIT_SKIA_CPU_PAINTING_THREADS".into(), "4".into()));
    }
    if env::var_os("WEBKIT_SKIA_GPU_PAINTING_THREADS").is_none() {
        config.push(("WEBKIT_SKIA_GPU_PAINTING_THREADS".into(), "2".into()));
    }
    if env::var_os("WEBKIT_FORCE_COMPOSITING_MODE").is_none() {
        config.push(("WEBKIT_FORCE_COMPOSITING_MODE".into(), "1".into()));
    }
    // High-refresh-rate displays (120Hz, 144Hz, 240Hz): uncap WebKit compositor rate
    if env::var_os("WEBKIT_DISABLE_FRAME_RATE_LIMIT").is_none() {
        config.push(("WEBKIT_DISABLE_FRAME_RATE_LIMIT".into(), "1".into()));
    }

    // 6. GStreamer Media Sink & Codec Ranking on NVIDIA:
    // WebKitGTK's zero-copy GL/DMABuf video sinks fail on NVIDIA drivers under X11/Xwayland.
    if is_nvidia {
        if env::var_os("WEBKIT_GST_DISABLE_GL_SINK").is_none() {
            config.push(("WEBKIT_GST_DISABLE_GL_SINK".into(), "1".into()));
        }
        if env::var_os("GST_PLUGIN_FEATURE_RANK").is_none() {
            config.push((
                "GST_PLUGIN_FEATURE_RANK".into(),
                "nvh264dec:NONE,nvh265dec:NONE,nvav1dec:NONE,nvvp8dec:NONE,nvvp9dec:NONE".into(),
            ));
        }
    }

    config
}

/// Returns the subset of desired variables that are not already active in the current process.
/// If any variable differs, `main()` must re-exec so GTK reads them at initialization.
pub fn calculate_graphics_config() -> Vec<(String, String)> {
    let desired = calculate_desired_graphics_config();
    let mut needed_changes = Vec::new();

    for (key, target) in desired {
        let current = env::var(&key).ok();
        if current.as_deref() != Some(&target) {
            needed_changes.push((key, target));
        }
    }

    needed_changes
}

/// Must be called from `main()` before any GTK/WebKit call.
pub fn configure_graphics_environment() {
    if env::var("ECHO_GFX_TUNING")
        .map(|v| v == "0")
        .unwrap_or(false)
    {
        log::info!("perf: graphics tuning disabled via ECHO_GFX_TUNING=0");
        return;
    }

    let desired = calculate_desired_graphics_config();
    for (key, value) in desired {
        log::info!("perf: active {key}={value}");
        unsafe {
            env::set_var(key, value);
        }
    }

    if is_nvidia() && env::var_os("__GL_SHADER_DISK_CACHE").is_none() {
        log::info!("perf: __GL_SHADER_DISK_CACHE=1");
        unsafe {
            env::set_var("__GL_SHADER_DISK_CACHE", "1");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_painting_threads_for() {
        assert_eq!(painting_threads_for(1), 1);
        assert_eq!(painting_threads_for(2), 1);
        assert_eq!(painting_threads_for(3), 2);
        assert_eq!(painting_threads_for(4), 3);
        assert_eq!(painting_threads_for(8), 4);
        assert_eq!(painting_threads_for(16), 4);
    }

    #[test]
    fn test_should_disable_dmabuf() {
        // All 4 combinations of (has_render_node, is_vm):
        assert!(!should_disable_dmabuf(true, false));
        assert!(should_disable_dmabuf(true, true));
        assert!(should_disable_dmabuf(false, false));
        assert!(should_disable_dmabuf(false, true));
    }

    #[test]
    fn test_vaapi_enabled() {
        // Intel/AMD/Nvidia/None x VM/non-VM:
        // 1. Intel
        assert!(vaapi_enabled(true, false, Some(GpuVendor::Intel)));
        assert!(!vaapi_enabled(true, true, Some(GpuVendor::Intel)));

        // 2. AMD
        assert!(vaapi_enabled(true, false, Some(GpuVendor::Amd)));
        assert!(!vaapi_enabled(true, true, Some(GpuVendor::Amd)));

        // 3. Nvidia
        assert!(!vaapi_enabled(true, false, Some(GpuVendor::Nvidia)));
        assert!(!vaapi_enabled(true, true, Some(GpuVendor::Nvidia)));

        // 4. None
        assert!(!vaapi_enabled(true, false, None));
        assert!(!vaapi_enabled(true, true, None));

        // 5. Without render node
        assert!(!vaapi_enabled(false, false, Some(GpuVendor::Intel)));
        assert!(!vaapi_enabled(false, false, Some(GpuVendor::Amd)));
    }

    #[test]
    fn test_vendor_from_id() {
        assert_eq!(vendor_from_id("0x8086\n"), Some(GpuVendor::Intel));
        assert_eq!(vendor_from_id("0x1002\n"), Some(GpuVendor::Amd));
        assert_eq!(vendor_from_id("0x10de\n"), Some(GpuVendor::Nvidia));
        assert_eq!(vendor_from_id("0x9999\n"), None);
        assert_eq!(vendor_from_id(""), None);
    }

    #[test]
    fn test_is_vm_string() {
        assert!(is_vm_string("Oracle VirtualBox Guest"));
        assert!(is_vm_string("VMware Virtual Platform"));
        assert!(is_vm_string("QEMU Standard PC (Q35 + ICH9, 2009)"));
        assert!(is_vm_string("Common KVM Processor"));
        assert!(is_vm_string("innotek GmbH"));
        assert!(is_vm_string("Parallels Virtual Platform"));
        assert!(is_vm_string("flags: fpu vme de pse hypervisor"));
        assert!(!is_vm_string("ASUSTeK COMPUTER INC."));
    }

    #[test]
    fn test_parse_meminfo_ratio() {
        let synthetic_ok =
            "MemTotal:        8000000 kB\nMemFree:          500000 kB\nMemAvailable:    1000000 kB\n";
        let ratio = parse_meminfo_ratio(synthetic_ok).expect("valid ratio");
        assert!((ratio - 0.125).abs() < f64::EPSILON);

        let synthetic_missing_available =
            "MemTotal:        8000000 kB\nMemFree:          500000 kB\n";
        assert_eq!(parse_meminfo_ratio(synthetic_missing_available), None);

        let synthetic_zero_total = "MemTotal:              0 kB\nMemAvailable:    1000000 kB\n";
        assert_eq!(parse_meminfo_ratio(synthetic_zero_total), None);
    }

    #[test]
    fn test_low_memory_and_suspend_background() {
        assert!(low_memory(0.149));
        assert!(!low_memory(0.150));
        assert!(!low_memory(0.151));

        assert!(should_suspend_background(0.149));
        assert!(!should_suspend_background(0.150));
        assert!(!should_suspend_background(0.151));
    }

    #[test]
    fn test_nvidia_with_x11_requests_x11_and_gl() {
        std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
        let cfg = calculate_desired_graphics_config_internal(true, true, true);
        assert_eq!(
            cfg,
            vec![
                ("GDK_BACKEND".to_string(), "x11".to_string()),
                ("GSK_RENDERER".to_string(), "gl".to_string()),
                ("WEBKIT_FORCE_DMABUF_RENDERER".to_string(), "1".to_string()),
                ("EGL_PLATFORM".to_string(), "x11".to_string()),
                ("__GL_SYNC_TO_VBLANK".to_string(), "0".to_string()),
                ("WEBKIT_LAYERS_TILE_SIZE".to_string(), "512".to_string()),
                (
                    "WEBKIT_SKIA_CPU_PAINTING_THREADS".to_string(),
                    "4".to_string()
                ),
                (
                    "WEBKIT_SKIA_GPU_PAINTING_THREADS".to_string(),
                    "2".to_string()
                ),
                ("WEBKIT_FORCE_COMPOSITING_MODE".to_string(), "1".to_string()),
                (
                    "WEBKIT_DISABLE_FRAME_RATE_LIMIT".to_string(),
                    "1".to_string()
                ),
                ("WEBKIT_GST_DISABLE_GL_SINK".to_string(), "1".to_string()),
                (
                    "GST_PLUGIN_FEATURE_RANK".to_string(),
                    "nvh264dec:NONE,nvh265dec:NONE,nvav1dec:NONE,nvvp8dec:NONE,nvvp9dec:NONE"
                        .to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_non_nvidia_preserves_native_backend() {
        std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
        let cfg = calculate_desired_graphics_config_internal(false, true, false);
        assert_eq!(
            cfg,
            vec![
                ("GSK_RENDERER".to_string(), "gl".to_string()),
                (
                    "WEBKIT_SKIA_CPU_PAINTING_THREADS".to_string(),
                    "4".to_string()
                ),
                (
                    "WEBKIT_SKIA_GPU_PAINTING_THREADS".to_string(),
                    "2".to_string()
                ),
                ("WEBKIT_FORCE_COMPOSITING_MODE".to_string(), "1".to_string()),
                (
                    "WEBKIT_DISABLE_FRAME_RATE_LIMIT".to_string(),
                    "1".to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_nvidia_without_x_preserves_wayland() {
        std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
        let cfg = calculate_desired_graphics_config_internal(true, false, false);
        assert_eq!(
            cfg,
            vec![
                ("GSK_RENDERER".to_string(), "gl".to_string()),
                (
                    "WEBKIT_SKIA_CPU_PAINTING_THREADS".to_string(),
                    "4".to_string()
                ),
                (
                    "WEBKIT_SKIA_GPU_PAINTING_THREADS".to_string(),
                    "2".to_string()
                ),
                ("WEBKIT_FORCE_COMPOSITING_MODE".to_string(), "1".to_string()),
                (
                    "WEBKIT_DISABLE_FRAME_RATE_LIMIT".to_string(),
                    "1".to_string()
                ),
                ("WEBKIT_GST_DISABLE_GL_SINK".to_string(), "1".to_string()),
                (
                    "GST_PLUGIN_FEATURE_RANK".to_string(),
                    "nvh264dec:NONE,nvh265dec:NONE,nvav1dec:NONE,nvvp8dec:NONE,nvvp9dec:NONE"
                        .to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_is_nvidia_override() {
        env::set_var("ECHO_NVIDIA", "0");
        assert!(!is_nvidia());
        env::set_var("ECHO_NVIDIA", "1");
        assert!(is_nvidia());
        env::remove_var("ECHO_NVIDIA");
    }

    #[test]
    fn test_vaapi_vendor_from_ids_hybrid() {
        // Hybrid Intel + NVIDIA: finds Intel regardless of order
        assert_eq!(
            vaapi_vendor_from_ids(["0x10de", "0x8086"]),
            Some(GpuVendor::Intel)
        );
        assert_eq!(
            vaapi_vendor_from_ids(["0x8086", "0x10de"]),
            Some(GpuVendor::Intel)
        );

        // Hybrid AMD + NVIDIA: finds AMD regardless of order
        assert_eq!(
            vaapi_vendor_from_ids(["0x10de", "0x1002"]),
            Some(GpuVendor::Amd)
        );
        assert_eq!(
            vaapi_vendor_from_ids(["0x1002", "0x10de"]),
            Some(GpuVendor::Amd)
        );

        // Standalone NVIDIA: no VA-API vendor found
        assert_eq!(vaapi_vendor_from_ids(["0x10de"]), None);

        // Empty or unknown
        assert_eq!(vaapi_vendor_from_ids(["0x1234"]), None);
        assert_eq!(vaapi_vendor_from_ids([] as [&str; 0]), None);
    }

    #[test]
    fn test_hybrid_graphics_vaapi_enabled() {
        let hybrid_intel = vaapi_vendor_from_ids(["0x10de", "0x8086"]);
        assert!(vaapi_enabled(true, false, hybrid_intel));

        let hybrid_amd = vaapi_vendor_from_ids(["0x10de", "0x1002"]);
        assert!(vaapi_enabled(true, false, hybrid_amd));

        let pure_nvidia = vaapi_vendor_from_ids(["0x10de"]);
        assert!(!vaapi_enabled(true, false, pure_nvidia));

        // When render node is missing or running in VM, VA-API is disabled even on hybrid
        assert!(!vaapi_enabled(false, false, hybrid_intel));
        assert!(!vaapi_enabled(true, true, hybrid_intel));
    }

    #[test]
    fn test_parse_meminfo_total_mb() {
        let meminfo = "MemTotal:       16384000 kB\nMemFree:         1000000 kB\n";
        assert_eq!(parse_meminfo_total_mb(meminfo), Some(16000));
        assert_eq!(parse_meminfo_total_mb("NoMemHere: 123 kB"), None);
    }

    #[test]
    fn test_calculate_memory_pressure_plan() {
        // <= 4GB RAM or low memory
        assert_eq!(
            calculate_memory_pressure_plan(4096, false),
            (768, 0.60, 0.75, 0.90)
        );
        assert_eq!(
            calculate_memory_pressure_plan(16384, true),
            (768, 0.60, 0.75, 0.90)
        );

        // <= 8GB RAM
        assert_eq!(
            calculate_memory_pressure_plan(8192, false),
            (1024, 0.65, 0.80, 0.92)
        );

        // > 8GB RAM
        assert_eq!(
            calculate_memory_pressure_plan(16384, false),
            (1536, 0.70, 0.85, 0.95)
        );
    }
}
