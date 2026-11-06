use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::{Stream, stream};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{ClientTlsConfig, Endpoint};

pub mod gnmi_ext {
    tonic::include_proto!("gnmi_ext");
}

pub mod gnmi {
    tonic::include_proto!("gnmi");
}

#[derive(Debug)]
struct NoTlsVerifier;

impl ServerCertVerifier for NoTlsVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &tonic::transport::CertificateDer<'_>,
        intermediates: &[tonic::transport::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &tonic::transport::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &tonic::transport::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Clone)]
struct GnmiAuthInterceptor {
    username: MetadataValue<tonic::metadata::Ascii>,
    password: MetadataValue<tonic::metadata::Ascii>,
}

impl tonic::service::Interceptor for GnmiAuthInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        request
            .metadata_mut()
            .insert("username", self.username.clone());
        request
            .metadata_mut()
            .insert("password", self.password.clone());
        Ok(request)
    }
}

pub struct GnmiClient {
    client: gnmi::g_nmi_client::GNmiClient<
        InterceptedService<tonic::transport::Channel, GnmiAuthInterceptor>,
    >,
}

#[derive(Debug, Clone)]
pub struct GnmiModel {
    pub name: String,
    pub organization: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct GnmiCapabilities {
    pub models: Vec<GnmiModel>,
    pub encodings: Vec<String>,
    pub gnmi_version: String,
}

pub type GnmiSubscription =
    Pin<Box<dyn Stream<Item = Result<gnmi::SubscribeResponse, tonic::Status>> + Send>>;

const MAX_GNMI_RESPONSE_SIZE: usize = 64 * 1024 * 1024;

impl GnmiClient {
    pub async fn new(
        uri: impl Into<Bytes>,
        username: &str,
        password: &str,
    ) -> Result<Self, anyhow::Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let auth = GnmiAuthInterceptor {
            username: MetadataValue::try_from(username)?,
            password: MetadataValue::try_from(password)?,
        };
        let uri = uri.into();
        let use_tls = uri.starts_with(b"https://");
        let endpoint = Endpoint::from_shared(uri)?;
        let endpoint = if use_tls {
            endpoint.tls_config_with_verifier(ClientTlsConfig::new(), Arc::new(NoTlsVerifier))?
        } else {
            endpoint
        };
        let channel = endpoint.connect().await?;

        let client = gnmi::g_nmi_client::GNmiClient::with_interceptor(channel, auth)
            .max_decoding_message_size(MAX_GNMI_RESPONSE_SIZE);

        Ok(Self { client })
    }

    pub async fn capabilities(&mut self) -> Result<GnmiCapabilities, tonic::Status> {
        let request = tonic::Request::new(gnmi::CapabilityRequest { extension: vec![] });
        let response = self.client.capabilities(request).await?;
        let response = response.into_inner();
        Ok(GnmiCapabilities {
            models: response
                .supported_models
                .into_iter()
                .map(|model| GnmiModel {
                    name: model.name,
                    organization: model.organization,
                    version: model.version,
                })
                .collect(),
            encodings: response
                .supported_encodings
                .into_iter()
                .filter_map(gnmi::Encoding::from_i32)
                .map(|encoding| encoding.as_str_name().to_owned())
                .collect(),
            gnmi_version: response.g_nmi_version,
        })
    }

    pub async fn get(&mut self, path: &str) -> Result<gnmi::GetResponse, tonic::Status> {
        self.get_with_models(path, &[]).await
    }

    pub async fn get_with_models(
        &mut self,
        path: &str,
        models: &[GnmiModel],
    ) -> Result<gnmi::GetResponse, tonic::Status> {
        let request = tonic::Request::new(gnmi::GetRequest {
            path: vec![parse_path(path)],
            r#type: gnmi::get_request::DataType::State as i32,
            encoding: gnmi::Encoding::JsonIetf as i32,
            prefix: None,
            use_models: models.iter().cloned().map(Into::into).collect(),
            extension: vec![],
        });
        Ok(self.client.get(request).await?.into_inner())
    }

    pub async fn subscribe(&mut self, paths: &[&str]) -> Result<GnmiSubscription, tonic::Status> {
        self.subscribe_with_models(paths, &[]).await
    }

    pub async fn subscribe_with_models(
        &mut self,
        paths: &[&str],
        models: &[GnmiModel],
    ) -> Result<GnmiSubscription, tonic::Status> {
        let request = gnmi::SubscribeRequest {
            request: Some(gnmi::subscribe_request::Request::Subscribe(
                gnmi::SubscriptionList {
                    subscription: paths
                        .iter()
                        .map(|path| gnmi::Subscription {
                            path: Some(parse_path(path)),
                            mode: gnmi::SubscriptionMode::Sample as i32,
                            sample_interval: 0,
                            heartbeat_interval: 0,
                            suppress_redundant: false,
                        })
                        .collect(),
                    mode: gnmi::subscription_list::Mode::Stream as i32,
                    encoding: gnmi::Encoding::JsonIetf as i32,
                    prefix: None,
                    qos: None,
                    allow_aggregation: false,
                    use_models: models.iter().cloned().map(Into::into).collect(),
                    updates_only: false,
                },
            )),
            extension: vec![],
        };
        let response = self
            .client
            .subscribe(stream::once(std::future::ready(request)))
            .await?;
        Ok(Box::pin(response.into_inner()))
    }
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

fn parse_path(path: &str) -> gnmi::Path {
    gnmi::Path {
        elem: path
            .trim_start_matches('/')
            .split('/')
            .filter(|element| !element.is_empty())
            .map(parse_path_element)
            .collect(),
        element: vec![],
        origin: String::new(),
        target: String::new(),
    }
}

fn parse_path_element(element: &str) -> gnmi::PathElem {
    let (name, predicates) = element
        .split_once('[')
        .map_or((element, ""), |(name, predicates)| (name, predicates));
    let mut key = std::collections::HashMap::new();

    for predicate in predicates
        .split(']')
        .map(|part| part.trim_start_matches('['))
    {
        if predicate.is_empty() {
            continue;
        }
        if let Some((key_name, value)) = predicate.split_once('=') {
            key.insert(key_name.to_owned(), value.to_owned());
        }
    }

    gnmi::PathElem {
        name: name.to_owned(),
        key,
    }
}

mod tests {
    use crate::data_acquisition::transport::grpc::gnmi::typed_value::Value::JsonIetfVal;

    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_gnmi_client() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = GnmiClient::new("http://200.1.0.1:57401", "admin", "NokiaSrl1!")
                .await
                .unwrap();
            let capabilities = client.capabilities().await.unwrap();
            println!("Capabilities: {:?}", capabilities);
        })
    }

    #[test]
    fn test_gnmi_get() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = GnmiClient::new("http://200.1.0.1:57401", "admin", "NokiaSrl1!")
                .await
                .unwrap();
            let path = "/srl_nokia-network-instance:network-instance[name=*]/srl_nokia-network-instance:protocols/srl_nokia-ospf:ospf/srl_nokia-ospf:instance[name=*]/srl_nokia-ospf:area[area-id=*]/srl_nokia-ospf:lsdb";
            dbg!(parse_path(path));
            let response = client.get(path).await.unwrap();
            println!("Get Response: {:?}", &response);
            let json_raw = response.notification[0].update[0]
                .val
                .as_ref()
                .unwrap()
                .value
                .as_ref()
                .unwrap();
            if let JsonIetfVal(json) = json_raw {
                println!("JSON IETF Value: {}", String::from_utf8_lossy(json));
            }
        })
    }

    #[test]
    fn parses_path_keys() {
        let path = parse_path(
            "/network-instance[name=default]/protocols/isis/instance[name=1]/level[level-number=*]",
        );

        assert_eq!(path.elem[0].name, "network-instance");
        assert_eq!(path.elem[0].key["name"], "default");
        assert_eq!(path.elem[3].key["name"], "1");
        assert_eq!(path.elem[4].key["level-number"], "*");
    }
}
