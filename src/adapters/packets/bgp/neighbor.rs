use std::net::IpAddr;


pub struct BgpPeer {
    pub peer_address: IpAddr,
    pub admin_state: AdminState,
    pub advertised_capabilities: Vec<BgpCapabilities>,
    
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AdminState {
    Enable = 1,
    Disable = 2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BgpCapabilities {
    MpBgp = 0,
    RouteRefresh = 1,
    ExtNhEncoding = 2,
    GracefulRestart = 3,
    FourOctetAsn = 4,
    OrfSendExcomm = 5,
    OrfRecieveExcomm = 6,
    AddPath = 7,
}