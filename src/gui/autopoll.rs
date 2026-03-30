use crate::{
    data_aquisition::{snmp::SnmpClient, ssh::SshClient},
    gui::new_app::AppPanel,
    network::{
        node::Node,
        router::{InterfaceStats, RouterId},
    },
    parsers::isis_parser::topology::{IsIsBfsTopology, IsIsTopology},
    topology::{OspfSnmpTopology, ospf_protocol::OspfBfsSnmpTopology, source::SnapshotSource},
};
use anyhow::anyhow;
use egui::CollapsingHeader;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, watch},
    task::JoinHandle,
};
type PollResult = Result<PollMessage, String>;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct AutoPoller {
    source_specs: HashMap<RouterId, SourceSpec>,
    enabled: bool,
    poll_interval: Duration,
    interval_tx: Option<watch::Sender<Duration>>,
    interval_rx: Option<watch::Receiver<Duration>>,
    poll_rx: Option<mpsc::Receiver<PollResult>>,
    poll_tx: Option<mpsc::Sender<PollResult>>,
    poll_handles: HashMap<RouterId, JoinHandle<()>>,
    runtime: Option<Arc<Runtime>>,
}

impl AppPanel for AutoPoller {
    fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn super::actions::AppActions,
    ) {
        CollapsingHeader::new("Auto Poller").show(ui, |ui| {
            if ui.button("Print sources").clicked() {
                println!("Sources:");
                for (id, spec) in &self.source_specs {
                    println!(" - {}", id);
                }
            }
            let mut enabled = self.enabled;
            ui.checkbox(&mut enabled, "Enabled");
            if enabled != self.enabled {
                if enabled {
                    let _ = self.start();
                } else {
                    self.stop();
                }
            }
            let mut interval_seconds = self.poll_interval().as_secs();
            ui.add(egui::Slider::new(&mut interval_seconds, 1..=60).text("Interval (seconds)"));

            if self.poll_interval().as_secs() != interval_seconds {
                let _ = self.set_poll_interval(Duration::from_secs(interval_seconds));
            }
        });
    }
}

impl AutoPoller {
    pub fn new(interval: Option<Duration>, runtime: Arc<Runtime>) -> Self {
        Self {
            source_specs: HashMap::new(),
            enabled: false,
            poll_interval: interval.unwrap_or(DEFAULT_POLL_INTERVAL),
            interval_tx: None,
            interval_rx: None,
            poll_rx: None,
            poll_tx: None,
            poll_handles: HashMap::new(),
            runtime: Some(runtime),
        }
    }

    pub fn source_specs(&self) -> &HashMap<RouterId, SourceSpec> {
        &self.source_specs
    }

    pub fn set_source_specs(&mut self, source_specs: HashMap<RouterId, SourceSpec>) {
        self.source_specs = source_specs;
        self.reset();
    }

    pub fn clear_source_specs(&mut self) {
        self.source_specs.clear();
        self.reset();
    }

    pub fn poll_rx(&self) -> Option<&mpsc::Receiver<PollResult>> {
        self.poll_rx.as_ref()
    }

    pub fn poll_rx_mut(&mut self) -> Option<&mut mpsc::Receiver<PollResult>> {
        self.poll_rx.as_mut()
    }

    pub fn add_source(&mut self, router_id: RouterId, source_spec: SourceSpec) {
        let new_source_added = self
            .source_specs
            .insert(router_id.clone(), source_spec.clone())
            .is_none();
        if new_source_added && self.enabled() {
            self.start_polling(&router_id, &source_spec);
            println!("Added new source {} and started polling", &router_id);
        }
    }

    fn start_polling(&mut self, source_id: &RouterId, spec: &SourceSpec) {
        println!("Starting polling for source {}", &source_id);
        let poll_tx = self.poll_tx.clone().unwrap();
        let interval_rx = self.interval_rx.clone().unwrap();

        let runtime = self.runtime.clone().unwrap();
        let handle = runtime.spawn(run_source_task(
            source_id.clone(),
            spec.clone(),
            interval_rx,
            poll_tx,
        ));

        self.poll_handles.insert(source_id.clone(), handle);
    }

    pub fn remove_source(&mut self, router_id: &RouterId) {
        self.source_specs.remove(router_id);
    }

    pub fn set_poll_interval(&mut self, interval: Duration) -> anyhow::Result<()> {
        println!("Setting poll interval to {:?}", interval);
        if self.poll_interval == interval {
            return Ok(());
        }

        self.poll_interval = interval;

        if !self.enabled {
            return Ok(());
        }

        if let Some(tx) = &self.interval_tx {
            tx.send(interval)?;
        }
        Ok(())
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn reset(&mut self) {
        let enabled_before = self.enabled();
        self.stop();
        if enabled_before {
            let _ = self.start();
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        println!("Starting auto poller");
        if self.enabled {
            return Ok(());
        }

        // Create channels
        let (poll_tx, poll_rx) = mpsc::channel(100);
        self.poll_tx = Some(poll_tx);
        self.poll_rx = Some(poll_rx);

        let (interval_tx, interval_rx) = watch::channel(if self.poll_interval.is_zero() {
            DEFAULT_POLL_INTERVAL
        } else {
            self.poll_interval
        });

        self.interval_tx = Some(interval_tx);
        self.interval_rx = Some(interval_rx);
        // Start tasks
        let specs = self
            .source_specs()
            .iter()
            .map(|(src_id, spec)| (src_id.clone(), spec.clone()))
            .collect::<Vec<_>>();
        for (src_id, spec) in specs {
            self.start_polling(&src_id, &spec);
        }

        self.enabled = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        println!("Stopping auto poller");
        if !self.enabled {
            return;
        }

        for (_, h) in self.poll_handles.drain() {
            h.abort();
        }
        self.poll_tx = None;
        self.poll_rx = None;
        self.interval_tx = None;
        self.enabled = false;
    }
}

async fn run_source_task(
    source_id: RouterId,
    spec: SourceSpec,
    mut interval_rx: watch::Receiver<Duration>,
    poll_tx: mpsc::Sender<PollResult>,
) {
    println!("Starting source task for {}", source_id);

    let jitter = Duration::from_millis(hash_source_id(&source_id) % 250);
    tokio::time::sleep(jitter).await;

    let mut source = match spec.build_topology().await {
        Ok(topo) => Some(topo),
        Err(err) => {
            let _ = poll_tx
                .send(Err(format!(
                    "Failed to build topology for source {}: {}",
                    source_id, err
                )))
                .await;
            None
        }
    };

    let mut backoff = ExpBackoff::new(*interval_rx.borrow(), 2.0, Duration::from_secs(300));
    let mut ticker = tokio::time::interval(backoff.base());

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if source.is_none() {
                    match spec.build_topology().await {
                        Ok(s) => source = Some(s),
                        Err(e) => {
                            let _ = poll_tx.send(Err(format!("Reinit failed for source {}: {}\nExponential backoff: {} s",
                                source_id, e, backoff.next().as_millis() / 1000))).await;
                            ticker = tokio::time::interval(backoff.current());
                            continue;
                        }
                    }
                }
                match source.as_mut().unwrap().fetch_snapshot().await {
                    Ok((id, nodes, stats)) => {
                        if ticker.period() != backoff.base() {
                            ticker = tokio::time::interval(backoff.base());
                        }
                        let _ = poll_tx.send(Ok(PollMessage {
                            source_id: id,
                            nodes,
                            if_stats: stats,
                            source_spec: spec.clone()
                        })).await;
                    }
                    Err(e) => {
                        source = None; // Force rebuild next tick
                        let _ = poll_tx.send(Err(format!("Failed to fetch snapshot for source {}: {}",
                            source_id, e))).await;
                    }
                }
            }
            changed = interval_rx.changed() => {
                if changed.is_err() {
                    // sender dropped, exit task
                    break;
                }
                let new_interval = *interval_rx.borrow();
                if new_interval != backoff.base() {
                    backoff.change_base(new_interval);
                    ticker = tokio::time::interval(backoff.base());
                }
            }
        }
    }
}

fn hash_source_id(source_id: &RouterId) -> u64 {
    let mut hasher = DefaultHasher::new();
    source_id.hash(&mut hasher);
    hasher.finish()
}

pub struct PollMessage {
    pub source_id: RouterId,
    pub nodes: Vec<Node>,
    pub if_stats: Vec<InterfaceStats>,
    pub source_spec: SourceSpec,
}

impl PollMessage {
    pub fn source_id(&self) -> &RouterId {
        &self.source_id
    }
    pub fn nodes(&self) -> &Vec<Node> {
        &self.nodes
    }
    pub fn if_stats(&self) -> &Vec<InterfaceStats> {
        &self.if_stats
    }
    pub fn source_spec(&self) -> &SourceSpec {
        &self.source_spec
    }
}

#[derive(Clone)]
pub enum ProtocolKind {
    Ospf,
    Isis,
}

#[derive(Clone)]
pub enum AcquisitionConfig {
    Snmp(SnmpAcquisitionConfig),
    Ssh(SshAcquisitionConfig),
}

#[derive(Clone)]
pub struct SnmpAcquisitionConfig {
    address: SocketAddr,
    community: String,
    snmp_version: snmp2::Version,
    security: Option<snmp2::v3::Security>,
}

#[derive(Clone)]
pub struct SshAcquisitionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[derive(Clone)]
pub struct SourceSpec {
    pub protocol: ProtocolKind,
    pub acquisition: AcquisitionConfig,
}

impl SourceSpec {
    pub fn new_ssh(
        host: String,
        port: u16,
        username: String,
        password: String,
        protocol: ProtocolKind,
    ) -> Self {
        Self {
            protocol,
            acquisition: AcquisitionConfig::Ssh(SshAcquisitionConfig {
                host,
                port,
                username,
                password,
            }),
        }
    }

    pub fn new_snmp(
        address: SocketAddr,
        community: String,
        version: snmp2::Version,
        security: Option<snmp2::v3::Security>,
        protocol: ProtocolKind,
    ) -> Self {
        Self {
            protocol,
            acquisition: AcquisitionConfig::Snmp(SnmpAcquisitionConfig {
                address,
                community,
                snmp_version: version,
                security,
            }),
        }
    }

    pub async fn build_topology(&self) -> Result<Box<dyn SnapshotSource>, String> {
        match (&self.protocol, &self.acquisition) {
            (ProtocolKind::Ospf, AcquisitionConfig::Snmp(config)) => {
                let client = SnmpClient::new(
                    config.address,
                    &config.community,
                    config.snmp_version,
                    config.security.clone(),
                );
                let topo = OspfBfsSnmpTopology::from_snmp_client(client)
                    .await
                    .map_err(|e| format!("Failed to build OSPF topology: {}", e))?;
                Ok(Box::new(topo))
            }
            (ProtocolKind::Isis, AcquisitionConfig::Ssh(config)) => {
                let client = SshClient::new_with_password(
                    config.username.clone(),
                    config.host.clone(),
                    config.password.clone(),
                    config.port,
                );
                let topo = IsIsBfsTopology::new_from_ssh_client(client)
                    .await
                    .map_err(|e| format!("Failed to build ISIS topology: {}", e))?;
                Ok(Box::new(topo))
            }
            _ => Err("Unsupported protocol or acquisition method".to_string()),
        }
    }
}

struct ExpBackoff {
    base: Duration,
    current: Duration,
    factor: f32,
    max: Duration,
}

impl ExpBackoff {
    pub fn new(base: Duration, factor: f32, max: Duration) -> Self {
        Self {
            base,
            current: base,
            factor,
            max,
        }
    }

    pub fn base(&self) -> Duration {
        self.base
    }

    pub fn reset(&mut self) {
        self.current = self.base;
    }

    pub fn change_base(&mut self, new_base: Duration) {
        self.base = new_base;
        self.reset();
    }

    pub fn current(&self) -> Duration {
        self.current
    }

    pub fn next(&mut self) -> Duration {
        let next = Duration::from_secs_f32(self.current.as_secs_f32() * self.factor);
        let next_clamped = std::cmp::min(next, self.max);
        self.current = next_clamped;
        next_clamped
    }
}
