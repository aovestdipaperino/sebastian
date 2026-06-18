//! Unit tests for the sequenceDiagram parser (`sequence::parse`): actor
//! registration and message extraction.

use sebastian::sequence::parse;

#[test]
fn parses_actors_and_a_message() {
    let db = parse("sequenceDiagram\nAlice->>Bob: Hello\n").expect("parse");
    assert!(db.actors.contains_key("Alice"));
    assert!(db.actors.contains_key("Bob"));
    assert_eq!(db.messages.len(), 1);
    assert_eq!(db.messages[0].from, "Alice");
    assert_eq!(db.messages[0].to, "Bob");
    assert_eq!(db.messages[0].message, "Hello");
}

#[test]
fn parses_multiple_messages_in_order() {
    let db = parse("sequenceDiagram\nA->>B: one\nB->>A: two\n").expect("parse");
    assert_eq!(db.messages.len(), 2);
    assert_eq!(db.messages[0].message, "one");
    assert_eq!(db.messages[1].message, "two");
    assert_eq!(db.messages[1].from, "B");
    assert_eq!(db.messages[1].to, "A");
}

#[test]
fn explicit_participants_register_actors_in_order() {
    let db = parse("sequenceDiagram\nparticipant Bob\nparticipant Alice\nBob->>Alice: hi\n")
        .expect("parse");
    let keys: Vec<&String> = db.actors.keys().collect();
    // Declared order is preserved (Bob before Alice).
    assert_eq!(keys, vec!["Bob", "Alice"]);
}

#[test]
fn message_text_is_trimmed() {
    let db = parse("sequenceDiagram\nA->>B:   spaced   \n").expect("parse");
    assert_eq!(db.messages[0].message, "spaced");
}
