//! Native console UI. Network work runs on a background thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, Vec2};
use egui_plot::{Line, Plot, PlotPoints};
use uuid::Uuid;

use crate::api::{Api, ApiError};
use crate::model::*;

const ACCENT: Color32 = Color32::from_rgb(0x15, 0xda, 0xe3);
const OK: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x8c);
const BAD: Color32 = Color32::from_rgb(0xf0, 0x71, 0x78);
const WARN: Color32 = Color32::from_rgb(0xe6, 0xb4, 0x50);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Overview,
    Devices,
    Device,
    Jobs,
    Job,
    Policies,
    Groups,
    Alerts,
    Reports,
    Settings,
    Shell,
}

enum Work {
    Login { user: String, pass: String, totp: String },
    Logout,
    LoadOverview,
    LoadDevices,
    LoadDevice(Uuid),
    LoadJobs,
    LoadJob(Uuid),
    DispatchJob { device_id: Uuid, payload: serde_json::Value },
    CancelJob(Uuid),
    LoadPolicies,
    CreatePolicy { name: String, rules: serde_json::Value },
    TogglePolicy { id: Uuid, enabled: bool },
    DeletePolicy(Uuid),
    AssignPolicy { id: Uuid, device_id: Option<Uuid>, group_id: Option<Uuid> },
    LoadGroups,
    CreateGroup { name: String, description: String },
    DeleteGroup(Uuid),
    AddMember { group: Uuid, device: Uuid },
    RemoveMember { group: Uuid, device: Uuid },
    LoadAlerts { unresolved: bool },
    CreateAlert { name: String, metric: String, operator: String, threshold: f64, severity: String },
    DeleteAlert(Uuid),
    ResolveEvent(Uuid),
    LoadReports,
    LoadSettings,
    EnrollToken,
    MfaSetup,
    MfaVerify(String),
    RequestInventory(Uuid),
    RequestTelemetry(Uuid),
    RevokeDevice(Uuid),
    OpenShell(Uuid),
    ShellInput { session: Uuid, data: String },
    CloseShell(Uuid),
}

enum Reply {
    Error(String),
    LoggedIn(User),
    LoggedOut,
    Overview { status: ServerStatus, devices: Vec<Device>, jobs: Vec<Job>, alerts: Vec<AlertEvent> },
    Devices(Vec<Device>),
    Device { device: Device, metrics: Vec<Metric>, software: Vec<SoftwareItem>, patches: DevicePatches },
    Jobs(Vec<Job>),
    Job(Job),
    Policies { policies: Vec<Policy>, devices: Vec<Device>, groups: Vec<Group> },
    Groups { groups: Vec<Group>, members: Vec<(Uuid, Vec<GroupMember>)>, devices: Vec<Device> },
    Alerts { rules: Vec<AlertRule>, events: Vec<AlertEvent> },
    Reports(ComplianceFleet),
    Settings { settings: Settings, status: ServerStatus },
    Token(EnrollToken),
    MfaUrl(String),
    MfaEnabled,
    ShellOpened(Uuid),
    ShellClosed,
    OkRefresh,
}

pub struct Console {
    api: Api,
    tx: Sender<(Work, Api)>,
    rx: Receiver<Reply>,
    screen: Screen,
    user: Option<User>,
    busy: bool,
    error: Option<String>,
    last_refresh: Instant,
    // login
    api_url: String,
    username: String,
    password: String,
    totp: String,
    // data
    status: Option<ServerStatus>,
    devices: Vec<Device>,
    selected_device: Option<Uuid>,
    metrics: Vec<Metric>,
    software: Vec<SoftwareItem>,
    patches: DevicePatches,
    jobs: Vec<Job>,
    selected_job: Option<Uuid>,
    job_detail: Option<Job>,
    policies: Vec<Policy>,
    groups: Vec<Group>,
    group_members: Vec<(Uuid, Vec<GroupMember>)>,
    alert_rules: Vec<AlertRule>,
    alert_events: Vec<AlertEvent>,
    unresolved_only: bool,
    reports: Option<ComplianceFleet>,
    settings: Option<Settings>,
    enroll_token: Option<EnrollToken>,
    mfa_url: Option<String>,
    mfa_code: String,
    // forms
    job_script: String,
    job_kind: usize,
    policy_name: String,
    policy_fw: bool,
    policy_usb: bool,
    group_name: String,
    alert_name: String,
    alert_metric: usize,
    alert_threshold: f64,
    // shell
    shell_session: Option<Uuid>,
    shell_out: String,
    shell_in: String,
    shell_rx: Option<Receiver<String>>,
}

impl Console {
    pub fn new(api: Api) -> Self {
        let (tx, work_rx) = mpsc::channel::<(Work, Api)>();
        let (reply_tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("osfm-net".into())
            .spawn(move || worker(work_rx, reply_tx))
            .expect("worker thread");
        let api_url = api.base().to_string();
        Self {
            api,
            tx,
            rx,
            screen: Screen::Overview,
            user: None,
            busy: false,
            error: None,
            last_refresh: Instant::now(),
            api_url,
            username: "admin".into(),
            password: String::new(),
            totp: String::new(),
            status: None,
            devices: Vec::new(),
            selected_device: None,
            metrics: Vec::new(),
            software: Vec::new(),
            patches: DevicePatches { pending_count: 0, patches: Vec::new() },
            jobs: Vec::new(),
            selected_job: None,
            job_detail: None,
            policies: Vec::new(),
            groups: Vec::new(),
            group_members: Vec::new(),
            alert_rules: Vec::new(),
            alert_events: Vec::new(),
            unresolved_only: true,
            reports: None,
            settings: None,
            enroll_token: None,
            mfa_url: None,
            mfa_code: String::new(),
            job_script: "uname -a".into(),
            job_kind: 0,
            policy_name: String::new(),
            policy_fw: true,
            policy_usb: false,
            group_name: String::new(),
            alert_name: "High CPU".into(),
            alert_metric: 0,
            alert_threshold: 90.0,
            shell_session: None,
            shell_out: String::new(),
            shell_in: String::new(),
            shell_rx: None,
        }
    }

    fn is_admin(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.role == "admin")
    }

    fn send(&mut self, work: Work) {
        self.busy = true;
        self.error = None;
        let _ = self.tx.send((work, self.api.clone()));
    }

    fn pump(&mut self) {
        while let Ok(reply) = self.rx.try_recv() {
            self.busy = false;
            match reply {
                Reply::Error(e) => self.error = Some(e),
                Reply::LoggedIn(u) => {
                    self.user = Some(u);
                    self.send(Work::LoadOverview);
                }
                Reply::LoggedOut => {
                    self.user = None;
                    self.screen = Screen::Overview;
                }
                Reply::Overview { status, devices, jobs, alerts } => {
                    self.status = Some(status);
                    self.devices = devices;
                    self.jobs = jobs;
                    self.alert_events = alerts;
                }
                Reply::Devices(d) => self.devices = d,
                Reply::Device { device, metrics, software, patches } => {
                    self.selected_device = Some(device.id);
                    self.devices.retain(|x| x.id != device.id);
                    self.devices.insert(0, device);
                    self.metrics = metrics;
                    self.software = software;
                    self.patches = patches;
                }
                Reply::Jobs(j) => self.jobs = j,
                Reply::Job(j) => {
                    self.selected_job = Some(j.id);
                    self.job_detail = Some(j);
                }
                Reply::Policies { policies, devices, groups } => {
                    self.policies = policies;
                    self.devices = devices;
                    self.groups = groups;
                }
                Reply::Groups { groups, members, devices } => {
                    self.groups = groups;
                    self.group_members = members;
                    self.devices = devices;
                }
                Reply::Alerts { rules, events } => {
                    self.alert_rules = rules;
                    self.alert_events = events;
                }
                Reply::Reports(r) => self.reports = Some(r),
                Reply::Settings { settings, status } => {
                    self.settings = Some(settings);
                    self.status = Some(status);
                }
                Reply::Token(t) => self.enroll_token = Some(t),
                Reply::MfaUrl(u) => self.mfa_url = Some(u),
                Reply::MfaEnabled => {
                    self.mfa_url = None;
                    if let Some(u) = &mut self.user {
                        u.totp_enabled = true;
                    }
                }
                Reply::ShellOpened(id) => {
                    self.shell_session = Some(id);
                    self.shell_out.clear();
                    let (stx, srx) = mpsc::channel();
                    self.shell_rx = Some(srx);
                    let api = self.api.clone();
                    std::thread::spawn(move || {
                        api.stream_sse(&format!("/api/v1/shell/{id}/stream"), stx);
                    });
                }
                Reply::ShellClosed => self.shell_session = None,
                Reply::OkRefresh => match self.screen {
                    Screen::Overview => self.send(Work::LoadOverview),
                    Screen::Devices => self.send(Work::LoadDevices),
                    Screen::Device => {
                        if let Some(id) = self.selected_device {
                            self.send(Work::LoadDevice(id));
                        }
                    }
                    Screen::Jobs => self.send(Work::LoadJobs),
                    Screen::Job => {
                        if let Some(id) = self.selected_job {
                            self.send(Work::LoadJob(id));
                        }
                    }
                    Screen::Policies => self.send(Work::LoadPolicies),
                    Screen::Groups => self.send(Work::LoadGroups),
                    Screen::Alerts => self.send(Work::LoadAlerts { unresolved: self.unresolved_only }),
                    Screen::Reports => self.send(Work::LoadReports),
                    Screen::Settings => self.send(Work::LoadSettings),
                    Screen::Shell => {}
                },
            }
        }
        while let Some(rx) = &self.shell_rx {
            match rx.try_recv() {
                Ok(chunk) => self.shell_out.push_str(&chunk),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.shell_rx = None;
                    break;
                }
            }
        }
    }
}

impl eframe::App for Console {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        if self.user.is_some() && !self.busy {
            let due = match self.screen {
                Screen::Overview if self.last_refresh.elapsed().as_secs() >= 15 => Some(Work::LoadOverview),
                Screen::Job if self.last_refresh.elapsed().as_secs() >= 2 => {
                    self.selected_job.map(Work::LoadJob)
                }
                _ => None,
            };
            if let Some(work) = due {
                self.last_refresh = Instant::now();
                self.send(work);
            }
        }

        if self.user.is_none() {
            self.login_ui(ctx);
            return;
        }

        egui::SidePanel::left("nav")
            .exact_width(168.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("OSFM-EDM").strong().color(ACCENT).size(16.0));
                ui.label(RichText::new("native console").small().weak());
                ui.separator();
                self.nav_btn(ui, Screen::Overview, "Overview");
                self.nav_btn(ui, Screen::Devices, "Devices");
                self.nav_btn(ui, Screen::Jobs, "Jobs");
                self.nav_btn(ui, Screen::Policies, "Policies");
                self.nav_btn(ui, Screen::Groups, "Groups");
                self.nav_btn(ui, Screen::Alerts, "Alerts");
                self.nav_btn(ui, Screen::Reports, "Reports");
                self.nav_btn(ui, Screen::Settings, "Settings");
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if ui.button("Sign out").clicked() {
                        self.send(Work::Logout);
                    }
                    if let Some(u) = &self.user {
                        ui.label(RichText::new(&u.role).small().weak());
                        ui.label(&u.username);
                    }
                });
            });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                    ui.label("working…");
                }
                if let Some(err) = &self.error {
                    ui.colored_label(BAD, err);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.api.base()).small().weak());
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.screen {
            Screen::Overview => self.ui_overview(ui),
            Screen::Devices => self.ui_devices(ui),
            Screen::Device => self.ui_device(ui),
            Screen::Jobs => self.ui_jobs(ui),
            Screen::Job => self.ui_job(ui),
            Screen::Policies => self.ui_policies(ui),
            Screen::Groups => self.ui_groups(ui),
            Screen::Alerts => self.ui_alerts(ui),
            Screen::Reports => self.ui_reports(ui),
            Screen::Settings => self.ui_settings(ui),
            Screen::Shell => self.ui_shell(ui),
        });
    }
}

impl Console {
    fn nav_btn(&mut self, ui: &mut egui::Ui, screen: Screen, label: &str) {
        let selected = self.screen == screen
            || (screen == Screen::Devices && self.screen == Screen::Device)
            || (screen == Screen::Jobs && self.screen == Screen::Job);
        if ui.selectable_label(selected, label).clicked() {
            self.screen = screen;
            match screen {
                Screen::Overview => self.send(Work::LoadOverview),
                Screen::Devices => self.send(Work::LoadDevices),
                Screen::Jobs => self.send(Work::LoadJobs),
                Screen::Policies => self.send(Work::LoadPolicies),
                Screen::Groups => self.send(Work::LoadGroups),
                Screen::Alerts => self.send(Work::LoadAlerts { unresolved: self.unresolved_only }),
                Screen::Reports => self.send(Work::LoadReports),
                Screen::Settings => self.send(Work::LoadSettings),
                _ => {}
            }
        }
    }

    fn login_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(RichText::new("OSFM-EDM").size(28.0).color(ACCENT).strong());
                ui.label("Native console — no browser");
                ui.add_space(16.0);
                egui::Grid::new("login").num_columns(2).show(ui, |ui| {
                    ui.label("API");
                    ui.add(egui::TextEdit::singleline(&mut self.api_url).desired_width(280.0));
                    ui.end_row();
                    ui.label("User");
                    ui.add(egui::TextEdit::singleline(&mut self.username).desired_width(280.0));
                    ui.end_row();
                    ui.label("Password");
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(280.0));
                    ui.end_row();
                    ui.label("TOTP");
                    ui.add(egui::TextEdit::singleline(&mut self.totp).desired_width(280.0));
                    ui.end_row();
                });
                ui.add_space(8.0);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button(RichText::new("Sign in").color(Color32::BLACK)).clicked() || enter {
                    // Keep the TLS trust store from startup; only the host string changes.
                    self.send(Work::Login {
                        user: self.username.clone(),
                        pass: self.password.clone(),
                        totp: self.totp.clone(),
                    });
                    self.password.clear();
                }
                if self.busy {
                    ui.spinner();
                }
                if let Some(err) = &self.error {
                    ui.colored_label(BAD, err);
                }
            });
        });
    }

    fn ui_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        if let Some(s) = &self.status {
            ui.horizontal(|ui| {
                stat(ui, "Online", format!("{} / {}", s.online_devices, s.total_devices));
                stat(ui, "Connected", s.connected_agents.to_string());
                stat(ui, "Pending jobs", s.pending_jobs.to_string());
                stat(ui, "Policies", s.total_policies.to_string());
            });
            ui.label(RichText::new(format!("server {}", s.version)).weak());
        }
        ui.separator();
        ui.columns(2, |cols| {
            cols[0].label(RichText::new("Devices").strong());
            let device_hits: Vec<(Uuid, String)> = self
                .devices
                .iter()
                .take(12)
                .map(|d| (d.id, format!("{}   {}", d.hostname, d.status)))
                .collect();
            let mut open_dev = None;
            for (id, label) in &device_hits {
                if cols[0].selectable_label(false, label).clicked() {
                    open_dev = Some(*id);
                }
            }
            cols[1].label(RichText::new("Recent jobs").strong());
            let job_hits: Vec<(Uuid, String)> = self
                .jobs
                .iter()
                .take(12)
                .map(|j| (j.id, format!("{}  {}", short(&j.id), j.status)))
                .collect();
            let mut open_job = None;
            for (id, label) in &job_hits {
                if cols[1].selectable_label(false, label).clicked() {
                    open_job = Some(*id);
                }
            }
            if let Some(id) = open_dev {
                self.screen = Screen::Device;
                self.send(Work::LoadDevice(id));
            }
            if let Some(id) = open_job {
                self.screen = Screen::Job;
                self.send(Work::LoadJob(id));
            }
        });
    }

    fn ui_devices(&mut self, ui: &mut egui::Ui) {
        ui.heading("Devices");
        egui::Grid::new("dev").striped(true).min_col_width(80.0).show(ui, |ui| {
            ui.strong("Host");
            ui.strong("OS");
            ui.strong("Status");
            ui.strong("Agent");
            ui.strong("Last seen");
            ui.end_row();
            let rows: Vec<_> = self.devices.clone();
            for d in rows {
                if ui.link(&d.hostname).clicked() {
                    self.screen = Screen::Device;
                    self.send(Work::LoadDevice(d.id));
                }
                ui.label(format!("{} {}", d.os, d.os_version.clone().unwrap_or_default()));
                status_label(ui, &d.status);
                ui.label(d.agent_version.clone().unwrap_or_else(|| "—".into()));
                ui.label(d.last_seen.clone().unwrap_or_else(|| "—".into()));
                ui.end_row();
            }
        });
    }

    fn ui_device(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected_device else {
            ui.label("No device selected");
            return;
        };
        let host = self
            .devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.hostname.clone())
            .unwrap_or_else(|| short(&id));
        ui.horizontal(|ui| {
            ui.heading(&host);
            if ui.button("← list").clicked() {
                self.screen = Screen::Devices;
            }
        });
        if self.is_admin() {
            ui.horizontal(|ui| {
                if ui.button("Shell").clicked() {
                    self.screen = Screen::Shell;
                    self.send(Work::OpenShell(id));
                }
                if ui.button("Refresh inventory").clicked() {
                    self.send(Work::RequestInventory(id));
                }
                if ui.button("Snapshot").clicked() {
                    self.send(Work::RequestTelemetry(id));
                }
                if ui.button(RichText::new("Revoke").color(BAD)).clicked() {
                    self.send(Work::RevokeDevice(id));
                }
            });
        }
        if !self.metrics.is_empty() {
            let cpu: PlotPoints = self
                .metrics
                .iter()
                .enumerate()
                .map(|(i, m)| [i as f64, m.cpu_pct.unwrap_or(0.0)])
                .collect();
            let ram: PlotPoints = self
                .metrics
                .iter()
                .enumerate()
                .map(|(i, m)| [i as f64, ram_pct(m)])
                .collect();
            Plot::new("tel")
                .height(220.0)
                .allow_scroll(false)
                .show(ui, |p| {
                    p.line(Line::new(cpu).name("CPU %").color(ACCENT));
                    p.line(Line::new(ram).name("RAM %").color(OK));
                });
        } else {
            ui.label("No telemetry in the last 24h.");
        }
        ui.separator();
        ui.collapsing(format!("Software ({})", self.software.len()), |ui| {
            for s in &self.software {
                ui.label(format!("{}  {}", s.name, s.version.clone().unwrap_or_default()));
            }
        });
        ui.collapsing(format!("Patches ({} pending)", self.patches.pending_count), |ui| {
            for p in &self.patches.patches {
                ui.label(format!(
                    "{}  {}  {}",
                    p.title.clone().unwrap_or_else(|| p.patch_id.clone()),
                    p.status,
                    p.severity.clone().unwrap_or_default()
                ));
            }
        });
    }

    fn ui_jobs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Jobs");
        if self.is_admin() {
            ui.group(|ui| {
                ui.label("Dispatch");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("jk")
                        .selected_text(["script", "reboot", "inventory"][self.job_kind])
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.job_kind, 0, "script");
                            ui.selectable_value(&mut self.job_kind, 1, "reboot");
                            ui.selectable_value(&mut self.job_kind, 2, "inventory");
                        });
                    if self.job_kind == 0 {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.job_script)
                                .desired_width(400.0)
                                .desired_rows(3),
                        );
                    }
                    if ui.button("Run on first listed device").clicked() {
                        if let Some(d) = self.devices.first() {
                            let payload = match self.job_kind {
                                1 => serde_json::json!({"type":"reboot","delay_seconds":60}),
                                2 => serde_json::json!({"type":"collect_inventory"}),
                                _ => serde_json::json!({"type":"run_script","shell":"bash","script":self.job_script}),
                            };
                            self.send(Work::DispatchJob { device_id: d.id, payload });
                        }
                    }
                });
                if self.devices.is_empty() {
                    ui.label("Load devices first (open Devices).");
                } else {
                    ui.label(format!("target: {}", self.devices[0].hostname));
                    ui.horizontal(|ui| {
                        for d in self.devices.clone() {
                            if ui.small_button(&d.hostname).clicked() {
                                self.devices.retain(|x| x.id != d.id);
                                self.devices.insert(0, d);
                            }
                        }
                    });
                }
            });
        }
        for j in self.jobs.clone() {
            ui.horizontal(|ui| {
                if ui.link(short(&j.id)).clicked() {
                    self.screen = Screen::Job;
                    self.send(Work::LoadJob(j.id));
                }
                status_label(ui, &j.status);
                ui.label(short(&j.device_id));
                ui.label(j.created_at.clone().unwrap_or_default());
            });
        }
    }

    fn ui_job(&mut self, ui: &mut egui::Ui) {
        let Some(j) = self.job_detail.clone() else {
            ui.label("No job");
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(short(&j.id));
            if ui.button("← list").clicked() {
                self.screen = Screen::Jobs;
                self.send(Work::LoadJobs);
            }
            if self.is_admin() && !matches!(j.status.as_str(), "completed" | "done" | "failed" | "cancelled") {
                if ui.button("Cancel").clicked() {
                    self.send(Work::CancelJob(j.id));
                }
            }
        });
        status_label(ui, &j.status);
        ui.label(format!("exit {:?}", j.exit_code));
        ui.collapsing("payload", |ui| {
            ui.monospace(serde_json::to_string_pretty(&j.payload).unwrap_or_default());
        });
        ui.separator();
        ui.label(RichText::new("Logs").strong());
        egui::ScrollArea::vertical().max_height(360.0).stick_to_bottom(true).show(ui, |ui| {
            for line in &j.logs {
                let color = if line.stream == "stderr" { BAD } else { Color32::LIGHT_GRAY };
                ui.colored_label(color, &line.line);
            }
        });
        if ui.button("Refresh").clicked() {
            self.send(Work::LoadJob(j.id));
        }
    }

    fn ui_policies(&mut self, ui: &mut egui::Ui) {
        ui.heading("Policies");
        if self.is_admin() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.policy_name);
                ui.checkbox(&mut self.policy_fw, "firewall");
                ui.checkbox(&mut self.policy_usb, "block USB");
                if ui.button("Create").clicked() && !self.policy_name.is_empty() {
                    let mut rules = vec![serde_json::json!({"type":"firewall","enabled":self.policy_fw})];
                    rules.push(serde_json::json!({"type":"usb_storage","allow":!self.policy_usb}));
                    self.send(Work::CreatePolicy {
                        name: self.policy_name.clone(),
                        rules: serde_json::Value::Array(rules),
                    });
                }
            });
        }
        for p in self.policies.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&p.name);
                    ui.label(if p.enabled { "enabled" } else { "disabled" });
                    ui.label(format!("v{}", p.version));
                    if self.is_admin() {
                        if ui.button(if p.enabled { "Disable" } else { "Enable" }).clicked() {
                            self.send(Work::TogglePolicy { id: p.id, enabled: !p.enabled });
                        }
                        if ui.button("Delete").clicked() {
                            self.send(Work::DeletePolicy(p.id));
                        }
                    }
                });
                ui.monospace(p.rules.to_string());
                if self.is_admin() {
                    let assign: Vec<(Uuid, String)> =
                        self.devices.iter().map(|d| (d.id, d.hostname.clone())).collect();
                    ui.horizontal(|ui| {
                        for (did, host) in &assign {
                            if ui.small_button(format!("→ {host}")).clicked() {
                                self.send(Work::AssignPolicy {
                                    id: p.id,
                                    device_id: Some(*did),
                                    group_id: None,
                                });
                            }
                        }
                    });
                }
            });
        }
    }

    fn ui_groups(&mut self, ui: &mut egui::Ui) {
        ui.heading("Groups");
        if self.is_admin() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.group_name);
                if ui.button("Create").clicked() && !self.group_name.is_empty() {
                    self.send(Work::CreateGroup {
                        name: self.group_name.clone(),
                        description: String::new(),
                    });
                }
            });
        }
        for g in self.groups.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&g.name);
                    if self.is_admin() && ui.button("Delete").clicked() {
                        self.send(Work::DeleteGroup(g.id));
                    }
                });
                let members: Vec<(Uuid, String, String)> = self
                    .group_members
                    .iter()
                    .find(|(id, _)| *id == g.id)
                    .map(|(_, m)| {
                        m.iter()
                            .map(|x| (x.device_id, x.hostname.clone(), x.status.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                for (mid, host, st) in &members {
                    ui.horizontal(|ui| {
                        ui.label(format!("{host} ({st})"));
                        if self.is_admin() && ui.small_button("remove").clicked() {
                            self.send(Work::RemoveMember { group: g.id, device: *mid });
                        }
                    });
                }
                if self.is_admin() {
                    let add: Vec<(Uuid, String)> =
                        self.devices.iter().map(|d| (d.id, d.hostname.clone())).collect();
                    ui.horizontal(|ui| {
                        for (did, host) in &add {
                            if ui.small_button(format!("+ {host}")).clicked() {
                                self.send(Work::AddMember { group: g.id, device: *did });
                            }
                        }
                    });
                }
            });
        }
    }

    fn ui_alerts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Alerts");
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.unresolved_only, "unresolved only").changed() {
                self.send(Work::LoadAlerts { unresolved: self.unresolved_only });
            }
        });
        if self.is_admin() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.alert_name);
                egui::ComboBox::from_id_salt("am")
                    .selected_text(["cpu_pct", "ram_pct", "disk_pct"][self.alert_metric])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.alert_metric, 0, "cpu_pct");
                        ui.selectable_value(&mut self.alert_metric, 1, "ram_pct");
                        ui.selectable_value(&mut self.alert_metric, 2, "disk_pct");
                    });
                ui.add(egui::DragValue::new(&mut self.alert_threshold).range(0.0..=100.0));
                if ui.button("Add rule").clicked() {
                    self.send(Work::CreateAlert {
                        name: self.alert_name.clone(),
                        metric: ["cpu_pct", "ram_pct", "disk_pct"][self.alert_metric].into(),
                        operator: ">".into(),
                        threshold: self.alert_threshold,
                        severity: "warning".into(),
                    });
                }
            });
        }
        ui.label(RichText::new("Rules").strong());
        for r in self.alert_rules.clone() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{}  {} {} {:?}  {}",
                    r.name,
                    r.metric.clone().unwrap_or_default(),
                    r.operator.clone().unwrap_or_default(),
                    r.threshold,
                    if r.enabled { "on" } else { "off" }
                ));
                if self.is_admin() && ui.small_button("delete").clicked() {
                    self.send(Work::DeleteAlert(r.id));
                }
            });
        }
        ui.separator();
        ui.label(RichText::new("Events").strong());
        for e in self.alert_events.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(
                    if e.severity.as_deref() == Some("critical") { BAD } else { WARN },
                    e.message.clone().unwrap_or_default(),
                );
                if self.is_admin() && e.resolved_at.is_none() && ui.small_button("resolve").clicked() {
                    self.send(Work::ResolveEvent(e.id));
                }
            });
        }
    }

    fn ui_reports(&mut self, ui: &mut egui::Ui) {
        ui.heading("Compliance");
        if let Some(r) = &self.reports {
            ui.horizontal(|ui| {
                stat(ui, "Rate", format!("{:.1}%", r.compliance_rate));
                stat(ui, "Ok", r.compliant.to_string());
                stat(ui, "Violations", r.non_compliant.to_string());
            });
            for v in &r.recent_violations {
                ui.label(format!(
                    "{}  policy {}  {}  {}",
                    short(&v.device_id),
                    short(&v.policy_id),
                    if v.compliant { "ok" } else { "violation" },
                    v.reported_at.clone().unwrap_or_default()
                ));
            }
        }
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        if let Some(s) = &self.settings {
            ui.label(format!("Public URL: {}", s.server_url));
            ui.label(format!("API port: {}", s.server_port));
            ui.label(format!("TLS flag: {}", s.tls_configured));
            ui.label(format!("CA: {}", if s.ca_initialized { "ready" } else { "missing" }));
        }
        if let Some(st) = &self.status {
            ui.label(format!("Version {}", st.version));
        }
        ui.separator();
        ui.label("Enrollment — one-time token, 24h. On the device:");
        ui.monospace("osfm-edm-agent --server http://<api-host>:8080 --token <token>");
        if self.is_admin() && ui.button("Generate token").clicked() {
            self.send(Work::EnrollToken);
        }
        if let Some(t) = &self.enroll_token {
            ui.label(format!("expires {}", t.expires_at));
            ui.text_edit_singleline(&mut t.token.clone());
        }
        ui.separator();
        ui.label("TOTP");
        if self.user.as_ref().is_some_and(|u| u.totp_enabled) {
            ui.label("enabled");
        }
        if ui.button("Start TOTP setup").clicked() {
            self.send(Work::MfaSetup);
        }
        if let Some(url) = self.mfa_url.clone() {
            ui.monospace(url);
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.mfa_code);
                if ui.button("Enable").clicked() {
                    self.send(Work::MfaVerify(self.mfa_code.clone()));
                }
            });
        }
    }

    fn ui_shell(&mut self, ui: &mut egui::Ui) {
        ui.heading("Remote shell");
        ui.label(RichText::new("Piped /bin/sh — not a PTY").weak());
        let Some(dev) = self.selected_device else {
            ui.label("Open a device first");
            return;
        };
        ui.horizontal(|ui| {
            if self.shell_session.is_none() && ui.button("Open").clicked() {
                self.send(Work::OpenShell(dev));
            }
            if let Some(sid) = self.shell_session {
                if ui.button("Close").clicked() {
                    self.send(Work::CloseShell(sid));
                }
            }
        });
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.monospace(&self.shell_out);
            });
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.shell_in)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            let send = ui.button("Send").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if send {
                if let Some(sid) = self.shell_session {
                    let mut data = self.shell_in.clone();
                    if !data.ends_with('\n') {
                        data.push('\n');
                    }
                    self.send(Work::ShellInput { session: sid, data });
                    self.shell_in.clear();
                }
            }
        });
    }
}

fn worker(rx: Receiver<(Work, Api)>, tx: Sender<Reply>) {
    while let Ok((work, api)) = rx.recv() {
        let reply = match run(&api, work) {
            Ok(r) => r,
            Err(e) => Reply::Error(e.to_string()),
        };
        if tx.send(reply).is_err() {
            break;
        }
    }
}

fn run(api: &Api, work: Work) -> Result<Reply, ApiError> {
    match work {
        Work::Login { user, pass, totp } => {
            api.login(&user, &pass, &totp)?;
            Ok(Reply::LoggedIn(api.get("/api/v1/auth/me")?))
        }
        Work::Logout => {
            let _ = api.logout();
            Ok(Reply::LoggedOut)
        }
        Work::LoadOverview => {
            let status = api.get("/api/v1/settings/status")?;
            let devices = api.get("/api/v1/devices")?;
            let jobs: Vec<Job> = api.get("/api/v1/jobs")?;
            let alerts = api.get("/api/v1/alerts/events?unresolved=true&limit=8")?;
            Ok(Reply::Overview { status, devices, jobs, alerts })
        }
        Work::LoadDevices => Ok(Reply::Devices(api.get("/api/v1/devices")?)),
        Work::LoadDevice(id) => {
            let device = api.get(&format!("/api/v1/devices/{id}"))?;
            let metrics = api.get(&format!("/api/v1/devices/{id}/telemetry"))?;
            let software = api.get(&format!("/api/v1/software/device/{id}"))?;
            let patches = api.get(&format!("/api/v1/patches/device/{id}"))?;
            Ok(Reply::Device { device, metrics, software, patches })
        }
        Work::LoadJobs => Ok(Reply::Jobs(api.get("/api/v1/jobs")?)),
        Work::LoadJob(id) => Ok(Reply::Job(api.get(&format!("/api/v1/jobs/{id}"))?)),
        Work::DispatchJob { device_id, payload } => {
            api.post_empty("/api/v1/jobs", Some(&serde_json::json!({ "device_id": device_id, "payload": payload })))?;
            Ok(Reply::Jobs(api.get("/api/v1/jobs")?))
        }
        Work::CancelJob(id) => {
            api.post_empty(&format!("/api/v1/jobs/{id}/cancel"), None::<&()>)?;
            Ok(Reply::Job(api.get(&format!("/api/v1/jobs/{id}"))?))
        }
        Work::LoadPolicies => Ok(Reply::Policies {
            policies: api.get("/api/v1/policies")?,
            devices: api.get("/api/v1/devices")?,
            groups: api.get("/api/v1/groups")?,
        }),
        Work::CreatePolicy { name, rules } => {
            api.post_empty("/api/v1/policies", Some(&serde_json::json!({ "name": name, "rules": rules })))?;
            run(api, Work::LoadPolicies)
        }
        Work::TogglePolicy { id, enabled } => {
            let _: Policy = api.patch(&format!("/api/v1/policies/{id}"), &serde_json::json!({ "enabled": enabled }))?;
            run(api, Work::LoadPolicies)
        }
        Work::DeletePolicy(id) => {
            api.delete(&format!("/api/v1/policies/{id}"))?;
            run(api, Work::LoadPolicies)
        }
        Work::AssignPolicy { id, device_id, group_id } => {
            api.post_empty(
                &format!("/api/v1/policies/{id}/assign"),
                Some(&serde_json::json!({ "device_id": device_id, "group_id": group_id })),
            )?;
            Ok(Reply::OkRefresh)
        }
        Work::LoadGroups => {
            let groups: Vec<Group> = api.get("/api/v1/groups")?;
            let devices = api.get("/api/v1/devices")?;
            let mut members = Vec::new();
            for g in &groups {
                let m: Vec<GroupMember> = api.get(&format!("/api/v1/groups/{}/members", g.id))?;
                members.push((g.id, m));
            }
            Ok(Reply::Groups { groups, members, devices })
        }
        Work::CreateGroup { name, description } => {
            api.post_empty("/api/v1/groups", Some(&serde_json::json!({ "name": name, "description": description })))?;
            run(api, Work::LoadGroups)
        }
        Work::DeleteGroup(id) => {
            api.delete(&format!("/api/v1/groups/{id}"))?;
            run(api, Work::LoadGroups)
        }
        Work::AddMember { group, device } => {
            api.post_empty(
                &format!("/api/v1/groups/{group}/members"),
                Some(&serde_json::json!({ "device_id": device })),
            )?;
            run(api, Work::LoadGroups)
        }
        Work::RemoveMember { group, device } => {
            api.delete(&format!("/api/v1/groups/{group}/members/{device}"))?;
            run(api, Work::LoadGroups)
        }
        Work::LoadAlerts { unresolved } => {
            let q = if unresolved { "?unresolved=true" } else { "" };
            Ok(Reply::Alerts {
                rules: api.get("/api/v1/alerts/rules")?,
                events: api.get(&format!("/api/v1/alerts/events{q}"))?,
            })
        }
        Work::CreateAlert { name, metric, operator, threshold, severity } => {
            api.post_empty(
                "/api/v1/alerts/rules",
                Some(&serde_json::json!({ "name": name, "metric": metric, "operator": operator, "threshold": threshold, "severity": severity })),
            )?;
            run(api, Work::LoadAlerts { unresolved: true })
        }
        Work::DeleteAlert(id) => {
            api.delete(&format!("/api/v1/alerts/rules/{id}"))?;
            run(api, Work::LoadAlerts { unresolved: true })
        }
        Work::ResolveEvent(id) => {
            api.post_empty(&format!("/api/v1/alerts/events/{id}/resolve"), None::<&()>)?;
            run(api, Work::LoadAlerts { unresolved: true })
        }
        Work::LoadReports => Ok(Reply::Reports(api.get("/api/v1/reports/compliance")?)),
        Work::LoadSettings => Ok(Reply::Settings {
            settings: api.get("/api/v1/settings")?,
            status: api.get("/api/v1/settings/status")?,
        }),
        Work::EnrollToken => Ok(Reply::Token(api.post_no_body("/api/v1/enroll/token")?)),
        Work::MfaSetup => {
            let v: serde_json::Value = api.post_no_body("/api/v1/auth/mfa/setup")?;
            let url = v
                .get("otpauth_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Reply::MfaUrl(url))
        }
        Work::MfaVerify(code) => {
            api.post_empty("/api/v1/auth/mfa/verify", Some(&serde_json::json!({ "code": code })))?;
            Ok(Reply::MfaEnabled)
        }
        Work::RequestInventory(id) => {
            api.post_empty(&format!("/api/v1/devices/{id}/request-inventory"), None::<&()>)?;
            run(api, Work::LoadDevice(id))
        }
        Work::RequestTelemetry(id) => {
            api.post_empty(&format!("/api/v1/devices/{id}/request-telemetry"), None::<&()>)?;
            run(api, Work::LoadDevice(id))
        }
        Work::RevokeDevice(id) => {
            api.delete(&format!("/api/v1/devices/{id}"))?;
            Ok(Reply::Devices(api.get("/api/v1/devices")?))
        }
        Work::OpenShell(device) => {
            let opened: ShellOpen = api.post_no_body(&format!("/api/v1/shell/{device}"))?;
            Ok(Reply::ShellOpened(opened.session_id))
        }
        Work::ShellInput { session, data } => {
            api.post_empty(&format!("/api/v1/shell/{session}/input"), Some(&serde_json::json!({ "data": data })))?;
            Ok(Reply::OkRefresh)
        }
        Work::CloseShell(session) => {
            api.delete(&format!("/api/v1/shell/{session}/close"))?;
            Ok(Reply::ShellClosed)
        }
    }
}

fn stat(ui: &mut egui::Ui, label: &str, value: String) {
    ui.group(|ui| {
        ui.set_min_size(Vec2::new(120.0, 48.0));
        ui.label(RichText::new(label).small().weak());
        ui.label(RichText::new(value).size(18.0).strong());
    });
}

fn status_label(ui: &mut egui::Ui, status: &str) {
    let c = match status {
        "online" => OK,
        "stale" => WARN,
        _ => Color32::GRAY,
    };
    ui.colored_label(c, status);
}

fn short(id: &Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn ram_pct(m: &Metric) -> f64 {
    match (m.ram_used_mb, m.ram_total_mb) {
        (Some(u), Some(t)) if t > 0 => (u as f64 / t as f64) * 100.0,
        _ => 0.0,
    }
}
