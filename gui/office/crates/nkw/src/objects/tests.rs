//! The plugin end to end on a pure `World` and a fake system port: nothing
//! here reaches a real player, volume or PipeWire.

use std::sync::Mutex;

use nkw_core::{Output, Station};
use nkw_sys::{AudioEvent, StreamId, StreamInfo};

use super::*;
use crate::world::{Opts, World};

#[derive(Default)]
struct Shared {
    events: Mutex<Vec<SysEvent>>,
    media: Mutex<Vec<&'static str>>,
    sink: Mutex<Vec<f32>>,
    vols: Mutex<Vec<(StreamId, [f32; 2])>>,
    shut: Mutex<bool>,
}

struct Fake(Arc<Shared>);

impl MediaControl for Fake {
    fn play_pause(&self) {
        self.0.media.lock().unwrap().push("play_pause");
    }
    fn next(&self) {
        self.0.media.lock().unwrap().push("next");
    }
    fn previous(&self) {
        self.0.media.lock().unwrap().push("previous");
    }
    fn set_volume(&self, _: f32) {
        self.0.media.lock().unwrap().push("set_volume");
    }
}

impl StreamVolume for Fake {
    fn set_channel_volumes(&self, id: StreamId, lr: [f32; 2]) {
        self.0.vols.lock().unwrap().push((id, lr));
    }
    fn set_default_sink_volume(&self, v: f32) {
        self.0.sink.lock().unwrap().push(v);
    }
}

impl Port for Fake {
    fn events(&self) -> Vec<SysEvent> {
        std::mem::take(&mut *self.0.events.lock().unwrap())
    }
    fn media(&self) -> &dyn MediaControl {
        self
    }
    fn audio(&self) -> &dyn StreamVolume {
        self
    }
    fn shutdown(self: Box<Self>) {
        *self.0.shut.lock().unwrap() = true;
    }
}

fn demo_world() -> World {
    let out = Output {
        width_px: 1280,
        height_px: 720,
        scale: 1.0,
    };
    crate::demo::world(
        out,
        Opts {
            reduced_motion: false,
        },
    )
}

struct Rig {
    w: World,
    pids: BTreeMap<ScreenId, u32>,
    visible: Vec<RoomId>,
    now: Time,
    reduced: bool,
}

impl Rig {
    fn new() -> Rig {
        let w = demo_world();
        let visible = vec![w.current_room_id()];
        Rig {
            w,
            pids: BTreeMap::new(),
            visible,
            now: 1.0,
            reduced: false,
        }
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            world: &self.w,
            now: self.now,
            frame: 0,
            reduced_motion: self.reduced,
            demo: true,
            scale: 1.0,
            size_px: [1280, 720],
            pids: &self.pids,
            visible_rooms: &self.visible,
            line: None,
        }
    }

    /// drain + tick, as the hub does each frame, at `now`
    fn frame(&mut self, o: &mut Objects, now: Time) -> Vec<Req> {
        self.now = now;
        let mut r = o.drain(&self.ctx());
        r.extend(o.tick(&self.ctx()));
        let opens = o.opens_rooms(self.w.current_room_id(), &self.ctx());
        self.visible = self.w.visible_rooms(&opens);
        r
    }
}

fn faked(demo: bool) -> (Objects, Arc<Shared>) {
    let s = Arc::new(Shared::default());
    (
        Objects::with(demo, true, Some(Box::new(Fake(s.clone())))),
        s,
    )
}

#[test]
fn the_demo_world_is_placed_in_the_reception_with_a_door_per_room() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let d = o.dump();
    let objs = d["objects"].as_array().unwrap();
    // 14 demo objects + a clock in each of the 2 other rooms + 3 doors
    assert_eq!(objs.len(), 14 + 2 + 3);
    assert!(
        objs.iter()
            .filter(|x| x["kind"] != "Door" && x["kind"] != "WallClock")
            .all(|x| x["room"] == 0)
    );
    assert_eq!(d["doors"].as_array().unwrap().len(), 3);
    assert_eq!(d["sys"]["mpris"], "off");
    assert_eq!(d["demo"], true);
    // the fixture (minus its clock) landed: battery 73 %, the plant 3 bars
    assert_eq!(
        objs.iter().find(|x| x["kind"] == "Battery").unwrap()["data_kind"],
        "Power"
    );
    // every clock shows the demo's time
    for c in objs.iter().filter(|x| x["kind"] == "WallClock") {
        assert_eq!(c["data_kind"], "Clock", "{c}");
    }
    // calm: no badge until an event says so
    assert_eq!(d["badges"], serde_json::json!([]));
}

#[test]
fn battery_at_12_percent_badges_and_meows() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let before = o.sound.played.count;
    let j = serde_json::json!({"Power":{"present":true,"pct":12,"charging":false,"time_to_empty_s":1800}});
    let r = o.inject(&j, &rig.ctx());
    assert!(r.contains(&redraw()));
    assert_eq!(o.dump()["badges"][0], serde_json::json!({"BatteryLow": 12}));
    assert_eq!(o.badges(&rig.ctx())[0], Badge::BatteryLow(12));
    assert!(o.sound.played.last.contains(&Cue::Meow));
    assert!(o.sound.played.count > before);
}

#[test]
fn a_closed_door_blocks_hides_the_rooms_and_opens_again() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let r0 = RoomId(0);
    assert!(o.obstacles(r0, &rig.ctx()).iter().all(|a| {
        // only furniture: nothing in the door's gap
        let (door, _) = rig.w.room_door(r0).unwrap();
        !a.contains(door)
    }));
    assert_eq!(o.opens_rooms(r0, &rig.ctx()), [RoomId(1), RoomId(2)]);
    o.inject(
        &serde_json::json!({"Door":{"id":0,"toggle":true}}),
        &rig.ctx(),
    );
    rig.frame(&mut o, 1.1);
    assert_eq!(
        o.wishes(&rig.ctx())
            .iter()
            .filter(|w| w.want == Want::Continuous)
            .count(),
        1,
        "the swing animates"
    );
    rig.frame(&mut o, 2.0);
    let d = o.dump();
    assert_eq!(d["doors"][0]["passable"], false);
    assert_eq!(d["visible_rooms"], serde_json::json!([0]));
    let (door, _) = rig.w.room_door(r0).unwrap();
    assert!(o.obstacles(r0, &rig.ctx()).iter().any(|a| a.contains(door)));
    assert!(o.opens_rooms(r0, &rig.ctx()).is_empty());
    assert!(o.sound.played.last.contains(&Cue::DoorClose));
    // reopened: the rooms are back the same frame
    o.inject(
        &serde_json::json!({"Door":{"id":0,"toggle":true}}),
        &rig.ctx(),
    );
    assert_eq!(o.opens_rooms(r0, &rig.ctx()), [RoomId(1), RoomId(2)]);
    assert!(o.sound.played.last.contains(&Cue::DoorOpen));
}

#[test]
fn at_rest_in_the_demo_nothing_wants_frames_but_the_auto_close_timer() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    // the turntable plays, but it is out of view from the desk
    let w = o.wishes(&rig.ctx());
    assert!(
        w.iter()
            .all(|w| matches!(w.want, Want::At(_)) && w.pass == nkw_core::Pass::Compose),
        "{w:?}"
    );
    assert_eq!(o.dump()["rate_wishes"], 0);
    // the doors close by themselves after 8 s, then nothing at all
    rig.frame(&mut o, 9.5);
    rig.frame(&mut o, 11.0);
    assert!(
        o.wishes(&rig.ctx()).is_empty(),
        "{:?}",
        o.wishes(&rig.ctx())
    );
    assert_eq!(o.dump()["doors"][1]["passable"], false);
}

#[test]
fn facing_the_turntable_it_spins_at_12_hz_and_reads() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let stand = o.world.find(Kind::Turntable).unwrap().stand;
    rig.w.station = Station::Free(stand);
    rig.w.camera = stand;
    rig.frame(&mut o, 2.0);
    let rates: Vec<_> = o
        .wishes(&rig.ctx())
        .into_iter()
        .filter(|w| matches!(w.want, Want::Rate(_)))
        .collect();
    assert_eq!(rates, [Wish::world(Want::Rate(12.0))]);
    let tags = o.tags(&rig.ctx());
    assert!(
        tags.iter().any(|t| t.text == "Lo-fi for cats — acme radio"),
        "{tags:?}"
    );
    // reduced motion: still
    rig.reduced = true;
    rig.frame(&mut o, 3.0);
    assert!(
        o.wishes(&rig.ctx())
            .iter()
            .all(|w| !matches!(w.want, Want::Rate(_)))
    );
}

#[test]
fn the_clock_sets_the_day_band() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let j = serde_json::json!({"Clock":{"unix_s":0,"hour":20,"minute":30,"band":6}});
    let r = o.inject(&j, &rig.ctx());
    assert!(r.contains(&Req::Command {
        name: "world.band".into(),
        args: BTreeMap::from([("band".into(), "6".into())]),
    }));
    rig.w.band = 6;
    let r = o.inject(&j, &rig.ctx());
    assert!(
        !r.iter().any(|x| matches!(x, Req::Command { .. })),
        "same band: nothing"
    );
    // the readout follows
    let id = o.world.find(Kind::WallClock).unwrap().id;
    assert_eq!(
        map::readout(Kind::WallClock, o.world.data.get(&id)).as_deref(),
        Some("20:30")
    );
}

#[test]
fn media_and_volume_commands_reach_the_port() {
    let mut rig = Rig::new();
    let (mut o, s) = faked(true);
    rig.frame(&mut o, 1.0);
    let none = BTreeMap::new();
    for (cmd, want) in [
        ("media.play_pause", "play_pause"),
        ("media.next", "next"),
        ("media.prev", "previous"),
    ] {
        assert!(o.command(cmd, &none, &rig.ctx()).is_some());
        assert_eq!(s.media.lock().unwrap().last().copied(), Some(want));
    }
    // the knob: 0.6 → 0.65 → 0.6
    o.command("volume.up", &none, &rig.ctx()).unwrap();
    o.command("volume.down", &none, &rig.ctx()).unwrap();
    assert_eq!(*s.sink.lock().unwrap(), [0.65, 0.6]);
    // mute: to 0 and the needle up; again: back
    o.command("volume.mute", &none, &rig.ctx()).unwrap();
    let tt = o.world.find(Kind::Turntable).unwrap().id;
    assert!(!o.anims[&tt].needle_down);
    o.command("volume.mute", &none, &rig.ctx()).unwrap();
    assert_eq!(s.sink.lock().unwrap()[2..], [0.0, 0.6]);
    assert!(o.anims[&tt].needle_down);
    assert!(o.command("nobody.else", &none, &rig.ctx()).is_none());
}

#[test]
fn goto_object_glides_to_its_stand() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let args = BTreeMap::from([("name".to_string(), "vitrola".to_string())]);
    let r = o.command("world.goto_object", &args, &rig.ctx()).unwrap();
    let stand = o.world.find(Kind::Turntable).unwrap().stand;
    assert_eq!(r, [Req::GlideTo(stand)]);
    let args = BTreeMap::from([("name".to_string(), "unicórnio".to_string())]);
    let r = o.command("world.goto_object", &args, &rig.ctx()).unwrap();
    assert!(matches!(
        &r[0],
        Req::Toast {
            kind: ToastKind::Error,
            ..
        }
    ));
}

#[test]
fn interact_needs_something_in_the_crosshair() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let none = BTreeMap::new();
    // at the desk, facing the wall: the clock is far off the crosshair
    assert_eq!(o.command("world.interact", &none, &rig.ctx()), None);
    // at the turntable's stand
    let tt = o.world.find(Kind::Turntable).unwrap().clone();
    rig.w.camera = tt.stand;
    assert_eq!(
        o.command("world.adjust_up", &none, &rig.ctx()),
        Some(vec![Req::Action(nkw_core::Action::Adjust(tt.id, 1))])
    );
    // facing the reception's door from inside
    let door = o
        .world
        .objects
        .iter()
        .find(|x| x.kind == Kind::Door(DoorId(0)))
        .unwrap()
        .clone();
    let mut p = door.stand;
    p.pos.x += 0.4; // off-centre, still through the gap
    rig.w.camera = p;
    assert_eq!(
        o.command("world.interact", &none, &rig.ctx()),
        Some(vec![Req::Action(nkw_core::Action::Interact(door.id))])
    );
}

#[test]
fn on_object_runs_the_model_and_doors_toggle_when_walking() {
    let mut rig = Rig::new();
    let (mut o, s) = faked(true);
    rig.frame(&mut o, 1.0);
    let tt = o.world.find(Kind::Turntable).unwrap().id;
    let r = o.on_object(tt, ObjectAction::Interact, &rig.ctx());
    assert!(r.contains(&redraw()));
    assert_eq!(*s.media.lock().unwrap(), ["play_pause"]);
    // a passable door from a NAV stand: walk through it
    let door = o
        .world
        .objects
        .iter()
        .find(|x| x.kind == Kind::Door(DoorId(0)))
        .unwrap()
        .id;
    let r = o.on_object(door, ObjectAction::Interact, &rig.ctx());
    assert!(matches!(r[..], [Req::GlideTo(_)]));
    // walking: it shuts
    rig.w.action(nkw_core::Action::Walk, 1.0);
    assert!(rig.w.walking.is_some());
    o.on_object(door, ObjectAction::Interact, &rig.ctx());
    rig.frame(&mut o, 2.0);
    assert_eq!(o.dump()["doors"][0]["passable"], false);
    // shut: interact opens it, walking or not
    rig.w.action(nkw_core::Action::Back, 2.0);
    o.on_object(door, ObjectAction::Interact, &rig.ctx());
    rig.frame(&mut o, 3.0);
    assert_eq!(o.dump()["doors"][0]["passable"], true);
}

#[test]
fn window_streams_are_anchored_steered_and_restored_on_shutdown() {
    let mut rig = Rig::new();
    let (mut o, s) = faked(true);
    let (sid, _) = rig.w.add_screen("acme.alpha", "alpha", 0.0);
    rig.pids.insert(sid, 4242);
    rig.frame(&mut o, 1.0);
    s.events
        .lock()
        .unwrap()
        .push(SysEvent::Audio(AudioEvent::Added(StreamInfo {
            id: StreamId(31),
            pid: Some(4242),
            binary: None,
            app_id: None,
            output: true,
        })));
    s.events
        .lock()
        .unwrap()
        .push(SysEvent::Audio(AudioEvent::Added(StreamInfo {
            id: StreamId(32),
            pid: None,
            binary: Some("nothing-of-ours".into()),
            app_id: None,
            output: true,
        })));
    for i in 0..40 {
        rig.frame(&mut o, 1.0 + i as f64 * 0.05);
    }
    let d = o.dump();
    assert_eq!(d["audio"]["streams"], 2);
    assert_eq!(d["audio"]["anchored"], 1);
    let vols = s.vols.lock().unwrap().clone();
    assert!(!vols.is_empty());
    assert!(
        vols.iter().all(|(id, _)| *id == StreamId(31)),
        "the unanchored one is never touched"
    );
    o.shutdown();
    assert_eq!(
        s.vols.lock().unwrap().last(),
        Some(&(StreamId(31), [1.0, 1.0]))
    );
    assert!(*s.shut.lock().unwrap());
}

#[test]
fn the_media_players_streams_sound_from_the_turntable() {
    let mut rig = Rig::new();
    let (mut o, s) = faked(true);
    rig.frame(&mut o, 1.0);
    s.events
        .lock()
        .unwrap()
        .push(SysEvent::Audio(AudioEvent::Added(StreamInfo {
            id: StreamId(5),
            pid: None,
            binary: Some("demo".into()),
            app_id: None,
            output: true,
        })));
    rig.frame(&mut o, 1.1);
    assert_eq!(
        o.audio.streams[&StreamId(5)].anchor,
        Anchor::Object(ObjectId(1))
    );
}

#[test]
fn dump_carries_nothing_personal() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let text = o.dump().to_string().to_lowercase();
    for bad in ["ssid", "lo-fi", "acme radio", "device", "hostname"] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
}

#[test]
fn draw_adds_objects_of_visible_rooms_and_every_door() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let ctx = rig.ctx();
    let mut list = crate::scene::draw_list(&rig.w, &|_| None, &rig.visible);
    let before = list.instances.len();
    o.draw(&ctx, &mut list);
    let added = &list.instances[before..];
    assert_eq!(
        added.iter().filter(|i| i.mesh == MeshId::DoorLeaf).count(),
        3
    );
    assert!(added.len() > 30, "{}", added.len());
    let none: Vec<RoomId> = Vec::new();
    let ctx2 = Ctx {
        visible_rooms: &none,
        ..rig.ctx()
    };
    let mut list = crate::scene::draw_list(&rig.w, &|_| None, &none);
    let before = list.instances.len();
    o.draw(&ctx2, &mut list);
    assert_eq!(list.instances.len() - before, 6, "only the doors");
}

#[test]
fn pickables_and_palette() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    // every door open: all three rooms are drawn, so everything is pickable
    assert_eq!(o.pickables(&rig.ctx()).len(), 14 + 2 + 3);
    rig.visible = vec![RoomId(0)];
    let p = o.pickables(&rig.ctx());
    // the reception's 14 + its door
    assert_eq!(p.len(), 15);
    assert!(p.iter().all(|x| matches!(x.pick, Pick::Object { .. })));
    // + world.open, world.library_search, world.vault_lock
    assert_eq!(o.palette_items(&rig.ctx()).len(), 10);
}

#[test]
fn bad_injects_toast() {
    let rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    let r = o.inject(&serde_json::json!({"Nope": 1}), &rig.ctx());
    assert!(matches!(
        &r[..],
        [Req::Toast {
            kind: ToastKind::Error,
            ..
        }]
    ));
    let r = o.inject(
        &serde_json::json!({"Command": {"name": "volume.mute"}}),
        &rig.ctx(),
    );
    assert_eq!(r, [redraw()]);
    assert!(
        o.inject(&serde_json::json!({"Cue": "Meow"}), &rig.ctx())
            .is_empty()
    );
    assert!(o.sound.played.last.contains(&Cue::Meow));
}

#[test]
fn walking_plays_footsteps_on_the_reception_boards() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    rig.w.action(nkw_core::Action::Walk, 1.0);
    let start = rig.w.camera;
    for i in 0..20 {
        let mut p = start;
        p.pos.z += i as f32 * 0.1;
        rig.w.camera = p;
        rig.frame(&mut o, 1.0 + i as f64 * 0.05);
    }
    let steps = o
        .sound
        .played
        .last
        .iter()
        .filter(|c| **c == Cue::Footstep(nkw_objects::sound::Floor::Wood))
        .count();
    assert_eq!(steps, 3, "1.9 m: three strides");
}

#[test]
fn a_tick_is_cheap() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    rig.w.command(crate::world::Cmd::RoomNext, 1.0);
    let t = std::time::Instant::now();
    for i in 0..100 {
        rig.frame(&mut o, 1.0 + i as f64 * 0.016);
    }
    let per = t.elapsed() / 100;
    eprintln!("objects drain+tick: {per:?}");
    // debug build, a loaded machine: generous, but far from a frame
    assert!(
        per < std::time::Duration::from_millis(2),
        "{per:?} per drain+tick"
    );
}

#[test]
fn the_vault_asks_the_receptionist_and_opens_with_the_phrase() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    let v = o.world.find(Kind::Vault).unwrap().id;
    let r = o.on_object(v, ObjectAction::Interact, &rig.ctx());
    let cat = rig.w.rooms[0].desks[0].cat.unwrap();
    let talk = nkw_core::Action::Talk {
        cat,
        purpose: nkw_core::TalkPurpose::VaultUnlock,
    };
    assert!(r.contains(&Req::Action(talk)));
    // the machine's effects come back through `effect`
    for out in rig.w.action(talk, 1.0) {
        if let crate::world::Out::Effect(e) = out {
            o.effect(&e, &rig.ctx());
        }
    }
    assert_eq!(o.dialog().unwrap().text, vault::ASK);
    let phrase = o.vault.p.phrase.clone();
    let ctx = Ctx {
        line: Some(&phrase),
        ..rig.ctx()
    };
    o.effect(&nkw_core::Effect::SendLine, &ctx);
    assert_eq!(o.dump()["badges"][0], "VaultOpen");
    // a screenshot locks it first
    o.inject(&serde_json::json!({"Shot": true}), &rig.ctx());
    assert!(!o.vault.unlocked());
}

#[test]
fn status_is_the_fixture_under_demo_and_the_data_otherwise() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    assert_eq!(o.status(&rig.ctx()), status::demo());
    // overrides for shots: merged, validated, a bad one changes nothing
    let r = o.inject(
        &serde_json::json!({"status": {"battery": [12, false], "playing": true}}),
        &rig.ctx(),
    );
    assert!(r.contains(&Req::Wish(Wish::compose(Want::Once))));
    o.inject(&serde_json::json!({"status": {"net": null}}), &rig.ctx());
    let s = o.status(&rig.ctx());
    assert_eq!(s.battery, Some((12, false)));
    assert_eq!(s.playing, Some(true));
    assert_eq!(s.net, None);
    assert_eq!(s.clock, Some((14, 5)));
    let r = o.inject(&serde_json::json!({"status": {"ssid": "x"}}), &rig.ctx());
    assert!(matches!(
        &r[0],
        Req::Toast {
            kind: ToastKind::Error,
            ..
        }
    ));
    assert_eq!(o.status(&rig.ctx()), s);

    // outside the demo: from the adapters' data (a fake port here)
    let (mut o, sh) = faked(false);
    sh.events.lock().unwrap().extend([
        SysEvent::Power(nkw_sys::PowerState {
            present: true,
            pct: 55,
            charging: true,
            time_to_empty_s: None,
        }),
        SysEvent::Net(nkw_sys::NetState {
            connected: false,
            wifi: false,
            strength: 0,
        }),
    ]);
    rig.frame(&mut o, 2.0);
    let s = o.status(&rig.ctx());
    assert_eq!(s.battery, Some((55, true)));
    assert_eq!(s.net, Some((false, 0)));
    assert_eq!(s.playing, None, "no player");
    assert!(!s.vault_open);
}

#[test]
fn the_vault_open_reaches_the_status_and_a_shot_closes_it() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    o.inject(
        &serde_json::json!({"vault": {"phrase": "destrancar"}}),
        &rig.ctx(),
    );
    assert!(o.status(&rig.ctx()).vault_open);
    o.inject(&serde_json::json!({"Shot": true}), &rig.ctx());
    assert!(!o.status(&rig.ctx()).vault_open);
}

#[test]
fn idle_inhibit_is_recorded_and_never_reaches_the_host_under_demo() {
    let mut rig = Rig::new();
    let mut o = Objects::with(true, false, None);
    rig.frame(&mut o, 1.0);
    assert!(
        o.inject(&serde_json::json!({"IdleInhibit": true}), &rig.ctx())
            .is_empty()
    );
    let d = o.dump();
    assert_eq!(d["idle_inhibit"]["wanted"], true);
    assert_eq!(d["idle_inhibit"]["host"], "none");
    o.inject(&serde_json::json!({"IdleInhibit": false}), &rig.ctx());
    assert_eq!(o.dump()["idle_inhibit"]["wanted"], false);
}
