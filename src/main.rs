#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::PathBuf,
    process::Command,
};

use directories::ProjectDirs;
use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};

const DEFAULT_CONFIGS_DIR: &str = "configs";
const SERVER_EXE: &str = "ArmaReforgerServer.exe";

// <<< Edit your default args here >>>
const FIXED_ARGS: &[&str] = &[
    "-maxFPS", "120",
    // add more fixed args here if needed
];

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppSettings {
    last_config: Option<PathBuf>,
    server_dir: Option<PathBuf>, // if launcher is not next to the EXE
}

struct LauncherApp {
    settings: AppSettings,
    configs_dir: PathBuf,
    available_configs: Vec<PathBuf>,
    filter: String,
    selected_idx: Option<usize>,
    status: String,
}

impl LauncherApp {
    fn load_settings() -> AppSettings {
        if let Some(pd) = ProjectDirs::from("com", "MAG", "ReforgerLauncher") {
            let p = pd.config_dir().join("settings.json");
            if let Ok(txt) = fs::read_to_string(&p) {
                if let Ok(s) = serde_json::from_str::<AppSettings>(&txt) {
                    return s;
                }
            }
        }
        AppSettings::default()
    }

    fn save_settings(&self) {
        if let Some(pd) = ProjectDirs::from("com", "MAG", "ReforgerLauncher") {
            let _ = fs::create_dir_all(pd.config_dir());
            let p = pd.config_dir().join("settings.json");
            let _ = fs::write(&p, serde_json::to_string_pretty(&self.settings).unwrap());
        }
    }

    fn exe_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    fn detect_server_dir(settings: &AppSettings) -> PathBuf {
        if let Some(dir) = &settings.server_dir {
            return dir.clone();
        }
        // assume the launcher sits next to the server EXE
        Self::exe_dir()
    }

    fn server_exe_path(settings: &AppSettings) -> PathBuf {
        let dir = Self::detect_server_dir(settings);
        dir.join(SERVER_EXE)
    }

    fn new() -> Self {
        let settings = Self::load_settings();
        let exe_dir = Self::exe_dir();
        let configs_dir = exe_dir.join(DEFAULT_CONFIGS_DIR);
        let mut app = Self {
            settings,
            configs_dir,
            available_configs: vec![],
            filter: String::new(),
            selected_idx: None,
            status: String::new(),
        };
        app.refresh_configs();
        // try auto-select last used if present
        if let Some(last) = &app.settings.last_config {
            if let Some(idx) = app
                .available_configs
                .iter()
                .position(|p| p.as_path() == last.as_path())
            {
                app.selected_idx = Some(idx);
            }
        }
        app
    }

    fn refresh_configs(&mut self) {
        self.available_configs.clear();
        let dir = &self.configs_dir;
        if dir.exists() {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().map(|s| s.eq_ignore_ascii_case("json")).unwrap_or(false) {
                        self.available_configs.push(p);
                    }
                }
            }
            self.available_configs.sort();
        }
        if self.available_configs.is_empty() {
            self.status = format!(
                "No .json configs found in {}",
                self.configs_dir.display()
            );
        } else {
            self.status.clear();
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.trim().is_empty() {
            return (0..self.available_configs.len()).collect();
        }
        let f = self.filter.to_lowercase();
        self.available_configs
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_lowercase();
                if name.contains(&f) { Some(i) } else { None }
            })
            .collect()
    }

    fn launch_selected(&mut self) {
        let idx = match self.selected_idx {
            Some(i) => i,
            None => {
                self.status = "Select a config first.".into();
                return;
            }
        };
        if idx >= self.available_configs.len() {
            self.status = "Invalid selection.".into();
            return;
        }
        let config_path = &self.available_configs[idx];

        let server_exe = Self::server_exe_path(&self.settings);
        if !server_exe.exists() {
            self.status = format!(
                "Server exe not found: {}\nSet the correct server directory.",
                server_exe.display()
            );
            return;
        }

        // Build arguments: fixed + -config "<path>"
        let mut args: Vec<String> = FIXED_ARGS.iter().map(|s| s.to_string()).collect();
        args.push("-config".into());
        args.push(config_path.display().to_string());

        // Launch detached so the server lives after closing the launcher.
        #[cfg(windows)]
        {
            use std::process::Stdio;

            let mut full_args: Vec<String> = Vec::new();
            full_args.push(server_exe.display().to_string());
            full_args.extend(args.iter().cloned());

            let mut cmd = Command::new("cmd");
            cmd.current_dir(server_exe.parent().unwrap())
                .arg("/c")
                .arg("start")
                .arg("") // window title
                .args(&full_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());

            match cmd.spawn() {
                Ok(_) => {
                    self.status = format!("Launched:\n{} {}", server_exe.display(), args.join(" "));
                    self.settings.last_config = Some(config_path.clone());
                    self.save_settings();
                }
                Err(e) => self.status = format!("Failed to launch: {e}"),
            }
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Arma Reforger Server Launcher");

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Configs directory:");
                ui.monospace(self.configs_dir.display().to_string());

                if ui.button("Change…").clicked() {
                    if let Some(dir) = FileDialog::new()
                        .set_directory(&self.configs_dir)
                        .pick_folder()
                    {
                        self.configs_dir = dir;
                        self.refresh_configs();
                        self.selected_idx = None;
                    }
                }

                if ui.button("Refresh").clicked() {
                    self.refresh_configs();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(&mut self.filter);
                if ui.button("Clear").clicked() {
                    self.filter.clear();
                }
            });

            ui.add_space(6.0);
            ui.label("Configs:");
            ui.add_space(4.0);

            let filtered = self.filtered_indices();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(260.0)
                .show(ui, |ui| {
                    for (_row, &idx) in filtered.iter().enumerate() {
                        let name = self.available_configs[idx]
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("<invalid>");
                        let selected = self.selected_idx == Some(idx);
                        let resp = ui.selectable_label(selected, name);
                        if resp.clicked() {
                            self.selected_idx = Some(idx);
                        }
                        if resp.double_clicked() {
                            self.selected_idx = Some(idx);
                            self.launch_selected();
                        }
                    }
                });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("Browse config…").clicked() {
                    if let Some(file) = FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_directory(&self.configs_dir)
                        .pick_file()
                    {
                        // add temporarily to list and select it
                        if !self.available_configs.contains(&file) {
                            self.available_configs.push(file.clone());
                        }
                        self.selected_idx = self
                            .available_configs
                            .iter()
                            .position(|p| p == &file);
                    }
                }

                if ui.button("Set server folder…").clicked() {
                    if let Some(dir) = FileDialog::new()
                        .set_directory(Self::detect_server_dir(&self.settings))
                        .pick_folder()
                    {
                        self.settings.server_dir = Some(dir);
                        self.save_settings();
                    }
                }

                if ui.button("Launch").clicked() {
                    self.launch_selected();
                }
            });

            ui.add_space(8.0);

            if !self.status.is_empty() {
                ui.separator();
                ui.label(self.status.clone());
            }

            ui.add_space(6.0);

            // Show full command preview
            if let Some(idx) = self.selected_idx {
                if idx < self.available_configs.len() {
                    let cfg = &self.available_configs[idx];
                    let exe = Self::server_exe_path(&self.settings);
                    let mut preview = format!("{}", exe.display());
                    for a in FIXED_ARGS {
                        preview.push(' ');
                        preview.push_str(a);
                    }
                    preview.push_str(" -config ");
                    preview.push('"');
                    preview.push_str(&cfg.display().to_string());
                    preview.push('"');
                    ui.monospace(preview);
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../logo.png")[..])
                    .expect("Failed to load icon"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "ArmA Reforger - Server Launcher",
        native_options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
}
