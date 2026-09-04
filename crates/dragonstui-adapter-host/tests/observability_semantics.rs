use dragonstui_adapter_host::{
    Event, Observation, ObservationKind, ObservationSeverity, ObservationStatus, PROTOCOL_VERSION,
    ProtocolMessage,
};
use serde_json::{Number, json};

#[test]
fn old_format_event_decodes_as_unclassified_without_mutating_its_payload() {
    let encoded = r#"{"type":"event","protocol":1,"stream":"legacy","kind":"snapshot","payload":{"sequence":1}}"#;

    let ProtocolMessage::Event(event) = serde_json::from_str::<ProtocolMessage>(encoded).unwrap()
    else {
        panic!("old-format event must decode as an event");
    };

    assert_eq!(event.observation, None);
    assert_eq!(event.stream, "legacy");
    assert_eq!(event.kind, "snapshot");
    assert_eq!(event.payload, json!({"sequence": 1}));
}

#[test]
fn semantic_observations_round_trip_without_changing_generic_payload() {
    let observations = [
        Observation::Log {
            text: "completed ✓".to_owned(),
            severity: Some(ObservationSeverity::Warning),
            timestamp_millis: Some(42),
        },
        Observation::Metric {
            name: "queue_depth".to_owned(),
            value: Number::from(7),
            unit: Some("items".to_owned()),
            timestamp_millis: None,
        },
        Observation::Status {
            entity: "worker-a".to_owned(),
            check: "ready".to_owned(),
            status: ObservationStatus::Ok,
            timestamp_millis: Some(43),
        },
        Observation::Event {
            title: "checkpoint".to_owned(),
            detail: Some("ordered detail".to_owned()),
            timestamp_millis: None,
        },
        Observation::Error {
            message: "işlem başarısız".to_owned(),
            signature: Some("stable-error".to_owned()),
            stack: vec!["first frame".to_owned(), "second frame".to_owned()],
            timestamp_millis: Some(44),
        },
    ];

    for observation in observations {
        let kind = observation.kind();
        let message = ProtocolMessage::Event(Event {
            protocol: PROTOCOL_VERSION,
            stream: "semantic".to_owned(),
            kind: "fixture".to_owned(),
            observation: Some(observation),
            payload: json!({"opaque": [1, 2, 3]}),
        });

        let encoded = serde_json::to_string(&message).unwrap();
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&encoded).unwrap(),
            message
        );
        assert!(matches!(
            message,
            ProtocolMessage::Event(Event {
                observation: Some(ref decoded),
                ..
            }) if decoded.kind() == kind
        ));
    }
}

#[test]
fn invalid_or_unknown_semantic_observations_are_rejected_without_panicking() {
    let unknown = r#"{"type":"event","protocol":1,"stream":"semantic","kind":"fixture","observation":{"type":"future","title":"unknown"},"payload":{}}"#;
    let metric_without_a_number = r#"{"type":"event","protocol":1,"stream":"semantic","kind":"fixture","observation":{"type":"metric","name":"queue_depth","value":null},"payload":{}}"#;

    assert!(serde_json::from_str::<ProtocolMessage>(unknown).is_err());
    assert!(serde_json::from_str::<ProtocolMessage>(metric_without_a_number).is_err());
}

#[test]
fn additive_unknown_observation_fields_follow_the_existing_serde_tolerant_policy() {
    let encoded = r#"{"type":"event","protocol":1,"stream":"semantic","kind":"fixture","observation":{"type":"log","text":"hello","future_field":"ignored"},"payload":{"opaque":true}}"#;

    let ProtocolMessage::Event(event) = serde_json::from_str::<ProtocolMessage>(encoded).unwrap()
    else {
        panic!("semantic event must decode");
    };

    assert_eq!(event.payload, json!({"opaque": true}));
    assert_eq!(
        event.observation.as_ref().map(Observation::kind),
        Some(ObservationKind::Log)
    );
}
