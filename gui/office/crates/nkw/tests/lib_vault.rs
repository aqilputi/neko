//! The vault end to end (`--demo`): the phrase, the cards, the auto-locks,
//! no audit file under `--demo`, and a screenshot never shows it open.

mod common;

use common::run;

const UNLOCK: &str = r#"inject objects {"vault":{"phrase":"destrancar"}}"#;

#[test]
fn the_phrase_opens_it_with_three_cards_and_focus_lost_locks_it() {
    let st = tempfile::tempdir().unwrap();
    let r = run(
        &format!(
            "wait 100
key b
wait json /ext/objects/library/browsing true 2000
expect json /ext/objects/vault/state \"locked\"
inject objects {{\"vault\":{{\"phrase\":\"errada!\"}}}}
expect json /ext/objects/vault/state \"locked\"
expect json /ext/objects/vault/fails 1
wait 2100
{UNLOCK}
wait json /ext/objects/vault/cards 3 2000
expect json /ext/objects/vault/state \"unlocked\"
expect json /ext/objects/library/mode \"vault\"
expect json /ext/objects/badges/0 \"VaultOpen\"
dump {{dir}}/open.json
inject objects {{\"FocusLost\":true}}
expect json /ext/objects/vault/state \"locked\"
expect json /ext/objects/vault/cards 0
expect json /ext/objects/vault/last_lock \"lock_focus_lost\"
quit
"
        ),
        &["--state-dir", st.path().to_str().unwrap()],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let open = std::fs::read_to_string(r.path("open.json")).unwrap();
    for name in ["id_demo", "known_hosts", ".ssh"] {
        assert!(!open.contains(name), "{name} in the dump");
    }
    assert!(
        !st.path().join("vault.log").exists(),
        "never an audit file under --demo"
    );
}

#[test]
fn the_librarian_takes_the_phrase_on_the_line() {
    let r = run(
        r#"wait 100
key b
wait json /ext/objects/library/browsing true 2000
inject objects {"Command":{"name":"world.goto_object","args":{"name":"cofre"}}}
wait 700
inject objects {"Command":{"name":"world.interact"}}
wait json /ext/objects/vault/state "locked" 500
expect mode CONVERSA
key i
text "destrancar"
key Return
wait json /ext/objects/vault/state "unlocked" 2000
wait 1600
expect mode NAV
quit
"#,
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

/// `shot-obj-vault-open.nks` is impossible by design: every shot locks the
/// vault first (`LockReason::Shot`, the hub's inject before rendering).
#[test]
fn a_shot_never_shows_the_vault_open() {
    let r = run(
        &format!(
            "wait 100
key b
wait json /ext/objects/library/browsing true 2000
{UNLOCK}
wait json /ext/objects/vault/state \"unlocked\" 1000
shot {{dir}}/v.png
expect json /ext/objects/vault/state \"locked\"
expect json /ext/objects/vault/last_lock \"lock_shot\"
expect json /ext/objects/vault/cards 0
quit
"
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    assert!(r.path("v.png").exists());
}
