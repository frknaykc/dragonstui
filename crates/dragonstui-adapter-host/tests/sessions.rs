use dragonstui_adapter_host::{
    AdapterInfo, AdapterSession, Capability, PROTOCOL_VERSION, ProtocolMessage, RequestId,
    SessionClose, SessionExit, SessionId, SessionInput, SessionOpen, SessionOpened, SessionOutput,
    SessionResize,
};

#[test]
fn typed_session_messages_round_trip_without_overloading_observability_or_actions() {
    let request = RequestId::new("session-open-1").unwrap();
    let session = SessionId::new("session-1").unwrap();
    let open = ProtocolMessage::SessionOpen(SessionOpen {
        protocol: PROTOCOL_VERSION,
        id: request.clone(),
        capability: Capability::new("fixture.terminal").unwrap(),
        rows: 24,
        columns: 80,
    });
    let opened: ProtocolMessage =
        serde_json::from_str(&serde_json::to_string(&open).unwrap()).unwrap();
    assert_eq!(opened, open);

    let opened = ProtocolMessage::SessionOpened(SessionOpened {
        protocol: PROTOCOL_VERSION,
        id: request,
        session_id: session.clone(),
    });
    let round_trip: ProtocolMessage =
        serde_json::from_str(&serde_json::to_string(&opened).unwrap()).unwrap();
    assert_eq!(round_trip, opened);

    let messages = [
        ProtocolMessage::SessionInput(SessionInput {
            protocol: PROTOCOL_VERSION,
            session_id: session.clone(),
            data: "ping\u{3}".to_owned(),
        }),
        ProtocolMessage::SessionResize(SessionResize {
            protocol: PROTOCOL_VERSION,
            session_id: session.clone(),
            rows: 10,
            columns: 40,
        }),
        ProtocolMessage::SessionOutput(SessionOutput {
            protocol: PROTOCOL_VERSION,
            session_id: session.clone(),
            data: "ready\n".to_owned(),
        }),
        ProtocolMessage::SessionExit(SessionExit {
            protocol: PROTOCOL_VERSION,
            session_id: session.clone(),
            exit_code: Some(7),
        }),
        ProtocolMessage::SessionClose(SessionClose {
            protocol: PROTOCOL_VERSION,
            session_id: session,
        }),
    ];
    for message in messages {
        let round_trip: ProtocolMessage =
            serde_json::from_str(&serde_json::to_string(&message).unwrap()).unwrap();
        assert_eq!(round_trip, message);
    }
}

#[test]
fn invalid_session_ids_are_rejected_before_the_wire_boundary() {
    assert!(SessionId::new("").is_err());
    assert!(SessionId::new("Session").is_err());
}

#[test]
fn session_declarations_are_additive_to_existing_adapter_handshakes() {
    let legacy: AdapterInfo = serde_json::from_value(serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "id": "fixture",
        "version": "1.0.0",
        "capabilities": ["fixture.terminal"],
    }))
    .unwrap();
    assert!(legacy.sessions.is_empty());

    let declared = AdapterSession {
        capability: Capability::new("fixture.terminal").unwrap(),
        label: "Interactive fixture".to_owned(),
        description: Some("Provider-declared terminal session".to_owned()),
    };
    let value = serde_json::to_value(AdapterInfo {
        protocol: PROTOCOL_VERSION,
        id: dragonstui_adapter_host::AdapterId::new("fixture").unwrap(),
        version: "1.0.0".to_owned(),
        capabilities: vec![declared.capability.clone()],
        actions: Vec::new(),
        sessions: vec![declared],
    })
    .unwrap();
    assert_eq!(value["sessions"][0]["capability"], "fixture.terminal");
}
