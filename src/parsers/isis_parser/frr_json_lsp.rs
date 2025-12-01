/*!
This module defines structs that can be deserialized from JSON data
retrieved using FRR's `show isis database detail json` command.

They should be parsed to core_lsp structures before being turned into Nodes.
*/

/*

JSON structure:
{
    "areas": [
        {
            "area": {
                "name": [string | null]
            },
            "levels": [
                {
                    "id": [int],
                    "lsps": [
                        {
                            "lsp": {
                                "id": [string],
                                "own": ["*" | " "] // Can ignore, describes the same thing as ownLSP
                                "ownLSP": [bool]
                            },
                            "pduLen": [int],
                            "seqNumber": [hex string, like "0x00000003"],
                            "chksum": [hex string, like "0x43c6"],
                            "holdtime": [int]
                            "attPOl": [string like "0/0/0" describing (in order) Attached, Partition repair, and Overload bits],
                            // Now begin the TLVs as additional optional properties, TODO after core LSP structure
                            [opt] "supportedProtocols": {
                                "0": "IPv4",
                                "1": "IPv6",
                                ...
                            },
                        },
                        ...
                    ]
                },
                ...
            ]
        },
        ...
    ]
}

*/

use std::collections::HashMap;

use ipnetwork::IpNetwork;
use serde::Deserialize;

use crate::parsers::isis_parser::{core_lsp::{AreaAddress, AreaAddressesTlv, ExtendedIpReachabilityNeighbor, ExtendedIpReachabilityTlv, ExtendedIsNeighbor, IsExtendedReachabilityTlv, IsLevel, Lsp, LspError, LspId, RouterCapabilityTlv, SystemId, Tlv}, hostname::HostnameMap};

#[derive(Debug, Deserialize)]
pub struct JsonLspdb {
    pub areas: Vec<JsonArea>,
}

impl JsonLspdb {
    pub fn from_string(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonArea {
    #[serde(rename = "area")]
    #[allow(dead_code)]
    pub area_props: JsonAreaProps,
    pub levels: Vec<JsonLevel>,
}

#[derive(Debug, Deserialize)]
pub struct JsonAreaProps {
    #[allow(dead_code)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JsonLevel {
    pub id: u32,
    pub lsps: Vec<JsonLsp>,
}

#[derive(Debug, Deserialize)]
pub struct JsonLsp {
    #[serde(rename = "lsp")]
    id_section: JsonLspIdSection,
    #[serde(rename = "pduLen")]
    #[allow(dead_code)]
    pdu_len: u32,
    #[serde(rename = "seqNumber")]
    seq_number: String,
    #[allow(dead_code)]
    chksum: String,
    holdtime: u16,
    #[serde(rename = "attPOl")]
    #[allow(dead_code)]
    att_p_ol_flags: String,
    // TLVs below
    #[serde(rename = "supportedProtocols")]
    #[allow(dead_code)]
    supported_protocols: Option<JsonSupportedProtocols>,
    #[serde(rename = "areaAddr")]
    area_addr: Option<String>, // This is unreliable if there's multiple area addresses for one area
    hostname: Option<String>,
    #[serde(rename = "teRouterId")]
    #[allow(dead_code)]
    te_router_id: Option<String>,
    #[serde(rename = "routerCapability")]
    router_capability: Option<JsonRouterCapability>,
    #[serde(rename = "extReach")]
    extended_reachabilities: Option<Vec<JsonExtendedReachabilityNeighbor>>,
    #[serde(rename = "ipv4")]
    #[allow(dead_code)]
    ipv4_address: Option<String>,
    #[serde(rename = "extIpReach")]
    extended_ip_reachabilities: Option<Vec<JsonExtendedIpReachability>>,
}

impl JsonLsp {
    pub fn get_area_address(&self) -> Option<AreaAddress> {
        if let Some(area_address) = &self.area_addr {
            let string_parts: Vec<String> = area_address
                .split(".")
                .map(|part| {
                    if part.len() % 2 != 0 {
                        format!("0{}", part)
                    } else {
                        part.to_string()
                    }          
                })
                .collect();
            let mut raw: Vec<u8> = Vec::new();
            for part in string_parts {
                let mut bytes = hex::decode(part).ok()?;
                raw.append(&mut bytes);
            }
            Some(AreaAddress { raw_address: raw })
        } else {
            None
        }
    }
    pub fn try_into_lsp(self, is_level: u32, hostname_map: &HostnameMap) -> Result<Lsp, LspError> {
        let hostname = if let Some(hostname) = self.hostname.as_ref() {
            hostname.to_string()
        } else {
            // Extract hostname from id_section
            let parts: Vec<&str> = self.id_section.id.split(".").collect();
            parts.get(0).ok_or(LspError::MissingData("Missing hostname".to_string()))?.to_string()
        };
        let system_id = hostname_map.get_system_id_by_hostname(&hostname).ok_or(LspError::MissingData("Missing system ID".to_string()))?;
        let lsp_id = self.id_section.get_lsp_id(&system_id.to_string());
        let lsp_id = lsp_id?;
        let is_level: IsLevel = match is_level {
            1 => IsLevel::Level1,
            2 => IsLevel::Level2,
            _ => return Err(LspError::InvalidIsLevel(is_level)),
        };
        let mut tlvs: Vec<Tlv> = Vec::new();
        
        println!("Area address");
        if let Some(area_address) = &self.get_area_address() {
            let tlv = AreaAddressesTlv::new(vec![area_address.clone()]);
            tlvs.push(Tlv::AreaAddresses(tlv));
        } else {
            println!("No area address found");
            dbg!(&self);
        }
        
        println!("Router capability");
        if let Some(router_cap) = &self.router_capability {
            tlvs.push(Tlv::RouterCapability(router_cap.try_into()?));
        }
        
        println!("Extended reachabilities");
        if let Some(ext_reaches) = &self.extended_reachabilities {
            let mut neighbors: Vec<ExtendedIsNeighbor> = Vec::with_capacity(ext_reaches.len());
            for reach in ext_reaches {
                neighbors.push(reach.try_into()?);
            }
            tlvs.push(Tlv::ExtendedReachability(IsExtendedReachabilityTlv { neighbors }));
        }
        
        println!("Extended IP reachabilities");
        if let Some(ext_ip_reaches) = &self.extended_ip_reachabilities {
            let mut neighbors: Vec<ExtendedIpReachabilityNeighbor> = Vec::with_capacity(ext_ip_reaches.len());
            for reach in ext_ip_reaches {
                neighbors.push(reach.try_into()?);
            }
            tlvs.push(Tlv::ExtendedIpReachability(ExtendedIpReachabilityTlv { neighbors }));
        }
        
        Ok(Lsp::new(
            lsp_id,
            system_id.clone(),
            is_level,
            Some(self.seq_number.clone()),
            Some(hex::encode(self.holdtime.to_ne_bytes())),
            self.get_area_address(),
            tlvs,
        ))
    }
}



#[derive(Debug, Deserialize)]
pub struct JsonLspIdSection {
    id: String,
    #[serde(rename = "ownLSP", default)]
    #[allow(dead_code)]
    own: bool,
}

impl JsonLspIdSection {
    pub fn get_lsp_id(&self, system_id: &str) -> Result<LspId, LspError> {
        // Extract pseudonode and fragment id from self.id
        let p_f_section = self.id.split(".").last().ok_or(LspError::InvalidLspId(self.id.clone()))?;
        
        // Merge actual sysID with pseudonode and fragment id
        let lsp_id_str = format!("{}.{}", system_id, p_f_section);
        
        LspId::from_string(&lsp_id_str)
    }
}

#[derive(Debug)]
pub struct JsonSupportedProtocols {
    #[allow(dead_code)]
    protocols: Vec<String>
}

impl<'de> Deserialize<'de> for JsonSupportedProtocols {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        
            let value = serde_json::Value::deserialize(deserializer)?;
            let map = value.as_object().ok_or(serde::de::Error::custom("Expected JsonSupportedProtocols to be an object"))?;
            let mut protocols: Vec<String> = Vec::with_capacity(map.len());
            for value in map.values() {
                let string = value.as_str().ok_or(serde::de::Error::custom("Expected JsonSupportedProtocols' values to be string"))?.to_string();
                protocols.push(string);
            }
            Ok(Self {
                protocols
            })
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRouterCapability {
    id: String,
    #[serde(rename = "flagD")]
    flag_d: bool,
    #[serde(rename = "flagS")]
    flag_s: bool
}

impl TryInto<RouterCapabilityTlv> for &JsonRouterCapability {
    type Error = LspError;

    fn try_into(self) -> Result<RouterCapabilityTlv, Self::Error> {
        let te_router_id = self.id.parse().map_err(|_| LspError::InvalidIpPrefixOrAddress(self.id.clone()))?;
        let mut flags = HashMap::new();
        flags.insert("D".to_string(), self.flag_d);
        flags.insert("S".to_string(), self.flag_s);
        Ok(RouterCapabilityTlv {
            te_router_id: Some(te_router_id),
            flags
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonExtendedReachabilityNeighbor {
    #[serde(rename = "mtId")]
    #[allow(dead_code)]
    mt_id: String,
    id: String,
    metric: u32
}

impl TryInto<ExtendedIsNeighbor> for &JsonExtendedReachabilityNeighbor {
    type Error = LspError;

    fn try_into(self) -> Result<ExtendedIsNeighbor, Self::Error> {
        let id_string = self.id.clone();
        let mut parts = id_string.split(".").collect::<Vec<&str>>();
        let pseudonode_id = parts.pop().ok_or(LspError::InvalidSystemId(self.id.clone()))?.to_string();
        let id_bytes: Vec<u8> = parts.into_iter()
            .flat_map(|part| {
                let part = if part.len() % 2 != 0 {
                    format!("0{}", part)
                } else {
                    part.to_string()
                };
                (0..part.len())
                    .step_by(2)
                    .map(move |i| u8::from_str_radix(&part[i..i+2], 16))
            })
            .collect::<Result<_, _>>().map_err(|_| LspError::InvalidSystemId(self.id.clone()))?;
        
        
        let system_id = SystemId::new(&id_bytes)?;
        let mut pseudonode_id_vec = hex::decode(pseudonode_id).map_err(|_| LspError::InvalidSystemId(self.id.clone()))?;
        
        let pseudonode_id = pseudonode_id_vec.pop().ok_or(LspError::InvalidSystemId(self.id.clone()))?;
        
        Ok(ExtendedIsNeighbor {
            neighbor_id: system_id,
            metric: self.metric,
            pseudonode_id: pseudonode_id
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonExtendedIpReachability {
    #[serde(rename = "mtId")]
    #[allow(dead_code)]
    mt_id: String,
    #[serde(rename = "ipReach")]
    prefix: String,
    #[serde(rename = "ipReachMetric")]
    metric: u32,
    down: bool
}

impl TryInto<ExtendedIpReachabilityNeighbor> for &JsonExtendedIpReachability {
    type Error = LspError;

    fn try_into(self) -> Result<ExtendedIpReachabilityNeighbor, Self::Error> {
        let prefix: IpNetwork = self.prefix.parse().map_err(|_| LspError::InvalidIpPrefixOrAddress(self.prefix.clone()))?;
        Ok(ExtendedIpReachabilityNeighbor::new(
            prefix,
            self.metric,
            !self.down
        ))
    }
}

mod tests {
    #[allow(unused_imports)]
    use serde_json::json;
    #[allow(unused_imports)]
    use super::*;
    
    #[test]
    fn test_areas_deserialization() {
        let json = include_str!("../../../test_data/lspdb_dump.json");
        
        let result: Result<JsonLspdb, _> = serde_json::from_str(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        println!("{:?}", parsed);
    }
    
    #[test]
    fn test_lsp_id_section_deserialization() {
        let json = json!(
            {
              "id":"r1.00-00",
              "own":"*",
              "ownLSP":true
            }
        );
        
        let result: Result<JsonLspIdSection, _> = serde_json::from_value(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        println!("{:?}", parsed);
    }
    
    #[test]
    fn test_lsp_deserialization() {
        let json = json!(
            {
                "lsp":{
                  "id":"r1.00-00",
                  "own":"*",
                  "ownLSP":true
                },
                "pduLen":101,
                "seqNumber":"0x00000002",
                "chksum":"0xb9a3",
                "holdtime":1115,
                "attPOl":"0/0/0",
            }
        );
        
        let result: Result<JsonLsp, _> = serde_json::from_value(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        println!("{:?}", parsed);
    }
    
    #[test]
    fn test_level_deserialization() {
        let json = json!(
            {
                "id":1,
                "lsps":[
                    {
                        "lsp":{
                          "id":"r1.00-00",
                          "own":"*",
                          "ownLSP":true
                        },
                        "pduLen":101,
                        "seqNumber":"0x00000002",
                        "chksum":"0xb9a3",
                        "holdtime":1115,
                        "attPOl":"0/0/0",
                    }
                ]
            }
        );
        
        let result: Result<JsonLevel, _> = serde_json::from_value(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        println!("{:?}", parsed);
    }
    
    #[test]
    fn test_area_deserialization() {
        let json = json!(
            {
            "area":{
                "name":"1"
            },
            "levels":[
                {
                    "id":1,
                    "lsps":[
                        {
                            "lsp":{
                              "id":"r1.00-00",
                              "own":"*",
                              "ownLSP":true
                            },
                            "pduLen":101,
                            "seqNumber":"0x00000002",
                            "chksum":"0xb9a3",
                            "holdtime":1115,
                            "attPOl":"0/0/0",
                        }
                    ]
                }
            ]
            }
        );
        
        let result: Result<JsonArea, _> = serde_json::from_value(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        
        let parsed = result.unwrap();
        println!("{:?}", parsed);
    }
    
    #[test]
    fn test_lsp_with_tlvs_deserialization() {
        let json = json!(
            {
                "lsp":{
                    "id":"r1.00-00",
                    "own":"*",
                    "ownLSP":true
                },
                "pduLen":101,
                "seqNumber":"0x00000002",
                "chksum":"0xb9a3",
                "holdtime":1115,
                "attPOl":"0/0/0",
                "supportedProtocols":{
                    "0":"IPv4"
                },
                "areaAddr":"49.0001",
                "hostname":"r1",
                "teRouterId":"172.21.123.11",
                "routerCapability":{
                    "id":"172.21.123.11",
                    "flagD":false,
                    "flagS":false
                },
                "segmentRoutingAlgorithm":{
                    "0":"SPF"
                },
                "extReach":[
                    {
                    "mtId":"Extended",
                    "id":"0000.0000.0001.34",
                    "metric":10
                    },
                    {
                    "mtId":"Extended",
                    "id":"0000.0000.0001.46",
                    "metric":10
                    }
                ],
                "ipv4":"172.21.123.11",
                "extIpReach":[
                    {
                    "mtId":"Extended",
                    "ipReach":"172.21.123.0/24",
                    "ipReachMetric":10,
                    "down":false
                    },
                    {
                    "mtId":"Extended",
                    "ipReach":"172.21.14.0/24",
                    "ipReachMetric":10,
                    "down":false
                    }
                ]
                }
        );
        
        let result: Result<JsonLsp, _> = serde_json::from_value(json);
        
        if let Err(e) = &result {
            println!("{}", e)
        }
        
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.supported_protocols.is_some());
        
        println!("{:#?}", parsed);
    }
    
    #[test]
    fn test_to_core_lsp() {
        let json = json!(
            {
              "lsp":{
                "id":"r1.00-00",
                "own":"*",
                "ownLSP":true
              },
              "pduLen":101,
              "seqNumber":"0x00000002",
              "chksum":"0xb9a3",
              "holdtime":1115,
              "attPOl":"0/0/0",
              "supportedProtocols":{
                "0":"IPv4"
              },
              "areaAddr":"49.0001",
              "hostname":"r1",
              "teRouterId":"172.21.123.11",
              "routerCapability":{
                "id":"172.21.123.11",
                "flagD":false,
                "flagS":false
              },
              "segmentRoutingAlgorithm":{
                "0":"SPF"
              },
              "extReach":[
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0001.64",
                  "metric":10
                },
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0001.5a",
                  "metric":10
                }
              ],
              "ipv4":"172.21.123.11",
              "extIpReach":[
                {
                  "mtId":"Extended",
                  "ipReach":"172.21.123.0/24",
                  "ipReachMetric":10,
                  "down":false
                },
                {
                  "mtId":"Extended",
                  "ipReach":"172.21.14.0/24",
                  "ipReachMetric":10,
                  "down":false
                }
              ]
            }
        );
        
        let json_lsp: JsonLsp = serde_json::from_value(json).unwrap();
        let hostname_input = include_str!("../../../test_data/isis_hostname_map_input.txt");
        
        let hostname_map = HostnameMap::build_map_from_lines(hostname_input.lines());
        
        let result = json_lsp.try_into_lsp(1, &hostname_map);
        
        if let Err(err) = &result {
            eprintln!("Error: {:?}", err);
        }
        
        assert!(result.is_ok());
        let result = result.unwrap();
        println!("{:#?}", &result);
        println!("{}", result.system_id)
    }
}
