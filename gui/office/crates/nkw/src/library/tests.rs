//! The library through the objects plugin on a pure `World` and in-memory
//! file systems: nothing here reads a real directory.

use std::collections::BTreeMap;
use std::sync::Arc;

use nkw_core::persist::RoomKind;
use nkw_core::{Action, BookId, CatId, Output, RoomId, ScreenId, Station, Target, Time};
use serde_json::Value;

use super::model::MemFs;
use super::*;
use crate::ext::{Ctx, Plugin, Req};
use crate::objects::Objects;
use crate::world::{Opts, World};

struct Rig {
    w: World,
    pids: BTreeMap<ScreenId, u32>,
    visible: Vec<RoomId>,
    now: Time,
    lib_room: RoomId,
}

impl Rig {
    fn new() -> Rig {
        let out = Output {
            width_px: 1280,
            height_px: 720,
            scale: 1.0,
        };
        let mut w = crate::demo::world(
            out,
            Opts {
                reduced_motion: false,
            },
        );
        let (lib_room, _) = w.ensure_room(RoomKind::Library, ROOM_NAME, 0.0);
        let visible = vec![w.current_room_id()];
        Rig {
            w,
            pids: BTreeMap::new(),
            visible,
            now: 1.0,
            lib_room,
        }
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            world: &self.w,
            now: self.now,
            frame: 0,
            reduced_motion: false,
            demo: true,
            scale: 1.0,
            size_px: [1280, 720],
            pids: &self.pids,
            visible_rooms: &self.visible,
            line: None,
        }
    }

    fn frame(&mut self, o: &mut Objects) -> Vec<Req> {
        self.now += 0.05;
        let mut r = o.drain(&self.ctx());
        r.extend(o.tick(&self.ctx()));
        self.visible = vec![self.w.current_room_id()];
        r
    }

    /// Station and camera at a room's door (as `GotoRoom` would).
    fn goto(&mut self, room: RoomId) {
        let i = self.w.rooms.iter().position(|r| r.id == room).unwrap();
        self.w.station = Station::Door(room);
        self.w.camera = nkw_core::layout::room_entry(&self.w.slots()[i]);
    }

    /// Into the library (as `world.library` would, in NAV).
    fn enter(&mut self, o: &mut Objects) -> Vec<Req> {
        self.goto(self.lib_room);
        let mut r = self.frame(o);
        r.extend(self.frame(o));
        r
    }

    /// Before the vault (its room: the reception until the art pass moves
    /// it into the library).
    fn at_vault(&mut self, o: &mut Objects) {
        let room = o.dump()["vault"]["room"].as_u64().unwrap() as u32;
        self.goto(RoomId(room));
        self.frame(o);
    }

    fn cmd(&mut self, o: &mut Objects, name: &str, args: &[(&str, &str)]) -> Vec<Req> {
        let a: BTreeMap<String, String> = args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut r = o.command(name, &a, &self.ctx()).unwrap_or_default();
        r.extend(self.frame(o));
        r
    }
}

fn demo() -> Objects {
    Objects::with(true, false, None)
}

fn lib(o: &Objects) -> Value {
    o.dump()["library"].clone()
}

#[test]
fn nothing_is_read_before_you_arrive_at_the_library() {
    let mut rig = Rig::new();
    let mut o = demo();
    for _ in 0..20 {
        rig.frame(&mut o);
    }
    assert_eq!(o.lib.reads, 0, "no read_dir at rest or at startup");
    assert_eq!(lib(&o)["browsing"], false);
    assert_eq!(o.submode(), None);
    let r = rig.enter(&mut o);
    assert!(o.lib.reads >= 1);
    assert!(
        r.iter().any(|r| matches!(r, Req::GlideTo(_))),
        "to the bookcase"
    );
    let d = lib(&o);
    assert_eq!(d["rel"], "");
    assert_eq!(d["browsing"], true);
    assert_eq!(o.submode(), Some("library"));
    assert_eq!(
        d["entries"],
        serde_json::json!([
            "fotos",
            "livros",
            "musica",
            "projeto",
            "receitas",
            "api-gateway.tar.gz",
            "notas.md",
            "orcamento.csv"
        ])
    );
    assert_eq!(d["vault_count"], 1);
    assert_eq!((d["cols"].as_u64(), d["rows"].as_u64()), (Some(8), Some(1)));
}

#[test]
fn browse_into_a_folder_and_back() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    for _ in 0..3 {
        rig.cmd(&mut o, "world.book_next", &[]);
    }
    assert_eq!(lib(&o)["cursor"], serde_json::json!([0, 3]));
    assert_eq!(lib(&o)["selected"], "projeto");
    rig.cmd(&mut o, "world.book_open", &[]);
    assert_eq!(lib(&o)["rel"], "projeto");
    assert_eq!(lib(&o)["selected"], "docs");
    let crumb = o
        .tags(&rig.ctx())
        .into_iter()
        .find(|t| t.id == 0x11b0_0000)
        .unwrap();
    assert_eq!(crumb.text, "projeto");
    rig.cmd(&mut o, "world.shelf_back", &[]);
    assert_eq!(lib(&o)["rel"], "");
    assert_eq!(
        lib(&o)["selected"],
        "projeto",
        "the cursor on the folder we left"
    );
    rig.cmd(&mut o, "world.library_leave", &[]);
    assert_eq!(lib(&o)["browsing"], false);
    // not browsing: the keys are not ours
    assert!(
        o.command("world.book_next", &BTreeMap::new(), &rig.ctx())
            .is_none()
    );
}

#[test]
fn enter_on_a_file_is_an_intent_and_never_a_spawn_under_demo() {
    let mut rig = Rig::new();
    let mut o = demo();
    // came from acme
    rig.goto(RoomId(1));
    rig.frame(&mut o);
    rig.enter(&mut o);
    for _ in 0..6 {
        rig.cmd(&mut o, "world.book_next", &[]);
    }
    assert_eq!(lib(&o)["selected"], "notas.md");
    let r = rig.cmd(&mut o, "world.book_open", &[]);
    let intent = r.iter().find_map(|r| match r {
        Req::Command { name, args } if name == "world.place_next" => Some(args.clone()),
        _ => None,
    });
    let intent = intent.expect("the placement intent");
    assert_eq!(intent["room"], "1", "back in the room you came from");
    assert!(!r.iter().any(|r| matches!(r, Req::Spawn(_))));
    assert_eq!(lib(&o)["last_open"], "notas.md");
}

#[test]
fn world_open_refuses_vault_paths() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    for p in [
        "~/.ssh/id_demo",
        ".ssh/config",
        "projeto/.env",
        "~/projeto/../.ssh/known_hosts",
        "/srv/tls/servidor.pem",
    ] {
        let r = rig.cmd(&mut o, "world.open", &[("path", p)]);
        assert!(
            r.iter().any(
                |r| matches!(r, Req::Toast { text, .. } if text == "esse caminho fica no cofre")
            ),
            "{p}: {r:?}"
        );
        assert!(
            !r.iter()
                .any(|r| matches!(r, Req::Spawn(_) | Req::Command { .. })),
            "{p}"
        );
    }
    assert_eq!(lib(&o)["last_open"], Value::Null);
    let r = rig.cmd(&mut o, "world.open", &[("path", "~/notas.md")]);
    assert!(
        r.iter()
            .any(|r| matches!(r, Req::Command { name, .. } if name == "world.place_next"))
    );
    assert_eq!(lib(&o)["last_open"], "notas.md");
}

#[test]
fn search_brings_a_table_of_hits_and_never_the_vault() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    rig.cmd(&mut o, "world.library_search", &[("q", "lib")]);
    let d = lib(&o);
    assert_eq!(d["mode"], "search");
    let names: Vec<String> = serde_json::from_value(d["search"]["names"].clone()).unwrap();
    assert!(
        names.contains(&"projeto/src/lib.rs".to_string()),
        "{names:?}"
    );
    assert_eq!(d["search"]["truncated"], false);
    // the librarian's note while she brings them, then the table
    let ctx = rig.ctx();
    assert!(
        o.tags(&ctx)
            .iter()
            .any(|t| t.text.starts_with("a bibliotecária traz"))
    );
    let before = o.pickables(&ctx).len();
    rig.now += BRING_S;
    let after = o.pickables(&rig.ctx()).len();
    assert!(after > before, "the hits are pickable once on the table");
    for q in ["id_demo", "known", "ssh", "env", "config"] {
        rig.cmd(&mut o, "world.library_search", &[("q", q)]);
        let d = lib(&o);
        let s = serde_json::to_string(&d).unwrap();
        assert!(!s.contains(".ssh") && !s.contains(".env"), "{q}: {s}");
    }
    // esc: back to the bookcase
    rig.cmd(&mut o, "world.library_search", &[("q", "praia")]);
    assert_eq!(lib(&o)["mode"], "search");
    rig.cmd(&mut o, "world.library_leave", &[]);
    assert_eq!(lib(&o)["mode"], "shelf");
}

#[test]
fn the_lectern_shows_text_and_refuses_binary() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    for _ in 0..6 {
        rig.cmd(&mut o, "world.book_next", &[]);
    }
    assert_eq!(lib(&o)["selected"], "notas.md");
    rig.cmd(&mut o, "world.book_preview", &[]);
    assert_eq!(lib(&o)["lectern"], true);
    // a folder has no preview
    for _ in 0..3 {
        rig.cmd(&mut o, "world.book_prev", &[]);
    }
    let r = rig.cmd(&mut o, "world.book_preview", &[]);
    assert!(
        r.iter()
            .any(|r| matches!(r, Req::Toast { text, .. } if text == "é uma pasta"))
    );
    // a photo: none
    for _ in 0..3 {
        rig.cmd(&mut o, "world.book_prev", &[]);
    }
    rig.cmd(&mut o, "world.book_open", &[]); // into fotos
    assert_eq!(lib(&o)["rel"], "fotos");
    let r = rig.cmd(&mut o, "world.book_preview", &[]);
    assert!(
        r.iter()
            .any(|r| matches!(r, Req::Toast { text, .. } if text == "sem prévia"))
    );
}

#[test]
fn copy_path_goes_through_world_copy() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    let r = rig.cmd(&mut o, "world.book_copy_path", &[]);
    assert!(r.iter().any(|r| matches!(r,
        Req::Command { name, args } if name == "world.copy" && args["text"] == "~/fotos")));
}

#[test]
fn a_salto_pick_of_a_book_selects_it_and_browses() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    rig.cmd(&mut o, "world.library_leave", &[]);
    let picks = o.pickables(&rig.ctx());
    let books = picks
        .iter()
        .filter(|p| matches!(p.pick, nkw_core::Pick::Book(b) if b.0 < HIT_BASE))
        .count();
    assert_eq!(books, 8);
    let r = o.effect(&nkw_core::Effect::OpenBook(BookId(3)), &rig.ctx());
    assert!(r.iter().any(|r| matches!(r, Req::GlideTo(_))));
    assert_eq!(lib(&o)["browsing"], true);
    assert_eq!(lib(&o)["selected"], "projeto");
    // a board label: its first book
    o.effect(&nkw_core::Effect::OpenBook(BookId(BOARD_BASE)), &rig.ctx());
    assert_eq!(lib(&o)["selected"], "fotos");
}

#[test]
fn the_vault_opens_with_the_phrase_shows_cards_and_locks_on_every_reason() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    rig.at_vault(&mut o);
    let unlock = |rig: &mut Rig, o: &mut Objects| {
        let j = serde_json::json!({"vault": {"phrase": "destrancar"}});
        o.inject(&j, &rig.ctx());
        rig.frame(o);
        assert_eq!(o.dump()["vault"]["state"], "unlocked");
    };
    // FocusLost (the host window) and Shot
    for (j, reason) in [
        (serde_json::json!({"FocusLost": true}), "lock_focus_lost"),
        (serde_json::json!({"Shot": true}), "lock_shot"),
    ] {
        rig.now += 3.0;
        unlock(&mut rig, &mut o);
        assert_eq!(o.dump()["vault"]["cards"], 3);
        assert_eq!(lib(&o)["mode"], "vault");
        o.inject(&j, &rig.ctx());
        let d = o.dump();
        assert_eq!(d["vault"]["state"], "locked");
        assert_eq!(d["vault"]["cards"], 0, "the cards left memory");
        assert_eq!(d["vault"]["last_lock"], reason);
        assert_ne!(lib(&o)["mode"], "vault");
    }
    // EnteredFocus: any FOCO
    rig.now += 3.0;
    unlock(&mut rig, &mut o);
    rig.w
        .action(Action::Focus(Target::CatTerm(CatId(1))), rig.now);
    rig.frame(&mut o);
    assert_eq!(o.dump()["vault"]["last_lock"], "lock_entered_focus");
    assert_eq!(o.dump()["vault"]["cards"], 0);
    rig.w.action(Action::Back, rig.now);
    // Manual
    rig.now += 3.0;
    unlock(&mut rig, &mut o);
    rig.cmd(&mut o, "world.vault_lock", &[]);
    assert_eq!(o.dump()["vault"]["last_lock"], "lock_manual");
    // in the vault: no preview, enter on a card does nothing, copy touches
    rig.now += 3.0;
    unlock(&mut rig, &mut o);
    let r = rig.cmd(&mut o, "world.book_preview", &[]);
    assert!(
        r.iter()
            .any(|r| matches!(r, Req::Toast { text, .. } if text == "no cofre não há prévia"))
    );
    let r = rig.cmd(&mut o, "world.book_open", &[]);
    assert!(
        r.iter()
            .any(|r| matches!(r, Req::Toast { text, .. } if text == "no cofre é só a lista"))
    );
    // three cards on one board
    rig.cmd(&mut o, "world.book_next", &[]);
    assert_eq!(lib(&o)["cursor"], serde_json::json!([0, 1]));
    // LeftRoom: the vault's room is left
    let vault_room = o.dump()["vault"]["room"].as_u64().unwrap() as u32;
    let other = if vault_room == 1 { 2 } else { 1 };
    rig.goto(RoomId(other));
    rig.frame(&mut o);
    assert_eq!(o.dump()["vault"]["last_lock"], "lock_left_room");
}

#[test]
fn the_vault_s_librarian_is_desk_0_of_its_room() {
    let rig = Rig::new();
    let mut o = demo();
    let v = o.world.find(nkw_objects::object::Kind::Vault).unwrap().id;
    let r = o.on_object(v, nkw_core::ObjectAction::Interact, &rig.ctx());
    let room = o.dump()["vault"]["room"].as_u64().unwrap() as u32;
    let cat = rig
        .w
        .rooms
        .iter()
        .find(|r| r.id == RoomId(room))
        .and_then(|r| r.desks.first())
        .and_then(|d| d.cat);
    // in the library she guards it even before she has a desk (R4's id)
    let cat = cat.unwrap_or(crate::cats::office::LIBRARIAN);
    assert_eq!(
        RoomId(room),
        rig.lib_room,
        "the vault stands in the library"
    );
    assert!(
        r.contains(&Req::Action(Action::Talk {
            cat,
            purpose: nkw_core::TalkPurpose::VaultUnlock
        })),
        "{r:?}"
    );
}

/// Plan §5.6, addendum §5.9: a port full of vault entries, every output
/// path the library has — the dump, the palette, tags, pickables, the
/// requests of `world.open`/`enter`/`y`/`e`/search, the world's snapshot —
/// and no vault string anywhere, locked or open.
#[test]
fn vault_never_leaves() {
    const SECRETS: [&str; 6] = [
        "segredo-alfa",
        "chave-bravo",
        "credencial-demo",
        "token-acme",
        "anel-chaveiro",
        "kube-segredo",
    ];
    let mut m = MemFs::new(1);
    m.file(0, "projeto/src/lib.rs", 10)
        .text(0, "notas.md", "# notas")
        .file(0, ".ssh/segredo-alfa", 1)
        .file(0, ".ssh/id_chave-bravo", 1)
        .file(0, ".gnupg/credencial-demo.kbx", 1)
        .file(0, "projeto/.env.token-acme", 1)
        .file(0, ".local/share/keyrings/anel-chaveiro.keyring", 1)
        .file(0, ".kube/kube-segredo", 1)
        .file(0, "projeto/certs/token-acme.pem", 1)
        .file(0, "projeto/id_segredo-alfa", 1);
    let port: scan::Port = Arc::new(m);
    let mut rig = Rig::new();
    let mut o = demo();
    o.lib.use_port(port);
    let mut out: Vec<String> = Vec::new();
    let mut sink = |rs: &[Req]| out.extend(rs.iter().map(|r| format!("{r:?}")));
    sink(&rig.enter(&mut o));
    let tries: Vec<(&str, Vec<(&str, String)>)> = {
        let mut t: Vec<(&str, Vec<(&str, String)>)> = Vec::new();
        for s in SECRETS {
            t.push(("world.library_search", vec![("q", s.to_string())]));
            for p in [
                format!(".ssh/{s}"),
                format!("~/.ssh/{s}"),
                format!("projeto/.env.{s}"),
                format!("projeto/certs/{s}.pem"),
                format!(".kube/{s}"),
                format!("projeto/id_{s}"),
            ] {
                t.push(("world.open", vec![("path", p.clone())]));
                // the phone's Biblioteca "e" (by path)
                t.push(("world.book_edit", vec![("path", p)]));
            }
        }
        t
    };
    for (name, args) in &tries {
        let a: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
        sink(&rig.cmd(&mut o, name, &a));
    }
    let walk = [
        "world.book_next",
        "world.shelf_down",
        "world.book_open",
        "world.book_copy_path",
        "world.book_edit",
        "world.book_preview",
        "world.shelf_back",
        "world.book_open",
        "world.book_next",
        "world.book_open",
    ];
    let snap = |rig: &mut Rig, o: &mut Objects, out: &mut Vec<String>| {
        for n in walk {
            let r = rig.cmd(o, n, &[]);
            // `y` (and `e`) on a card is the one way out the plan allows: the
            // path the user asked to copy (§5.5); everything else is checked
            out.extend(
                r.iter()
                    .filter(|r| {
                        !matches!(r, Req::Command { name, .. }
                            if n == "world.book_copy_path" && name == "world.copy")
                    })
                    .map(|r| format!("{r:?}")),
            );
        }
        let ctx = rig.ctx();
        out.push(o.dump().to_string());
        out.push(format!("{:?}", o.palette_items(&ctx)));
        out.push(format!("{:?}", o.tags(&ctx)));
        out.push(format!("{:?}", o.pickables(&ctx)));
        out.push(format!("{:?}", o.badges(&ctx)));
        out.push(format!("{:?}", o.dialog()));
        out.push(serde_json::to_string(&rig.w.snapshot()).unwrap());
        // wave 4: the status and the phone's apps and pages
        out.push(format!("{:?}", o.status(&ctx)));
        out.push(format!("{:?}", o.lib.phone_hits()));
        out.push(format!("{:?}", o.phone_apps(&ctx)));
        for app in ["musica", "biblioteca", "cofre"] {
            let st = crate::ui::phone::PhoneState {
                app: Some(app.into()),
                ..Default::default()
            };
            let page = o.phone_page(app, &st, &ctx);
            assert!(page.is_some(), "{app}");
            out.push(format!("{page:?}"));
        }
    };
    snap(&mut rig, &mut o, &mut out);
    // and with the vault open, browsing its cards
    rig.at_vault(&mut o);
    let j = serde_json::json!({"vault": {"phrase": "destrancar"}});
    out.extend(o.inject(&j, &rig.ctx()).iter().map(|r| format!("{r:?}")));
    rig.frame(&mut o);
    assert_eq!(o.dump()["vault"]["state"], "unlocked");
    assert!(o.dump()["vault"]["cards"].as_u64().unwrap() >= 4);
    snap(&mut rig, &mut o, &mut out);
    let all = out.join("\n");
    for s in SECRETS {
        assert!(!all.contains(s), "{s} left the vault:\n{all}");
    }
    for s in [".ssh", ".gnupg", ".kube", "keyrings", ".env"] {
        assert!(!all.contains(s), "{s} left the vault");
    }
    // the library did work: the shelf and the search are there
    assert!(all.contains("projeto"));
}

#[test]
fn rates_stay_capped_and_the_slide_is_short() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    rig.cmd(&mut o, "world.book_next", &[]);
    let w = o.lib.wants(rig.now, false);
    assert!(w.contains(&Want::Continuous), "sliding");
    assert!(o.lib.wants(rig.now + SLIDE_S + 0.01, false).is_empty());
    assert!(o.lib.wants(rig.now, true).is_empty(), "reduced motion cuts");
    assert!(o.dump()["rate_wishes"].as_u64().unwrap() <= 4);
}

#[test]
fn the_big_fixture_loads_and_searches_within_the_budget() {
    let mut rig = Rig::new();
    let mut o = demo();
    o.inject(
        &serde_json::json!({"library": {"fixture": "big"}}),
        &rig.ctx(),
    );
    assert_eq!(lib(&o)["source"], "fixture:big");
    rig.enter(&mut o);
    assert!(lib(&o)["rows"].as_u64().unwrap() >= 1);
    rig.cmd(&mut o, "world.library_search", &[("q", "arquivo07")]);
    let d = lib(&o);
    assert!(d["search"]["scanned"].as_u64().unwrap() > 1000);
    assert!(d["timings"]["search_ms"].as_f64().unwrap() < 5000.0);
}

#[test]
fn the_phone_s_biblioteca_searches_and_opens_on_the_fixture() {
    let mut rig = Rig::new();
    let mut o = demo();
    rig.enter(&mut o);
    let st = crate::ui::phone::PhoneState {
        app: Some("biblioteca".into()),
        ..Default::default()
    };
    let page = |o: &Objects, rig: &Rig| o.phone_page("biblioteca", &st, &rig.ctx()).unwrap();
    let p = page(&o, &rig);
    assert!(p.items.is_empty());
    let run = p.search.unwrap().run.unwrap();
    // the overlay sends the line's text as `q`
    rig.cmd(&mut o, &run, &[("q", "notas")]);
    for _ in 0..40 {
        if !page(&o, &rig).items.is_empty() {
            break;
        }
        rig.frame(&mut o);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let p = page(&o, &rig);
    let hit = p
        .items
        .iter()
        .find(|i| i.title == "notas.md")
        .unwrap_or_else(|| panic!("{p:?}"));
    let act = hit.on_enter.clone().unwrap();
    assert!(act.close);
    let args: Vec<(&str, &str)> = act
        .args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let r = rig.cmd(&mut o, &act.name, &args);
    assert!(
        format!("{r:?}").contains("abriria notas.md"),
        "never a spawn under demo: {r:?}"
    );
    let (_, edit) = hit.on_keys.iter().find(|(k, _)| k == "e").unwrap();
    let args: Vec<(&str, &str)> = edit
        .args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let r = rig.cmd(&mut o, &edit.name, &args);
    assert!(format!("{r:?}").contains("abriria no editor"), "{r:?}");
}

/// Where a world point lands on a 60° frame of `aspect` from `pose`: NDC.
fn ndc(pose: nkw_core::Pose, aspect: f32, p: glam::Vec3) -> glam::Vec3 {
    let proj =
        glam::camera::rh::proj::opengl::perspective(nkw_core::layout::FOV_Y, aspect, 0.05, 100.0);
    (proj * pose.view()).project_point3(p)
}

#[test]
fn the_stand_frames_the_board_and_its_whole_unit() {
    use nkw_core::strip::{RoomSlot, Side};
    assert!((layout::STAND_M - 2.4).abs() < 1e-6);
    let slot = RoomSlot {
        x0: 10.0,
        len: 6.0,
        side: Side::North,
        door_x: 13.0,
    };
    let f = layout::floors();
    for root in [0, 1] {
        let r = layout::run(&slot, root).unwrap();
        let w = r.units() as f32 * layout::UNIT_W;
        for (px_w, px_h) in [(1280.0f32, 720.0f32), (1920.0, 1080.0)] {
            let aspect = px_w / px_h;
            for (board, &top) in f.iter().enumerate() {
                // a book on the second unit, standing on `board`
                let x = (r.len - w) / 2.0 + layout::UNIT_W * 1.5;
                let book = r.at(x, top + 0.12) + r.normal * 0.08;
                let st = stand_at(&r, book);
                assert!(st.pitch.abs() <= 0.5 + 1e-6, "pitch {}", st.pitch);
                let flat = (st.pos - r.centre).dot(r.normal);
                assert!((flat - layout::STAND_M).abs() < 1e-3, "{flat}");
                // the board's centre at 45–55 % of the frame's height
                let c = ndc(st, aspect, r.at(x, top + 0.15) + r.normal * 0.1);
                let frac = (1.0 - c.y) / 2.0;
                assert!(
                    (0.45..=0.55).contains(&frac),
                    "root {root} board {board} {px_w}: {frac}"
                );
                // the drawn unit (of the two stacked) the board is on, and
                // half a neighbour on each side, all inside the frame
                let (y0, y1) = if board < 3 {
                    (layout::UNIT_H, 2.0 * layout::UNIT_H)
                } else {
                    (0.0, layout::UNIT_H)
                };
                let front = r.normal * (layout::CASE_DEPTH / 2.0);
                for xx in [x - layout::UNIT_W, x + layout::UNIT_W] {
                    for yy in [y0, y1] {
                        let p = ndc(st, aspect, r.at(xx, yy) + front);
                        assert!(
                            p.x.abs() <= 1.0 && p.y.abs() <= 1.0 && p.z < 1.0,
                            "root {root} board {board} {px_w}: corner ({xx}, {yy}) at {p}"
                        );
                    }
                }
            }
        }
    }
}
