use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{create_dir_all, read_to_string, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    sync::OnceLock,
};

static SAVE_ROOT: OnceLock<PathBuf> = OnceLock::new();
static TARGET_URL: OnceLock<String> = OnceLock::new();
static LOG_MUTEX: Mutex<()> = Mutex::new(());

const DEFAULT_TARGET_URL: &str = "https://database.almond-eye.tech/import/";

#[derive(Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "outputPath")]
    pub output_path: Option<String>,
    #[serde(rename = "targetUrl", default = "default_target_url")]
    pub target_url: String,
}

fn default_target_url() -> String {
    DEFAULT_TARGET_URL.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            output_path: Some("%USERPROFILE%\\Documents\\Kyumaru".to_string()),
            target_url: default_target_url(),
        }
    }
}

pub fn save_root() -> &'static PathBuf {
    SAVE_ROOT.get().expect("save root not initialized")
}

pub fn target_url() -> &'static str {
    TARGET_URL.get().map(|s| s.as_str()).unwrap_or(DEFAULT_TARGET_URL)
}

pub fn init_config() -> Result<(), String> {
    let plugin_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let cfg_dir = plugin_dir.join("hachimi");
    let _ = create_dir_all(&cfg_dir);

    let cfg_path = cfg_dir.join("kyumaruConfig.json");

    let cfg: Config = if cfg_path.exists() {
        read_to_string(&cfg_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Config::default()
    };

    // Re-save config to ensure defaults are populated
    if let Ok(mut f) = File::create(&cfg_path) {
        let _ = writeln!(
            f,
            "{}",
            serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".into())
        );
    }

    let resolved_root = match cfg.output_path.as_deref() {
        Some(p) if !p.trim().is_empty() => {
            let path_str = p.trim();
            let expanded_path = if let Ok(home) = env::var("USERPROFILE") {
                path_str.replace("%USERPROFILE%", &home)
            } else if let Ok(home) = env::var("HOME") {
                path_str.replace("$USER", &home).replace("~", &home)
            } else {
                path_str.to_string()
            };
            let path = Path::new(&expanded_path);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                plugin_dir.join(path)
            }
        }
        _ => {
            let docs = env::var("USERPROFILE")
                .or_else(|_| env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| plugin_dir.clone());
            docs.join("Documents").join("Kyumaru")
        }
    };

    let _ = create_dir_all(&resolved_root);
    let _ = SAVE_ROOT.set(resolved_root);
    let _ = TARGET_URL.set(cfg.target_url);

    Ok(())
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::config::debug_log_internal(&format!($($arg)*))
    };
}

pub fn debug_log_internal(msg: &str) {
    let _guard = LOG_MUTEX.lock().ok();

    let plugin_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let log_dir = plugin_dir.join("hachimi");
    let _ = create_dir_all(&log_dir);
    let log_path = log_dir.join("kyumaru.log");

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let now = chrono::Local::now();
        let _ = writeln!(file, "{} {}", now.format("%Y-%m-%d %H:%M:%S%.3f"), msg);
    }
}
