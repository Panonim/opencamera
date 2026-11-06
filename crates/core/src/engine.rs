use crate::pairing::{Device, DeviceStore, Pairing};
use crate::pipeline::{self, Counters, VirtualCameraState};
use crate::process::ProcessingSettings;
use crate::protocol::{DesktopMessage, IceServer, PhoneSettingsPatch, PhoneState, PhoneStats};
use crate::{net, server, Result};
use serde::Serialize;
use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RECONNECT_WINDOW: Duration = Duration::from_secs(30);
const LOG_LIMIT: usize = 300;
const DEVELOPER_LOG_LIMIT: usize = 3000;
const DEVELOPER_STATS_EVERY: u64 = 10;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Idle,
    Pairing,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Disconnected,
}

/// A problem shown to the user, which the window turns into an explanation and a fix.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserError {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub url: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub running: bool,
    pub address: Option<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub phase: Phase,
    pub error: Option<UserError>,
    pub device: Option<Device>,
    pub phone: Option<PhoneState>,
    pub pairing: Option<PairingInfo>,
    pub server: ServerInfo,
    pub virtual_camera: VirtualCameraState,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub fps: f64,
    pub bitrate_kbps: f64,
    pub packet_loss: f64,
    pub dropped_frames: u64,
    pub rtt_ms: Option<f64>,
    pub width: u32,
    pub height: u32,
    /// Connection quality: good, fair, poor or none.
    pub quality: &'static str,
    pub phone: Option<PhoneStats>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub addresses: Vec<String>,
    pub server: ServerInfo,
    pub virtual_camera: VirtualCameraState,
    pub virtual_camera_details: String,
    pub paired_devices: usize,
    pub logs: Vec<String>,
}

pub trait EngineEvents: Send + Sync + 'static {
    fn state(&self, state: &EngineState);
    fn stats(&self, stats: &Stats);
    fn preview(&self, jpeg: Vec<u8>);
    fn devices(&self, devices: &[Device]);
    /// Receives each new log line while developer mode is on.
    fn log(&self, _line: &str) {}
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub port: u16,
    /// Network address to use, or the usual one when left empty.
    pub address: Option<Ipv4Addr>,
    pub ice_servers: Vec<IceServer>,
}

pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub network: NetworkConfig,
    pub processing: ProcessingSettings,
    /// Starts listening right away so known phones can reconnect without scanning again.
    pub accept_reconnects: bool,
    /// Records detailed debug lines and streams every log line to the app.
    pub developer: bool,
}

#[derive(Default)]
pub(crate) struct NetCounters {
    pub bytes: AtomicU64,
    pub packets: AtomicU64,
    pub lost: AtomicU64,
}

pub(crate) struct SessionHandle {
    pub id: u64,
    pub device_id: String,
    pub tx: mpsc::UnboundedSender<DesktopMessage>,
    pub close: Arc<Notify>,
}

impl SessionHandle {
    pub fn end(&self, reason: &str) {
        let _ = self.tx.send(DesktopMessage::Bye { reason: reason.into() });
        self.close.notify_one();
    }
}

pub(crate) struct Inner {
    pub data_dir: PathBuf,
    pub network: Mutex<NetworkConfig>,
    pub state: Mutex<EngineState>,
    pub pairing: Mutex<Pairing>,
    pub devices: Mutex<DeviceStore>,
    pub events: Arc<dyn EngineEvents>,
    pub pipeline: Mutex<std::sync::mpsc::Sender<pipeline::Msg>>,
    pub counters: Arc<Counters>,
    pub net: NetCounters,
    pub session: Mutex<Option<SessionHandle>>,
    pub session_ids: AtomicU64,
    pub keyframe: Notify,
    pub server: Mutex<Option<axum_server::Handle<std::net::SocketAddr>>>,
    pub phone_stats: Mutex<Option<PhoneStats>>,
    pub logs: Mutex<VecDeque<String>>,
    pub developer: AtomicBool,
    pub desktop_name: String,
}

impl Inner {
    pub fn update(&self, f: impl FnOnce(&mut EngineState)) {
        let snapshot = {
            let mut s = self.state.lock().unwrap();
            let before = s.clone();
            f(&mut s);
            if *s == before {
                return;
            }
            s.clone()
        };
        self.events.state(&snapshot);
    }

    pub fn phase(&self) -> Phase {
        self.state.lock().unwrap().phase
    }

    pub fn developer(&self) -> bool {
        self.developer.load(Ordering::Relaxed)
    }

    /// Adds a line to the troubleshooting log, which must never include codes or passwords.
    pub fn log(&self, msg: impl Into<String>) {
        self.write("INFO ", msg.into());
    }

    /// Adds a detailed line that is only recorded in developer mode, same privacy rule as `log`.
    pub fn debug(&self, msg: impl FnOnce() -> String) {
        if self.developer() {
            self.write("DEBUG", msg());
        }
    }

    fn write(&self, level: &str, msg: String) {
        tracing::info!("{msg}");
        let ms = crate::pairing::now_ms();
        let secs = ms / 1000;
        let line = format!("{:02}:{:02}:{:02}.{:03}Z {level} {msg}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60, ms % 1000);
        let developer = self.developer();
        {
            let mut logs = self.logs.lock().unwrap();
            let limit = if developer { DEVELOPER_LOG_LIMIT } else { LOG_LIMIT };
            while logs.len() >= limit {
                logs.pop_front();
            }
            logs.push_back(line.clone());
        }
        if developer {
            self.events.log(&line);
        }
    }

    pub fn send_pipeline(&self, msg: pipeline::Msg) {
        let _ = self.pipeline.lock().unwrap().send(msg);
    }

    pub fn emit_devices(&self) {
        let list = self.devices.lock().unwrap().list();
        self.events.devices(&list);
    }

    /// Waits a while for a dropped phone to come back before giving up.
    pub fn start_reconnect_window(self: &Arc<Self>) {
        self.update(|s| s.phase = Phase::Reconnecting);
        self.debug(|| format!("waiting {}s for phone to reconnect", RECONNECT_WINDOW.as_secs()));
        let me = self.clone();
        let generation = self.session_ids.load(Ordering::SeqCst);
        tokio::spawn(async move {
            tokio::time::sleep(RECONNECT_WINDOW).await;
            if me.session_ids.load(Ordering::SeqCst) == generation && me.phase() == Phase::Reconnecting {
                me.log("phone did not reconnect in time");
                me.update(|s| {
                    s.phase = Phase::Disconnected;
                    s.device = None;
                    s.phone = None;
                    s.error = Some(UserError { code: "lost_connection", detail: String::new() });
                });
            }
        });
    }
}

struct PipelineEvents(std::sync::Weak<Inner>);

impl pipeline::Events for PipelineEvents {
    fn preview(&self, jpeg: Vec<u8>) {
        if let Some(inner) = self.0.upgrade() {
            inner.events.preview(jpeg);
        }
    }
    fn request_keyframe(&self) {
        if let Some(inner) = self.0.upgrade() {
            inner.keyframe.notify_one();
        }
    }
    fn debug(&self, msg: &dyn Fn() -> String) {
        if let Some(inner) = self.0.upgrade() {
            inner.debug(msg);
        }
    }
    fn virtual_camera(&self, state: VirtualCameraState) {
        if let Some(inner) = self.0.upgrade() {
            let detail = state.message.as_deref().map(|m| format!(" - {m}")).unwrap_or_default();
            inner.log(format!("virtual camera: {}{detail}", state.status));
            if state.status != "active" {
                inner.log(crate::vcam::diagnostics());
            }
            inner.update(|s| s.virtual_camera = state);
        }
    }
}

pub struct Engine {
    inner: Arc<Inner>,
    runtime: tokio::runtime::Runtime,
}

impl Engine {
    pub fn start(config: EngineConfig, events: Arc<dyn EngineEvents>) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        std::fs::create_dir_all(&config.data_dir)?;
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let counters = Arc::new(Counters::default());
        let devices = DeviceStore::load(config.data_dir.join("devices.json"));
        let listen = config.accept_reconnects && !devices.is_empty();
        let (placeholder_tx, _) = std::sync::mpsc::channel();

        let inner = Arc::new(Inner {
            data_dir: config.data_dir,
            network: Mutex::new(config.network),
            state: Mutex::new(EngineState {
                phase: Phase::Idle,
                error: None,
                device: None,
                phone: None,
                pairing: None,
                server: ServerInfo::default(),
                virtual_camera: VirtualCameraState { status: "starting", message: None },
            }),
            pairing: Mutex::new(Pairing::default()),
            devices: Mutex::new(devices),
            events,
            pipeline: Mutex::new(placeholder_tx),
            counters: counters.clone(),
            net: NetCounters::default(),
            session: Mutex::new(None),
            session_ids: AtomicU64::new(0),
            keyframe: Notify::new(),
            server: Mutex::new(None),
            phone_stats: Mutex::new(None),
            logs: Mutex::new(VecDeque::new()),
            developer: AtomicBool::new(config.developer),
            desktop_name: hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_else(|_| "Computer".into()),
        });
        let tx = pipeline::spawn(config.processing, counters, Arc::new(PipelineEvents(Arc::downgrade(&inner))));
        *inner.pipeline.lock().unwrap() = tx;
        inner.log(format!("OpenCamera {VERSION} started"));
        inner.debug(|| format!("{} {}, data in {}", std::env::consts::OS, std::env::consts::ARCH, inner.data_dir.display()));

        let engine = Self { inner, runtime };
        engine.runtime.spawn(stats_loop(engine.inner.clone()));
        engine.runtime.spawn(prune_loop(engine.inner.clone()));
        if listen {
            if let Err(e) = engine.ensure_server() {
                engine.inner.log(format!("could not start server: {e}"));
            }
        }
        Ok(engine)
    }

    pub fn state(&self) -> EngineState {
        self.inner.state.lock().unwrap().clone()
    }

    fn ensure_server(&self) -> Result<()> {
        if self.inner.server.lock().unwrap().is_some() {
            return Ok(());
        }
        let net_cfg = self.inner.network.lock().unwrap().clone();
        let address = net_cfg.address.or_else(net::primary_address).ok_or("no_network")?;
        let mut sans = net::lan_addresses();
        if !sans.contains(&address) {
            sans.push(address);
        }
        self.inner.debug(|| format!("starting server on {address}:{}, certificate covers {sans:?}", net_cfg.port));
        let tls = net::load_or_create_cert(&self.inner.data_dir.join("tls"), &sans)?;
        let handle = server::start(self.inner.clone(), &self.runtime, net_cfg.port, tls)?;
        *self.inner.server.lock().unwrap() = Some(handle);
        self.inner.log(format!("listening on https://{address}:{}", net_cfg.port));
        self.inner.update(|s| s.server = ServerInfo { running: true, address: Some(address.to_string()), port: net_cfg.port });
        Ok(())
    }

    fn stop_server(&self) {
        if let Some(h) = self.inner.server.lock().unwrap().take() {
            h.shutdown();
        }
        self.inner.update(|s| s.server.running = false);
    }

    /// Makes a fresh one-time pairing link, cancels the old one, and starts listening.
    pub fn start_pairing(&self) -> Result<PairingInfo> {
        if let Err(e) = self.ensure_server() {
            let code = match e.to_string().as_str() {
                "no_network" => "no_network",
                s if s.contains("in use") || s.contains("AddrInUse") || s.contains("10048") => "port_in_use",
                _ => "server_failed",
            };
            self.inner.update(|s| s.error = Some(UserError { code, detail: e.to_string() }));
            return Err(e);
        }
        let (token, expires_at) = self.inner.pairing.lock().unwrap().issue();
        let server = self.inner.state.lock().unwrap().server.clone();
        let info = PairingInfo {
            url: format!("https://{}:{}/#t={token}", server.address.unwrap_or_default(), server.port),
            expires_at,
        };
        self.inner.log("pairing code created");
        self.inner.update(|s| {
            s.pairing = Some(info.clone());
            s.error = None;
            if matches!(s.phase, Phase::Idle | Phase::Disconnected | Phase::Failed) {
                s.phase = Phase::Pairing;
            }
        });
        Ok(info)
    }

    pub fn cancel_pairing(&self) {
        self.inner.pairing.lock().unwrap().cancel();
        self.inner.debug(|| "pairing cancelled".into());
        self.inner.update(|s| {
            s.pairing = None;
            if s.phase == Phase::Pairing {
                s.phase = Phase::Idle;
            }
        });
    }

    pub fn disconnect(&self) {
        if let Some(sess) = self.inner.session.lock().unwrap().take() {
            sess.end("desktop_disconnect");
        }
        self.inner.log("disconnected by user");
        self.inner.update(|s| {
            s.phase = Phase::Disconnected;
            s.device = None;
            s.phone = None;
            s.error = None;
        });
    }

    pub fn clear_error(&self) {
        self.inner.update(|s| {
            s.error = None;
            if matches!(s.phase, Phase::Failed | Phase::Disconnected) {
                s.phase = Phase::Idle;
            }
        });
    }

    pub fn set_phone_settings(&self, patch: PhoneSettingsPatch) {
        self.inner.debug(|| format!("phone settings sent: {patch:?}"));
        if let Some(sess) = self.inner.session.lock().unwrap().as_ref() {
            let _ = sess.tx.send(DesktopMessage::Control { settings: patch });
        }
    }

    pub fn set_processing(&self, settings: ProcessingSettings) {
        self.inner.debug(|| format!("processing changed: {settings:?}"));
        self.inner.send_pipeline(pipeline::Msg::Settings(settings));
    }

    /// Installs the virtual camera, waiting while the system asks for permission, then reopens it.
    pub fn setup_virtual_camera(&self) -> std::result::Result<(), String> {
        let result = crate::vcam::request_setup().map_err(|e| e.to_string());
        if let Err(e) = &result {
            self.inner.log(format!("virtual camera setup failed: {e}"));
        }
        self.inner.send_pipeline(pipeline::Msg::OpenVirtualCamera);
        result
    }

    /// Closes the virtual camera and then uninstalls it from the system.
    pub fn remove_virtual_camera(&self) -> std::result::Result<(), String> {
        self.inner.send_pipeline(pipeline::Msg::CloseVirtualCamera);
        std::thread::sleep(Duration::from_millis(300));
        let result = crate::vcam::request_removal().map_err(|e| e.to_string());
        self.inner.send_pipeline(pipeline::Msg::OpenVirtualCamera);
        result
    }

    /// Turns detailed logging and live log streaming on or off.
    pub fn set_developer(&self, on: bool) {
        self.inner.developer.store(on, Ordering::Relaxed);
        self.inner.log(format!("developer mode {}", if on { "on" } else { "off" }));
    }

    pub fn retry_virtual_camera(&self) {
        self.inner.send_pipeline(pipeline::Msg::OpenVirtualCamera);
    }

    pub fn devices(&self) -> Vec<Device> {
        self.inner.devices.lock().unwrap().list()
    }

    pub fn rename_device(&self, id: &str, name: &str) {
        if self.inner.devices.lock().unwrap().rename(id, name) {
            self.inner.update(|s| {
                if let Some(d) = s.device.as_mut().filter(|d| d.id == id) {
                    d.name = crate::pairing::clean_name(name);
                }
            });
            self.inner.emit_devices();
        }
    }

    pub fn forget_device(&self, id: &str) {
        let connected = self.inner.session.lock().unwrap().as_ref().is_some_and(|s| s.device_id == id);
        if connected {
            self.disconnect();
        }
        if self.inner.devices.lock().unwrap().forget(id) {
            self.inner.log("device forgotten");
            self.inner.emit_devices();
        }
    }

    /// Saves new network settings and restarts the server if it was running.
    pub fn set_network(&self, network: NetworkConfig) {
        self.inner.debug(|| format!("network changed: port {}, address {:?}, {} ICE server(s)", network.port, network.address, network.ice_servers.len()));
        *self.inner.network.lock().unwrap() = network;
        if self.inner.server.lock().unwrap().is_some() {
            self.stop_server();
            if let Err(e) = self.ensure_server() {
                self.inner.log(format!("could not restart server: {e}"));
            }
        }
    }

    pub fn diagnostics(&self) -> Diagnostics {
        let state = self.state();
        Diagnostics {
            version: VERSION,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            addresses: net::lan_addresses().iter().map(|a| a.to_string()).collect(),
            server: state.server,
            virtual_camera: state.virtual_camera,
            virtual_camera_details: crate::vcam::diagnostics(),
            paired_devices: self.inner.devices.lock().unwrap().list().len(),
            logs: self.inner.logs.lock().unwrap().iter().cloned().collect(),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(sess) = self.inner.session.lock().unwrap().take() {
            sess.end("desktop_closed");
        }
        self.stop_server();
        // Gives the video worker a moment to remove the virtual camera before the app quits.
        self.inner.send_pipeline(pipeline::Msg::Shutdown);
        std::thread::sleep(Duration::from_millis(200));
    }
}

async fn prune_loop(inner: Arc<Inner>) {
    let mut tick = tokio::time::interval(Duration::from_secs(600));
    loop {
        tick.tick().await;
        let connected = inner.session.lock().unwrap().as_ref().map(|s| s.device_id.clone());
        if inner.devices.lock().unwrap().prune(connected.as_deref()) {
            inner.log("forgot devices not seen for 24 hours");
            inner.emit_devices();
        }
    }
}

async fn stats_loop(inner: Arc<Inner>) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let (mut decoded, mut bytes, mut packets, mut lost) = (0u64, 0u64, 0u64, 0u64);
    let mut seconds = 0u64;
    loop {
        tick.tick().await;
        seconds += 1;
        let c = &inner.counters;
        let d = c.decoded.load(Ordering::Relaxed);
        let b = inner.net.bytes.load(Ordering::Relaxed);
        let p = inner.net.packets.load(Ordering::Relaxed);
        let l = inner.net.lost.load(Ordering::Relaxed);
        let (dd, db, dp, dl) = (d - decoded, b - bytes, p - packets, l - lost);
        (decoded, bytes, packets, lost) = (d, b, p, l);

        let phone = inner.phone_stats.lock().unwrap().clone();
        let connected = inner.phase() == Phase::Connected;
        let packet_loss = if dp + dl > 0 { dl as f64 * 100.0 / (dp + dl) as f64 } else { 0.0 };
        let rtt = phone.as_ref().and_then(|s| s.rtt_ms);
        let fps = dd as f64;
        if connected && seconds % DEVELOPER_STATS_EVERY == 0 {
            inner.debug(|| {
                format!(
                    "stats: {fps} fps, {} kbps, {packet_loss:.1}% loss, rtt {rtt:?} ms, {}x{}, dropped {}, decode errors {}, pending {}",
                    db * 8 / 1000,
                    c.width.load(Ordering::Relaxed),
                    c.height.load(Ordering::Relaxed),
                    c.dropped.load(Ordering::Relaxed),
                    c.decode_errors.load(Ordering::Relaxed),
                    c.pending.load(Ordering::Relaxed),
                )
            });
        }
        inner.events.stats(&Stats {
            fps,
            bitrate_kbps: db as f64 * 8.0 / 1000.0,
            packet_loss,
            dropped_frames: c.dropped.load(Ordering::Relaxed),
            rtt_ms: rtt,
            width: c.width.load(Ordering::Relaxed),
            height: c.height.load(Ordering::Relaxed),
            quality: if connected { quality(packet_loss, fps, rtt) } else { "none" },
            phone: if connected { phone } else { None },
        });
    }
}

fn quality(packet_loss: f64, fps: f64, rtt: Option<f64>) -> &'static str {
    if packet_loss > 5.0 || fps < 10.0 || rtt.is_some_and(|r| r > 300.0) {
        "poor"
    } else if packet_loss > 1.0 || fps < 20.0 || rtt.is_some_and(|r| r > 150.0) {
        "fair"
    } else {
        "good"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Quiet;

    impl EngineEvents for Quiet {
        fn state(&self, _: &EngineState) {}
        fn stats(&self, _: &Stats) {}
        fn preview(&self, _: Vec<u8>) {}
        fn devices(&self, _: &[Device]) {}
    }

    #[test]
    fn debug_lines_only_in_developer_mode() {
        let dir = std::env::temp_dir().join(format!("oc-log-{}", crate::pairing::random_secret(6)));
        let config = EngineConfig {
            data_dir: dir.clone(),
            network: NetworkConfig { port: 0, address: None, ice_servers: vec![] },
            processing: Default::default(),
            accept_reconnects: false,
            developer: false,
        };
        let engine = Engine::start(config, Arc::new(Quiet)).unwrap();
        engine.inner.debug(|| "hidden".into());
        engine.set_developer(true);
        engine.inner.debug(|| "shown".into());
        let logs = engine.diagnostics().logs;
        assert!(!logs.iter().any(|l| l.contains("hidden")));
        assert!(logs.iter().any(|l| l.ends_with("DEBUG shown")));
        drop(engine);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rates_connection_quality() {
        assert_eq!(quality(0.0, 30.0, Some(20.0)), "good");
        assert_eq!(quality(2.0, 30.0, None), "fair");
        assert_eq!(quality(0.0, 30.0, Some(400.0)), "poor");
        assert_eq!(quality(0.0, 5.0, None), "poor");
    }
}
