use heiwa_graph::{new_node, GraphStore};
use heiwa_protocol::{
    CalendarProposal, GraphEdge, GraphEntityKind, ProposalStatus, Sensitivity, SourceSpan,
};

#[test]
fn migrates_and_roundtrips_node_edge_span_proposal() {
    let store = GraphStore::open_in_memory().expect("open");
    assert_eq!(store.schema_version().unwrap(), "1");

    let mut person = new_node(GraphEntityKind::Person, "Ada");
    person.sensitivity = Sensitivity::Private;
    person.source_system = Some("test".into());
    person.external_id = Some("ada-1".into());
    store.upsert_node(&person).unwrap();

    let mut msg = new_node(GraphEntityKind::Message, "Hello");
    msg.properties = serde_json::json!({"body": "Let's meet tomorrow"});
    store.upsert_node(&msg).unwrap();

    store
        .insert_edge(&GraphEdge {
            id: "e1".into(),
            from_id: msg.id.clone(),
            to_id: person.id.clone(),
            kind: "participant".into(),
            created_at_unix: msg.created_at_unix,
            properties: serde_json::json!({}),
        })
        .unwrap();

    store
        .attach_source_span(&msg.id, &SourceSpan::message_id("mid-99"))
        .unwrap();

    store
        .upsert_calendar_proposal(&CalendarProposal {
            id: "p1".into(),
            title: "Meet Ada".into(),
            starts_at_unix: 1_700_000_000,
            ends_at_unix: 1_700_003_600,
            confidence: 0.82,
            sources: vec![SourceSpan::message_id("mid-99")],
            attendees: vec!["Ada".into()],
            notes: Some("from message".into()),
            status: ProposalStatus::PendingApproval,
        })
        .unwrap();

    let loaded = store.get_node(&person.id).unwrap().expect("person");
    assert_eq!(loaded.title.as_deref(), Some("Ada"));
    assert_eq!(store.count_nodes().unwrap(), 2);

    let summary = store.doctor_summary().unwrap();
    assert!(summary.contains("schema=1"));
    assert!(summary.contains("nodes=2"));
    assert!(summary.contains("calendar_proposals=1"));
}
