use dragonstui_adapter_host::{
    AdapterId, Capability, ErrorMessage, Event, PROTOCOL_VERSION, ProtocolMessage, Request,
    RequestId, Response,
};
use serde_json::json;

#[test]
fn request_round_trips_with_protocol_and_correlation() {
    let message = ProtocolMessage::Request(Request {
        protocol: PROTOCOL_VERSION,
        id: RequestId::new("req-42").unwrap(),
        operation: Capability::new("containers.list").unwrap(),
        payload: json!({"limit": 5}),
    });

    let encoded = serde_json::to_string(&message).unwrap();
    assert_eq!(
        encoded,
        r#"{"type":"request","protocol":1,"id":"req-42","operation":"containers.list","payload":{"limit":5}}"#
    );
    assert_eq!(
        serde_json::from_str::<ProtocolMessage>(&encoded).unwrap(),
        message
    );
}

#[test]
fn response_error_and_event_keep_typed_envelope_fields() {
    let response = ProtocolMessage::Response(Response {
        protocol: PROTOCOL_VERSION,
        id: RequestId::new("req-9").unwrap(),
        payload: json!(["ok"]),
    });
    let error = ProtocolMessage::Error(ErrorMessage {
        protocol: PROTOCOL_VERSION,
        id: Some(RequestId::new("req-9").unwrap()),
        code: "permission_denied".to_owned(),
        message: "not allowed".to_owned(),
    });
    let event = ProtocolMessage::Event(Event {
        protocol: PROTOCOL_VERSION,
        stream: "logs".to_owned(),
        kind: "entry".to_owned(),
        payload: json!({"line": "hello"}),
    });

    for message in [response, error, event] {
        let encoded = serde_json::to_string(&message).unwrap();
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&encoded).unwrap(),
            message
        );
    }
}

#[test]
fn malformed_unknown_and_invalid_identifiers_are_rejected() {
    assert!(serde_json::from_str::<ProtocolMessage>("not-json").is_err());
    assert!(
        serde_json::from_str::<ProtocolMessage>(r#"{"type":"surprise","protocol":1}"#).is_err()
    );
    assert!(AdapterId::new("../docker").is_err());
    assert!(Capability::new("containers..logs").is_err());
    assert!(
        serde_json::from_str::<ProtocolMessage>(
            r#"{"type":"request","protocol":1,"id":"req-1","operation":"../unsafe","payload":{}}"#
        )
        .is_err()
    );
}
