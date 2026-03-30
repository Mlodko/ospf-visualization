use std::{net::IpAddr, time::{Duration, SystemTime}};

use egui::Ui;

use crate::{network::router::RouterId, topology::{ospf_protocol::OspfProtocol, protocol::RoutingProtocol}};

pub trait Neighbor<P>
where P: RoutingProtocol {
    type NeighborDetails;
    type NeighborState;
    fn get_id(&self) -> RouterId;
    fn get_remote_ip(&self) -> Option<IpAddr>;
    fn get_last_successful_update(&self) -> Option<Duration>;
    
    fn get_state(&self) -> Self::NeighborState;
    
    fn get_details(&self) -> &Self::NeighborDetails;
    
    fn render_row(&self, ui: &mut Ui);
}

pub struct OspfNeighbor {
    pub nbr_ip_address: IpAddr,
    pub nbr_router_id: RouterId,
    pub state: OspfNeighborState,
    pub event_count: u32,
    pub last_update: SystemTime,
}

impl Neighbor<OspfProtocol> for OspfNeighbor {
    type NeighborDetails = Self;

    type NeighborState = OspfNeighborState;

    fn get_id(&self) -> RouterId {
        self.nbr_router_id.clone()
    }

    fn get_remote_ip(&self) -> Option<IpAddr> {
        Some(self.nbr_ip_address)
    }

    fn get_last_successful_update(&self) -> Option<Duration> {
        self.last_update.elapsed().ok()
    }

    fn get_state(&self) -> Self::NeighborState {
        self.state
    }

    fn get_details(&self) -> &Self::NeighborDetails {
        self
    }

    fn render_row(&self, ui: &mut Ui) {
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OspfNeighborState {
    ExchangeStart,
    Loading,
    Attempt,
    Exchange,
    Down,
    Init,
    Full,
    TwoWay
}