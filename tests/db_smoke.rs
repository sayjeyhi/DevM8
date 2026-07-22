use devm8::db::Db;

fn scratch_db(name: &str) -> (std::path::PathBuf, Db) {
    let dir = std::env::temp_dir().join(format!("devm8-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let db = Db::open(&path).unwrap();
    (dir, db)
}

#[tokio::test]
async fn pairing_token_and_history_roundtrip() {
    let (dir, db) = scratch_db("pairing");

    let code = db
        .create_pairing_code("alice@example.com", 10)
        .await
        .unwrap();
    let (token, email) = db
        .redeem_pairing_code(&code, Some("test-device"))
        .await
        .unwrap();
    assert_eq!(email, "alice@example.com");

    // Reusing the same code must fail.
    assert!(db.redeem_pairing_code(&code, None).await.is_err());

    let verified = db.verify_token(&token).await.unwrap();
    assert_eq!(verified, Some(("alice@example.com".to_string(), false)));

    db.record_chat_turn(
        Some("PROJ".into()),
        "alice@example.com".into(),
        "cli".into(),
        "sess-1".into(),
        "user",
        "hello".into(),
    )
    .await
    .unwrap();
    db.record_chat_turn(
        Some("PROJ".into()),
        "alice@example.com".into(),
        "cli".into(),
        "sess-1".into(),
        "assistant",
        "hi there".into(),
    )
    .await
    .unwrap();

    let sessions = db
        .list_sessions("alice@example.com", None, 10, 0)
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "sess-1");
    assert_eq!(sessions[0].first_message, "hello");

    let turns = db.get_session_history("sess-1").await.unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].role, "user");
    assert_eq!(turns[1].role, "assistant");

    db.revoke_token(&token).await.unwrap();
    assert_eq!(db.verify_token(&token).await.unwrap(), None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn placeholder_user_migrates_to_real_email() {
    let (dir, db) = scratch_db("migrate");

    let placeholder = db
        .get_or_create_user_for_channel("telegram", "12345", None)
        .await
        .unwrap();
    assert!(placeholder.contains("unmapped.devm8.local"));
    assert_eq!(
        db.resolve_email("telegram", "12345").await.unwrap(),
        Some(placeholder.clone())
    );

    db.rename_or_merge_user_email(&placeholder, "bob@example.com")
        .await
        .unwrap();

    assert_eq!(
        db.resolve_email("telegram", "12345").await.unwrap(),
        Some("bob@example.com".to_string())
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn merging_second_channel_identity_preserves_history() {
    let (dir, db) = scratch_db("merge");

    let tg_placeholder = db
        .get_or_create_user_for_channel("telegram", "111", None)
        .await
        .unwrap();
    db.rename_or_merge_user_email(&tg_placeholder, "carol@example.com")
        .await
        .unwrap();

    db.record_chat_turn(
        None,
        "carol@example.com".into(),
        "telegram".into(),
        "sess-a".into(),
        "user",
        "first channel".into(),
    )
    .await
    .unwrap();

    // Carol's Slack identity shows up first as its own placeholder...
    let slack_placeholder = db
        .get_or_create_user_for_channel("slack", "U999", None)
        .await
        .unwrap();
    assert_ne!(slack_placeholder, "carol@example.com");

    // ...then the admin attaches it to her existing email.
    db.rename_or_merge_user_email(&slack_placeholder, "carol@example.com")
        .await
        .unwrap();

    assert_eq!(
        db.resolve_email("slack", "U999").await.unwrap(),
        Some("carol@example.com".to_string())
    );

    // Original telegram-channel history must survive the merge.
    let sessions = db
        .list_sessions("carol@example.com", None, 10, 0)
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "sess-a");

    let _ = std::fs::remove_dir_all(&dir);
}
