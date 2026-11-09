pub mod gnmi_ext {
    tonic::include_proto!("gnmi_ext");
}

pub mod gnmi {
    tonic::include_proto!("gnmi");
}

pub mod client;
pub mod path;
pub mod types;

pub mod prelude {
    #![allow(unused_imports)]
    pub use super::client::{GnmiClient, GnmiSubscription};
    pub use super::path::{extend_path, parse_path};
    pub use super::types::{
        GnmiCapabilities, GnmiModel, GnmiSubscriptionConfig, GnmiSubscriptionModeConfig, JsonIetf,
        JsonIetfDelete, JsonIetfNotification, JsonIetfSubscriptionEvent, JsonIetfUpdate, Timestamp,
    };
}
