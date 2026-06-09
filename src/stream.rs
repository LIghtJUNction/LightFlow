//! Runtime stream framing for high-frequency status and preview data.
//!
//! The transport can evolve to WebTransport/HTTP3 without changing the frame
//! schema. The current gateway exposes discovery and snapshot endpoints so UI
//! and agent clients can negotiate the binary format before a live transport is
//! enabled.

use crate::api::{ApiError, ApiService};
use flatbuffers::FlatBufferBuilder;
use serde::{Deserialize, Serialize};
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use wtransport::endpoint::IncomingSession;
use wtransport::tls::Sha256DigestFmt;
use wtransport::{Endpoint, Identity, ServerConfig};

/// FlatBuffers file identifier for LightFlow runtime stream frames.
pub const FILE_IDENTIFIER: &str = "LFRS";

/// MIME type for LightFlow runtime stream FlatBuffers frames.
pub const MIME_TYPE: &str = "application/vnd.lightflow.runtime-frame+flatbuffers";

/// Versioned FlatBuffers schema for runtime stream frames.
pub const SCHEMA: &str = r#"namespace lightflow.runtime;

enum FrameKind:ubyte {
  RunSnapshot = 1,
  RunEvent = 2,
  RunTrace = 3,
  Preview = 4
}

table RuntimeFrame {
  version:ushort = 1;
  kind:FrameKind;
  run_id:string;
  status:string;
  sequence:ulong;
  timestamp_unix_ms:ulong;
  payload_json:string;
}

root_type RuntimeFrame;
file_identifier "LFRS";
"#;

const FRAME_VERSION: u16 = 1;
const FRAME_KIND_RUN_SNAPSHOT: u8 = 1;
const VT_VERSION: flatbuffers::VOffsetT = 4;
const VT_KIND: flatbuffers::VOffsetT = 6;
const VT_RUN_ID: flatbuffers::VOffsetT = 8;
const VT_STATUS: flatbuffers::VOffsetT = 10;
const VT_SEQUENCE: flatbuffers::VOffsetT = 12;
const VT_TIMESTAMP_UNIX_MS: flatbuffers::VOffsetT = 14;
const VT_PAYLOAD_JSON: flatbuffers::VOffsetT = 16;

/// Runtime stream discovery metadata.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamInfo {
    pub transports: Vec<RuntimeStreamTransport>,
    pub frame: RuntimeStreamFrameInfo,
    pub endpoints: RuntimeStreamEndpoints,
}

/// One runtime stream transport option.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamTransport {
    pub name: String,
    pub status: RuntimeStreamTransportStatus,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<RuntimeStreamTlsInfo>,
}

/// Transport readiness.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamTransportStatus {
    Available,
    Planned,
}

/// TLS guidance for secure transports.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamTlsInfo {
    pub self_signed: bool,
    pub certificate_hash_option: String,
    pub note: String,
}

/// Runtime stream frame metadata.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamFrameInfo {
    pub encoding: String,
    pub file_identifier: String,
    pub mime_type: String,
    pub schema_uri: String,
}

/// Runtime stream endpoint paths.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamEndpoints {
    pub discovery: String,
    pub schema: String,
    pub snapshot_template: String,
}

/// Build runtime stream discovery metadata.
#[must_use]
pub fn stream_info() -> RuntimeStreamInfo {
    RuntimeStreamInfo {
        transports: vec![
            RuntimeStreamTransport {
                name: "http_snapshot".to_owned(),
                status: RuntimeStreamTransportStatus::Available,
                endpoint: "/runtime/streams/{run_id}/snapshot.fb".to_owned(),
                command: Some("lightflow serve --port 5174".to_owned()),
                tls: None,
            },
            RuntimeStreamTransport {
                name: "webtransport".to_owned(),
                status: RuntimeStreamTransportStatus::Available,
                endpoint: "https://127.0.0.1:4433/{run_id}".to_owned(),
                command: Some("lightflow stream serve-webtransport --port 4433".to_owned()),
                tls: Some(RuntimeStreamTlsInfo {
                    self_signed: true,
                    certificate_hash_option: "serverCertificateHashes".to_owned(),
                    note: "The standalone WebTransport server prints SHA-256 certificate hash bytes at startup for browser clients.".to_owned(),
                }),
            },
        ],
        frame: RuntimeStreamFrameInfo {
            encoding: "flatbuffers".to_owned(),
            file_identifier: FILE_IDENTIFIER.to_owned(),
            mime_type: MIME_TYPE.to_owned(),
            schema_uri: "/runtime/streams/schema.fbs".to_owned(),
        },
        endpoints: RuntimeStreamEndpoints {
            discovery: "/runtime/streams".to_owned(),
            schema: "/runtime/streams/schema.fbs".to_owned(),
            snapshot_template: "/runtime/streams/{run_id}/snapshot.fb".to_owned(),
        },
    }
}

/// Serve run snapshots over WebTransport/HTTP3 on a standalone QUIC listener.
///
/// Clients connect to `https://<host>:<port>/<run_id>`. On a valid run id the
/// server opens one unidirectional stream, writes a `RuntimeFrame` FlatBuffer,
/// finishes the stream, and keeps accepting more sessions. A self-signed
/// development identity is generated at startup; browser clients must accept the
/// printed certificate hash through `serverCertificateHashes`.
pub async fn serve_webtransport(service: ApiService, bind: SocketAddr) -> io::Result<()> {
    let identity =
        Identity::self_signed(["localhost", "127.0.0.1", "::1"]).map_err(io::Error::other)?;
    let cert_hash = identity.certificate_chain().as_slice()[0]
        .hash()
        .fmt(Sha256DigestFmt::BytesArray);
    let config = ServerConfig::builder()
        .with_bind_address(bind)
        .with_identity(identity)
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();
    let endpoint = Endpoint::server(config).map_err(io::Error::other)?;
    let local_addr = endpoint.local_addr().map_err(io::Error::other)?;

    eprintln!("LightFlow WebTransport runtime stream listening on https://{local_addr}");
    eprintln!("WebTransport serverCertificateHashes SHA-256 bytes: {cert_hash}");
    eprintln!("Connect to https://{local_addr}/<run_id> to receive one FlatBuffers snapshot");

    for connection_id in 0_u64.. {
        let incoming = endpoint.accept().await;
        let service = service.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_webtransport_session(service, incoming).await {
                eprintln!("webtransport session {connection_id} error: {error}");
            }
        });
    }

    Ok(())
}

async fn handle_webtransport_session(
    service: ApiService,
    incoming: IncomingSession,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_request = incoming.await?;
    let run_id = session_request
        .path()
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned();

    if run_id.is_empty() {
        session_request.not_found().await;
        return Ok(());
    }

    let frame = match encode_run_snapshot(&service, &run_id) {
        Ok(frame) => frame,
        Err(error) if error.status_code() == 404 => {
            session_request.not_found().await;
            return Ok(());
        }
        Err(error) => {
            let connection = session_request.accept().await?;
            let mut stream = connection.open_uni().await?.await?;
            stream
                .write_all(format!("LightFlow stream error: {error}\n").as_bytes())
                .await?;
            stream.finish().await?;
            return Ok(());
        }
    };

    let connection = session_request
        .accept_with_headers([
            ("x-lightflow-frame-encoding", "flatbuffers"),
            ("x-lightflow-flatbuffers-file-identifier", FILE_IDENTIFIER),
            ("content-type", MIME_TYPE),
        ])
        .await?;
    let mut stream = connection.open_uni().await?.await?;
    stream.write_all(&frame).await?;
    stream.finish().await?;
    Ok(())
}

/// Encode the current run state as one FlatBuffers runtime stream frame.
pub fn encode_run_snapshot(service: &ApiService, run_id: &str) -> Result<Vec<u8>, ApiError> {
    let status = service.run_status(run_id)?;
    let events = service.run_events(run_id)?;
    let trace = service.run_trace(run_id)?;
    let payload = serde_json::json!({
        "status": status,
        "events_jsonl": events,
        "trace_jsonl": trace,
    });
    let payload = serde_json::to_string(&payload).map_err(|error| {
        ApiError::InvalidRequest(format!("failed to encode stream payload: {error}"))
    })?;
    Ok(encode_runtime_frame(
        FRAME_KIND_RUN_SNAPSHOT,
        run_id,
        status.status.as_str(),
        0,
        timestamp_unix_ms(),
        &payload,
    ))
}

fn encode_runtime_frame(
    kind: u8,
    run_id: &str,
    status: &str,
    sequence: u64,
    timestamp_unix_ms: u64,
    payload_json: &str,
) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let run_id = builder.create_string(run_id);
    let status = builder.create_string(status);
    let payload_json = builder.create_string(payload_json);

    let frame = builder.start_table();
    builder.push_slot(
        VT_PAYLOAD_JSON,
        payload_json,
        flatbuffers::WIPOffset::new(0),
    );
    builder.push_slot(VT_TIMESTAMP_UNIX_MS, timestamp_unix_ms, 0);
    builder.push_slot(VT_SEQUENCE, sequence, 0);
    builder.push_slot(VT_STATUS, status, flatbuffers::WIPOffset::new(0));
    builder.push_slot(VT_RUN_ID, run_id, flatbuffers::WIPOffset::new(0));
    builder.push_slot(VT_KIND, kind, 0);
    builder.push_slot(VT_VERSION, FRAME_VERSION, 1);
    let frame = builder.end_table(frame);
    builder.finish(frame, Some(FILE_IDENTIFIER));
    builder.finished_data().to_vec()
}

fn timestamp_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{FILE_IDENTIFIER, RuntimeStreamTransportStatus, encode_runtime_frame, stream_info};

    #[test]
    fn stream_info_advertises_flatbuffers_and_future_webtransport() {
        let info = stream_info();

        assert_eq!(info.frame.encoding, "flatbuffers");
        assert_eq!(info.frame.file_identifier, FILE_IDENTIFIER);
        assert_eq!(
            info.transports[0].command.as_deref(),
            Some("lightflow serve --port 5174")
        );
        assert_eq!(
            info.transports[0].status,
            RuntimeStreamTransportStatus::Available
        );
        assert_eq!(info.transports[1].name, "webtransport");
        assert_eq!(
            info.transports[1].status,
            RuntimeStreamTransportStatus::Available
        );
        assert_eq!(
            info.transports[1].endpoint,
            "https://127.0.0.1:4433/{run_id}"
        );
        assert_eq!(
            info.transports[1].command.as_deref(),
            Some("lightflow stream serve-webtransport --port 4433")
        );
        let tls = info.transports[1].tls.as_ref().expect("webtransport TLS");
        assert!(tls.self_signed);
        assert_eq!(tls.certificate_hash_option, "serverCertificateHashes");
    }

    #[test]
    fn runtime_frame_uses_lightflow_file_identifier() {
        let bytes = encode_runtime_frame(1, "run-001", "planned", 0, 42, "{}");

        assert!(flatbuffers::buffer_has_identifier(
            &bytes,
            FILE_IDENTIFIER,
            false
        ));
        assert!(bytes.len() > flatbuffers::SIZE_UOFFSET);
    }

    #[test]
    fn run_snapshot_frame_encodes_existing_runtime_run() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let service = crate::api::ApiService::new(
            &root,
            crate::runs::RunStore::new(crate::runs::RuntimeDirs::new(
                root.join("cfg"),
                root.join("state"),
                root.join("cache"),
                root.join("runtime"),
            )),
        );
        service.create_runtime_run(crate::api::RuntimeRunRequest {
            run_id: Some("stream-run".to_owned()),
            workflow_id: "workflow.default".to_owned(),
            inputs: serde_json::Value::Null,
        })?;

        let bytes = super::encode_run_snapshot(&service, "stream-run")?;

        assert!(flatbuffers::buffer_has_identifier(
            &bytes,
            FILE_IDENTIFIER,
            false
        ));
        assert!(bytes.len() > flatbuffers::SIZE_UOFFSET);

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn run_snapshot_frame_rejects_missing_run() {
        let root = unique_temp_root();
        let service = crate::api::ApiService::new(
            &root,
            crate::runs::RunStore::new(crate::runs::RuntimeDirs::new(
                root.join("cfg"),
                root.join("state"),
                root.join("cache"),
                root.join("runtime"),
            )),
        );

        let error = super::encode_run_snapshot(&service, "missing").unwrap_err();

        assert_eq!(error.status_code(), 404);
        let _ = std::fs::remove_dir_all(root);
    }

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-stream-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
