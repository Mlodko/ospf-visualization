use std::{pin::Pin, sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::Stream;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tonic::{metadata::MetadataValue, service::interceptor::InterceptedService, transport::{ClientTlsConfig, Endpoint}};
use super::{gnmi, types::*, path::parse_path};

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

    pub async fn subscribe(
        &mut self,
        config: &GnmiSubscriptionConfig,
    ) -> Result<GnmiSubscription, tonic::Status> {
        let mode: gnmi::SubscriptionMode = config.mode().into();
        let request = gnmi::SubscribeRequest {
            request: Some(gnmi::subscribe_request::Request::Subscribe(
                gnmi::SubscriptionList {
                    subscription: config
                        .paths()
                        .iter()
                        .map(|path| gnmi::Subscription {
                            path: Some(parse_path(path)),
                            mode: mode as i32,
                            sample_interval: config.mode().sample_interval_nanos().unwrap_or(0),
                            heartbeat_interval: config
                                .mode()
                                .heartbeat_interval_nanos()
                                .unwrap_or(0),
                            suppress_redundant: false,
                        })
                        .collect(),
                    mode: gnmi::subscription_list::Mode::Stream as i32,
                    encoding: gnmi::Encoding::JsonIetf as i32,
                    prefix: None,
                    qos: None,
                    allow_aggregation: false,
                    use_models: config.models().iter().cloned().map(Into::into).collect(),
                    updates_only: false,
                },
            )),
            extension: vec![],
        };
        let response = self
            .client
            .subscribe(futures_util::stream::once(std::future::ready(request)))
            .await?;
        Ok(Box::pin(response.into_inner()))
    }
}



mod tests {
    use futures_util::StreamExt;
use tokio::{runtime::Runtime, time::Instant};
    use super::*;


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
            let json_ietf = JsonIetf::try_from(json_raw.clone()).unwrap();
            println!("JSON IETF: {}", &json_ietf);
            let json_value: serde_json::Value = serde_json::Value::try_from(&json_ietf).unwrap();
            println!("JSON Value: {}", serde_json::to_string_pretty(&json_value).unwrap());
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

    #[test]
    fn parses_escaped_slashes_in_keys() {
        let path = parse_path("/interface[name=ethernet-1\\/1]/statistics/in-packets");

        assert_eq!(path.elem.len(), 3);
        assert_eq!(path.elem[0].name, "interface");
        assert_eq!(path.elem[0].key["name"], "ethernet-1/1");
    }

    #[test]
    fn test_gnmi_subscribe() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = GnmiClient::new("http://200.1.0.1:57401", "admin", "NokiaSrl1!")
                .await
                .unwrap();
            let paths = vec![
                "/interface[name=ethernet-1\\/1]/statistics/in-packets".to_string(),
                "/interface[name=ethernet-1\\/1]/statistics/out-packets".to_string(),
            ];

            dbg!(parse_path(&paths[0]));

            let config = GnmiSubscriptionConfig::new(
                paths,
                GnmiSubscriptionModeConfig::Sample {
                    sample_interval: Duration::from_secs(2),
                    heartbeat_interval: Duration::ZERO,
                },
                vec![],
            );
            let mut subscription = client.subscribe(&config).await.unwrap();

            let expected_messages = 6;
            let mut messages = 0;
            let deadline = Instant::now() + Duration::from_secs(11);
            while let Ok(Some(response)) =
                tokio::time::timeout_at(deadline, subscription.next()).await
            {
                println!("Subscription message: {:?}", response);
                match response.unwrap().response {
                    Some(gnmi::subscribe_response::Response::Update(_)) => messages += 1,
                    Some(gnmi::subscribe_response::Response::Error(error)) => {
                        panic!("subscription returned an error: {}", error.message)
                    }
                    Some(gnmi::subscribe_response::Response::SyncResponse(_)) | None => {}
                }
            }
            assert_eq!(
                messages, expected_messages,
                "expected an initial update plus five 2-second samples"
            );
        });
    }
}