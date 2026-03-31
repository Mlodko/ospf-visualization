use std::net::Ipv4Addr;

use petgraph::csr::{EdgeIndex, NodeIndex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Node {
    Router(RouterNode),
    Network(NetworkNode)
}

impl Node {
    pub fn new_router(router_type: RouterType) -> Self {
        Self::Router(RouterNode::new(router_type))
    }
    
    pub fn router(&self) -> Option<&RouterNode> {
        match self {
            Node::Router(router) => Some(router),
            _ => None
        }
    }
    
    pub fn router_mut(&mut self) -> Option<&mut RouterNode> {
        match self {
            Node::Router(router) => Some(router),
            _ => None
        }
    }
    
    pub fn network(&self) -> Option<&NetworkNode> {
        match self {
            Node::Network(network) => Some(network),
            _ => None
        }
    }
    
    pub fn network_mut(&mut self) -> Option<&mut NetworkNode> {
        match self {
            Node::Network(network) => Some(network),
            _ => None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RouterType {
    Core,
    Distribution,
    Access
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouterNode {
    pub interfaces: Vec<RouterInterface>,
    pub router_type: RouterType
}

impl RouterNode {
    pub fn new(router_type: RouterType) -> Self {
        Self {
            interfaces: Vec::new(),
            router_type
        }
    }
    
    pub fn add_interface(&mut self, ip_address: Ipv4Addr) {
        self.interfaces.push(RouterInterface::new(ip_address));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouterInterface {
    pub ip_address: Ipv4Addr
}

impl RouterInterface {
    pub fn new(ip_address: Ipv4Addr) -> Self {
        Self { ip_address }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetworkNode {
    pub prefix: ipnetwork::IpNetwork
}