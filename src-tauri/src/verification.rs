//! End-to-end cross-phase verification suite for Pipan v0.2.
//!
//! Exercises SQLite schema migrations (v1..v4), session lifecycle + node graph
//! coordinate persistence & cascade deletion, usage analytics + GitHub PR view
//! persistence, and extension/skill/attachment validation in-memory.

use std::collections::HashMap;

use rusqlite::params;

use crate::db::{Database, GithubView, NodePosition};
use crate::extensions::parse_and_validate_manifest;
use crate::files::{encode_base64, image_mime_for_extension, read_file_attachment_sync};
use crate::skills::parse_skill_markdown;
use crate::storage::{SaveSessionMetadataInput, SessionListQuery, now_unix_secs};
use crate::usage::UsageLedgerRecord;

#[test]
fn e2e_sqlite_session_and_node_graph_lifecycle() {
    let db = Database::open_in_memory().expect("open in-memory database");

    // Verify v1..v4 schema readiness (`PRAGMA user_version == 4`).
    {
        let conn = db.lock();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("query user_version");
        assert_eq!(version, 4, "expected schema user_version 4");
    }

    // Create two sessions with metadata, groups, tags, and cost metrics.
    let s1 = db
        .save_session_metadata(&SaveSessionMetadataInput {
            id: "sess-alpha".to_string(),
            name: Some("Alpha Feature".to_string()),
            status: Some("running".to_string()),
            mode: Some("directory".to_string()),
            cwd: Some("/workspace/pipan".to_string()),
            group_name: Some("Core".to_string()),
            branch: Some("feat/alpha".to_string()),
            message_count: Some(12),
            total_cost_usd: Some(0.45),
            tags: Some(vec!["rust".to_string(), "graph".to_string()]),
            ..Default::default()
        })
        .expect("save sess-alpha");
    assert_eq!(s1.name, "Alpha Feature");
    assert_eq!(s1.group_name.as_deref(), Some("Core"));
    assert_eq!(s1.tags, vec!["graph".to_string(), "rust".to_string()]);

    let s2 = db
        .save_session_metadata(&SaveSessionMetadataInput {
            id: "sess-beta".to_string(),
            name: Some("Beta Bugfix".to_string()),
            status: Some("stopped".to_string()),
            mode: Some("worktree".to_string()),
            cwd: Some("/workspace/pipan-wt".to_string()),
            group_name: Some("Maintenance".to_string()),
            branch: Some("fix/beta".to_string()),
            message_count: Some(4),
            total_cost_usd: Some(1.25),
            tags: Some(vec!["svelte".to_string()]),
            ..Default::default()
        })
        .expect("save sess-beta");
    assert_eq!(s2.id, "sess-beta");

    // Rename a session and verify persistence.
    let renamed = db
        .rename_session("sess-alpha", "Alpha Layout Upgrade")
        .expect("rename sess-alpha");
    assert_eq!(renamed.name, "Alpha Layout Upgrade");
    assert_eq!(renamed.title, "Alpha Layout Upgrade");

    // Search by tag and by renamed title, and sort by cost descending.
    let tag_matches = db
        .list_saved_sessions(&SessionListQuery {
            search: Some("graph".to_string()),
            filter: Some("active".to_string()),
            ..Default::default()
        })
        .expect("search by tag");
    assert_eq!(tag_matches.len(), 1);
    assert_eq!(tag_matches[0].id, "sess-alpha");

    let sorted_by_cost = db
        .list_saved_sessions(&SessionListQuery {
            filter: Some("active".to_string()),
            sort_by: Some("total_cost_usd".to_string()),
            sort_order: Some("desc".to_string()),
            ..Default::default()
        })
        .expect("sort by cost");
    assert_eq!(sorted_by_cost.len(), 2);
    assert_eq!(sorted_by_cost[0].id, "sess-beta");
    assert_eq!(sorted_by_cost[1].id, "sess-alpha");

    // Archive sess-beta and verify active vs archived filtering.
    let archived = db
        .archive_session("sess-beta", true)
        .expect("archive sess-beta");
    assert!(archived.archived);

    let active_only = db
        .list_saved_sessions(&SessionListQuery {
            filter: Some("active".to_string()),
            ..Default::default()
        })
        .expect("list active");
    assert_eq!(active_only.len(), 1);
    assert_eq!(active_only[0].id, "sess-alpha");

    let archived_only = db
        .list_saved_sessions(&SessionListQuery {
            filter: Some("archived".to_string()),
            ..Default::default()
        })
        .expect("list archived");
    assert_eq!(archived_only.len(), 1);
    assert_eq!(archived_only[0].id, "sess-beta");

    // Save and load both TB and LR node graph coordinates for sess-alpha.
    db.set_graph_direction("sess-alpha", "TB")
        .expect("set direction TB");
    assert_eq!(
        db.graph_direction("sess-alpha").expect("get direction"),
        Some("TB".to_string())
    );

    let mut tb_positions = HashMap::new();
    tb_positions.insert(
        "node-1".to_string(),
        NodePosition {
            x: 120.0,
            y: 80.0,
            pinned: true,
        },
    );
    db.save_node_positions("sess-alpha", "TB", &tb_positions)
        .expect("save TB positions");

    db.set_graph_direction("sess-alpha", "LR")
        .expect("set direction LR");
    let mut lr_positions = HashMap::new();
    lr_positions.insert(
        "node-1".to_string(),
        NodePosition {
            x: 340.0,
            y: 210.0,
            pinned: true,
        },
    );
    db.save_node_positions("sess-alpha", "LR", &lr_positions)
        .expect("save LR positions");

    let loaded_tb = db
        .load_node_positions("sess-alpha", "TB")
        .expect("load TB positions");
    let loaded_lr = db
        .load_node_positions("sess-alpha", "LR")
        .expect("load LR positions");
    assert_eq!(
        loaded_tb.get("node-1").map(|p| (p.x, p.y)),
        Some((120.0, 80.0))
    );
    assert_eq!(
        loaded_lr.get("node-1").map(|p| (p.x, p.y)),
        Some((340.0, 210.0))
    );

    // Delete sess-alpha and verify cascade deletion of session_tags and node_positions.
    let deleted = db
        .delete_session_and_artifacts("sess-alpha")
        .expect("delete sess-alpha");
    assert!(deleted);
    assert!(
        db.get_session("sess-alpha")
            .expect("query deleted session")
            .is_none()
    );

    let conn = db.lock();
    let remaining_tags: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM session_tags WHERE session_id = ?1",
            params!["sess-alpha"],
            |r| r.get(0),
        )
        .expect("count remaining tags");
    let remaining_positions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM node_positions WHERE session_id = ?1",
            params!["sess-alpha"],
            |r| r.get(0),
        )
        .expect("count remaining node_positions");
    assert_eq!(remaining_tags, 0, "session_tags should cascade on delete");
    assert_eq!(
        remaining_positions, 0,
        "node_positions should cascade on delete"
    );
}

#[test]
fn e2e_usage_analytics_and_github_views_persistence() {
    let db = Database::open_in_memory().expect("open in-memory database");
    let now = now_unix_secs();

    let records = vec![
        UsageLedgerRecord {
            session_id: "sess-analytics-1".to_string(),
            message_id: "msg-1".to_string(),
            timestamp: now - 60,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            input: 1_500,
            output: 500,
            cache_read: 200,
            cache_write: 100,
            cost_usd: 0.025,
            cwd: Some("/workspace/pipan".to_string()),
        },
        UsageLedgerRecord {
            session_id: "sess-analytics-2".to_string(),
            message_id: "msg-2".to_string(),
            timestamp: now - 30,
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            input: 800,
            output: 400,
            cache_read: 0,
            cache_write: 0,
            cost_usd: 0.015,
            cwd: Some("/workspace/pipan".to_string()),
        },
    ];

    let inserted = db
        .insert_usage_records(&records)
        .expect("insert usage records");
    assert_eq!(inserted, 2);

    let analytics = db
        .compute_full_usage_analytics(1, inserted)
        .expect("compute usage analytics");
    assert_eq!(analytics.totals.total_messages, 2);
    assert_eq!(analytics.totals.total_sessions, 2);
    assert_eq!(analytics.totals.input_tokens, 2_300);
    assert_eq!(analytics.totals.output_tokens, 900);
    assert!((analytics.totals.total_spend_usd - 0.040).abs() < 1e-6);
    assert!(!analytics.day.hourly_today.is_empty());
    assert!(!analytics.week.daily_this_week.is_empty());
    assert!(!analytics.month.daily_this_month.is_empty());
    assert!(!analytics.year.monthly_this_year.is_empty());
    assert_eq!(analytics.by_model.len(), 2);

    // Save, list, and delete custom GitHub PR views.
    let saved_view = db
        .save_github_view(&GithubView {
            id: "needs-review".to_string(),
            name: "Needs My Review".to_string(),
            query: "is:open review-requested:@me".to_string(),
            sort_order: 1,
        })
        .expect("save github view");
    assert_eq!(saved_view.name, "Needs My Review");

    let views = db.list_github_views().expect("list github views");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, "needs-review");
    assert_eq!(views[0].query, "is:open review-requested:@me");

    let removed = db
        .delete_github_view("needs-review")
        .expect("delete github view");
    assert!(removed);
    assert!(db.list_github_views().expect("list empty views").is_empty());
}

#[test]
fn e2e_workspace_extension_skill_and_attachment_validation() {
    // 1. Extension manifest validation.
    let raw_manifest = r#"{
        "id": "pipan.custom-inspector",
        "name": "Custom Inspector",
        "version": "0.2.0",
        "description": "Verification test extension",
        "contributes": {
            "panels": [{ "id": "inspector", "title": "Inspector", "icon": "search" }],
            "slashCommands": [{ "name": "/inspect", "description": "Run inspector" }]
        }
    }"#;
    let manifest = parse_and_validate_manifest(raw_manifest).expect("valid extension manifest");
    assert_eq!(manifest.id, "pipan.custom-inspector");
    assert_eq!(
        manifest.contributes.panels.as_ref().expect("panels").len(),
        1
    );
    assert_eq!(
        manifest
            .contributes
            .slash_commands
            .as_ref()
            .expect("slash commands")[0]
            .name,
        "inspect"
    );

    // 2. Skill markdown frontmatter parsing.
    let skill_md = "---\nname: rust-verifier\ndescription: Runs cargo checks and tests\n---\n# Body\nVerify all crates.";
    let (skill_name, skill_desc) = parse_skill_markdown(skill_md, "fallback-skill");
    assert_eq!(skill_name, "rust-verifier");
    assert_eq!(skill_desc, "Runs cargo checks and tests");

    // 3. Base64 & attachment payload encoding.
    let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(image_mime_for_extension("png"), Some("image/png"));
    let encoded = encode_base64(png_bytes);
    assert_eq!(encoded, "iVBORw0KGgo=");

    let temp_dir = std::env::temp_dir().join(format!("pipan_verify_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let img_path = temp_dir.join("sample.png");
    std::fs::write(&img_path, png_bytes).expect("write sample.png");

    let payload =
        read_file_attachment_sync(&img_path, temp_dir.to_str()).expect("read image attachment");
    assert_eq!(payload.kind, "image");
    assert_eq!(payload.mime_type, "image/png");
    assert!(payload.inside_workspace);
    assert_eq!(payload.data.as_deref(), Some("iVBORw0KGgo="));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
