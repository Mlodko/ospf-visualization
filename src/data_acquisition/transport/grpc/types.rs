use std::{fmt::Display, time::{Duration, SystemTime, UNIX_EPOCH}};
use anyhow::anyhow;
use super::{
    gnmi::{self, GetResponse, Notification, SubscribeResponse, typed_value},
    path::extend_path,
};

#[derive(Debug, Clone, Copy)]
pub struct Timestamp(i64);

impl Timestamp {
    pub fn new(timestamp: i64) -> Self {
        Self(timestamp)
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }

    pub fn as_system_time(&self) -> Result<SystemTime, anyhow::Error> {
        let timestamp: u64 = self.0.try_into().map_err(|e| anyhow!("Failed to convert timestamp to u64: {}", e))?;
        let time = UNIX_EPOCH.checked_add(Duration::from_nanos(timestamp)).ok_or_else(|| anyhow!("Timestamp overflow"))?;
        Ok(time)
    }
}

#[derive(Debug, Clone)]
pub struct JsonIetf(pub Vec<u8>);

impl TryFrom<typed_value::Value> for JsonIetf {
    type Error = anyhow::Error;

    fn try_from(value: typed_value::Value) -> Result<Self, Self::Error> {
        if let typed_value::Value::JsonIetfVal(json) = value {
            Ok(JsonIetf(json))
        } else {
            Err(anyhow::anyhow!("Expected JsonIetfVal, got {:?}", value))
        }
    }
}

impl TryFrom<&typed_value::Value> for JsonIetf {
    type Error = anyhow::Error;

    fn try_from(value: &typed_value::Value) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

impl Display for JsonIetf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0).to_string())
    }
}

impl TryFrom<&JsonIetf> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: &JsonIetf) -> Result<Self, Self::Error> {
        serde_json::from_slice(&value.0)
    }
}

#[derive(Debug, Clone)]
pub struct JsonIetfUpdate {
    timestamp: Timestamp,
    path: gnmi::Path,
    value: JsonIetf
}

impl JsonIetfUpdate {
    fn new(timestamp: Timestamp, path: gnmi::Path, value: JsonIetf) -> Self {
        Self { timestamp, path, value }
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn path(&self) -> &gnmi::Path {
        &self.path
    }

    pub fn value(&self) -> &JsonIetf {
        &self.value
    }
}

impl JsonIetfUpdate {
    pub fn from_update(update: &gnmi::Update, prefix: Option<&gnmi::Path>, timestamp: Timestamp) -> Result<Self, anyhow::Error> {
        let path = extend_path(prefix, update.path.as_ref()).ok_or_else(|| anyhow!("Failed to extend path"))?;
        let value = update
            .val
            .as_ref()
            .ok_or_else(|| anyhow!("Update value is missing"))?
            .value
            .as_ref()
            .ok_or_else(|| anyhow!("Typed value is missing"))?
            .try_into()?;
        Ok(JsonIetfUpdate::new(timestamp, path, value))
    }
}

#[derive(Debug, Clone)]
pub struct JsonIetfDelete {
    timestamp: Timestamp,
    path: gnmi::Path,
}

impl JsonIetfDelete {
    pub fn new(timestamp: Timestamp, path: gnmi::Path) -> Self {
        Self { timestamp, path }
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn path(&self) -> &gnmi::Path {
        &self.path
    }

    pub fn from_delete(
        delete: &gnmi::Path,
        prefix: Option<&gnmi::Path>,
        timestamp: Timestamp,
    ) -> Result<Self, anyhow::Error> {
        let path = extend_path(prefix, Some(delete))
            .ok_or_else(|| anyhow!("Delete path is missing"))?;
        Ok(JsonIetfDelete::new(timestamp, path))
    }
}

#[derive(Debug, Clone)]
pub struct JsonIetfNotification {
    timestamp: Timestamp,
    updates: Vec<JsonIetfUpdate>,
    deletes: Vec<JsonIetfDelete>,
}

impl JsonIetfNotification {
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn updates(&self) -> &[JsonIetfUpdate] {
        &self.updates
    }

    pub fn deletes(&self) -> &[JsonIetfDelete] {
        &self.deletes
    }
}

impl TryFrom<&Notification> for JsonIetfNotification {
    type Error = anyhow::Error;

    fn try_from(value: &Notification) -> Result<Self, Self::Error> {
        let timestamp = Timestamp::new(value.timestamp);
        let prefix = value.prefix.as_ref();
        let updates = value.update.iter()
            .map(|update| JsonIetfUpdate::from_update(update, prefix, timestamp))
            .collect::<Result<Vec<_>, _>>()?;
        let deletes = value.delete.iter()
            .map(|delete| JsonIetfDelete::from_delete(delete, prefix, timestamp))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JsonIetfNotification {
            timestamp,
            updates,
            deletes,
        })
    }
}

impl TryFrom<&GetResponse> for Vec<JsonIetfNotification> {
    type Error = anyhow::Error;

    fn try_from(value: &GetResponse) -> Result<Self, Self::Error> {
        value
            .notification.iter()
            .map(|notification| notification.try_into())
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug, Clone)]
pub enum JsonIetfEvent {
    Notification(JsonIetfNotification),
    Sync,
    EventError(gnmi::Error),
}

impl TryFrom<&SubscribeResponse> for JsonIetfEvent {
    type Error = anyhow::Error;

    fn try_from(value: &SubscribeResponse) -> Result<Self, Self::Error> {
        match &value.response {
            Some(gnmi::subscribe_response::Response::Update(notification)) => 
                Ok(JsonIetfEvent::Notification(notification.try_into()?)),
            Some(gnmi::subscribe_response::Response::SyncResponse(_)) => Ok(JsonIetfEvent::Sync),
            Some(gnmi::subscribe_response::Response::Error(e)) => Ok(JsonIetfEvent::EventError(e.clone())),
            None => Err(anyhow!("SubscribeResponse is missing its response")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GnmiModel {
    pub name: String,
    pub organization: String,
    pub version: String,
}

impl From<GnmiModel> for gnmi::ModelData {
    fn from(model: GnmiModel) -> Self {
        Self {
            name: model.name,
            organization: model.organization,
            version: model.version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GnmiCapabilities {
    pub models: Vec<GnmiModel>,
    pub encodings: Vec<String>,
    pub gnmi_version: String,
}

#[derive(Debug, Clone)]
pub struct GnmiSubscriptionConfig {
    paths: Vec<String>,
    mode: GnmiSubscriptionModeConfig,
    models: Vec<GnmiModel>,
}

impl GnmiSubscriptionConfig {
    pub fn new(
        paths: Vec<String>,
        mode: GnmiSubscriptionModeConfig,
        models: Vec<GnmiModel>,
    ) -> Self {
        Self {
            paths,
            mode,
            models,
        }
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    pub fn mode(&self) -> &GnmiSubscriptionModeConfig {
        &self.mode
    }

    pub fn models(&self) -> &[GnmiModel] {
        &self.models
    }
}

#[derive(Debug, Clone)]
pub enum GnmiSubscriptionModeConfig {
    OnChange,
    Sample {
        sample_interval: Duration,
        heartbeat_interval: Duration,
    },
}

impl GnmiSubscriptionModeConfig {
    pub fn on_change() -> Self {
        GnmiSubscriptionModeConfig::OnChange
    }

    pub fn sample(sample_interval: Duration, heartbeat_interval: Duration) -> Self {
        GnmiSubscriptionModeConfig::Sample {
            sample_interval,
            heartbeat_interval,
        }
    }

    pub fn sample_interval(&self) -> Option<Duration> {
        match self {
            GnmiSubscriptionModeConfig::OnChange => None,
            GnmiSubscriptionModeConfig::Sample {
                sample_interval, ..
            } => Some(*sample_interval),
        }
    }

    pub fn sample_interval_nanos(&self) -> Option<u64> {
        self.sample_interval().map(|d| d.as_nanos() as u64)
    }

    pub fn heartbeat_interval(&self) -> Option<Duration> {
        match self {
            GnmiSubscriptionModeConfig::OnChange => None,
            GnmiSubscriptionModeConfig::Sample {
                heartbeat_interval, ..
            } => Some(*heartbeat_interval),
        }
    }

    pub fn heartbeat_interval_nanos(&self) -> Option<u64> {
        self.heartbeat_interval().map(|d| d.as_nanos() as u64)
    }
}

impl From<&GnmiSubscriptionModeConfig> for gnmi::SubscriptionMode {
    fn from(mode: &GnmiSubscriptionModeConfig) -> Self {
        match mode {
            GnmiSubscriptionModeConfig::OnChange => gnmi::SubscriptionMode::OnChange,
            GnmiSubscriptionModeConfig::Sample { .. } => gnmi::SubscriptionMode::Sample,
        }
    }
}