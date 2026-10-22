//! The Bevy app: one window, the office scene, the chrome, and thin systems
//! that feed the pure modules (nav, strip, inbox, monitor, keys) and copy
//! their answers onto entities. Rendering is on demand (reactive winit):
//! frames are requested only while something moves.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::DebandDither;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{PresentMode, PrimaryWindow, RequestRedraw, WindowResized};
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};

use crate::catpose::{self, CatParams};
use crate::cli::Cli;
use crate::core_io::{self, Io, Req, Resp, WorldData};
use crate::inbox::{self, Queue};
use crate::keys::{self, Bind, Client, Route, Router};
use crate::model::{Office, Palette, Pose, Rgb, Sess, mix, now_ms};
use crate::monitor::{self, Feed, Role};
use crate::nav::{Act, Effect, Mode, Nav};
use crate::nkc;
use crate::raster::{self, Canvas, Face, TCell};
use crate::scene::{self, ColumnParts, IconKind, Kit, Mats, Rig, WalkerRig, rgb};
use crate::strip::{self, CamPose, Tween};
use crate::strip::{WALKER_HL, WALKER_SCALE};

/// Monitor texture size (px) for the activity tail.
const MON_W: usize = 640;
const MON_H: usize = 400;
const MON_FONT: f32 = 14.0;
/// The zoomed terminal's font size (logical px).
const TERM_FONT: f32 = 15.0;
/// How much of the viewport the zoomed monitor fills.
const ZOOM_FILL: f32 = 0.84;
const CAM_S: f64 = 0.42;
const TOP_H: f32 = 28.0;
const STATUS_H: f32 = 26.0;

pub fn run(cli: Cli) {
    let mut app = App::new();
    let window = Window {
        title: "nekuroddo".into(),
        name: Some("nekuroddo".into()),
        resolution: (1440, 860).into(),
        // a screenshot run must not wait on the compositor's frame callbacks
        // (Wayland stops them once the window is hidden)
        present_mode: if cli.shot.is_some() {
            PresentMode::AutoNoVsync
        } else {
            PresentMode::AutoVsync
        },
        ..default()
    };
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window),
        ..default()
    }));
    let idle = UpdateMode::reactive_low_power(Duration::from_secs(5));
    app.insert_resource(WinitSettings {
        focused_mode: idle,
        unfocused_mode: idle,
    });
    app.insert_resource(ClearColor(rgb(Palette::default().bg)));
    app.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 900.0,
        ..default()
    });
    app.insert_resource(CliRes(cli));
    app.add_systems(Startup, (setup, setup_ui).chain());
    app.add_systems(
        Update,
        (
            pump_io,
            keyboard,
            mouse,
            apply_effects,
            sync_columns,
            camera,
            animate_cats,
            monitors,
            panes,
            walkers,
            chrome,
            overlay,
            power,
            shot,
        )
            .chain(),
    );
    app.run();
    // let the core worker notice and stop the `nk serve` it spawned
    drop(app);
    std::thread::sleep(Duration::from_millis(300));
}

#[derive(Resource)]
struct CliRes(Cli);

#[derive(Resource)]
struct IoRes(Io);

#[derive(Resource)]
struct Faces {
    mon: Face,
    term: Face,
    term_scale: f32,
    /// window size, logical px
    vp: Vec2,
}

/// A monitor's tail feed and texture.
struct Mon {
    feed: Feed,
    image: Handle<Image>,
    mat: Handle<StandardMaterial>,
    dirty: bool,
}

/// An open pane: the connection, its emulator and its texture.
struct PaneSt {
    conn: Mutex<nkc::PaneConn>,
    term: nkc::Term,
    image: Handle<Image>,
    cols: u16,
    rows: u16,
    dirty: bool,
    last_draw: f64,
}

/// Per-cat animation state.
#[derive(Component)]
struct CatAnim {
    sid: String,
    pose: Pose,
    from: CatParams,
    t0: f64,
    phase: f32,
    puff_t0: Option<f64>,
    kitten: bool,
}

#[derive(Component)]
struct ColumnRoot {
    sid: String,
    look: String,
}

#[derive(Component)]
struct CamMarker;

#[derive(Component)]
struct TopBar;
#[derive(Component)]
struct StatusBar;
#[derive(Component)]
struct CtxLabel(usize);
#[derive(Component)]
struct LabelLayer;
#[derive(Component)]
struct TermOverlay;
#[derive(Component)]
struct PromptBox;
#[derive(Component)]
struct OverflowBadge;

/// Everything the app knows (one resource keeps system params short).
#[derive(Resource)]
struct St {
    palette: Palette,
    sessions: Vec<Sess>,
    office: Office,
    xs: Vec<f32>,
    mail: inbox::Mail,
    usage_5h: Option<i64>,
    today: f64,
    version: String,
    nav: Nav,
    tween: Tween,
    zoom_target: bool,
    router: Router,
    binds: Vec<Bind>,
    toast: Option<(String, bool, f64)>,
    prompt: Option<(String, String)>,
    bars: bool,
    status: String,
    queue: Queue,
    monitors: HashMap<String, Mon>,
    panes: HashMap<String, PaneSt>,
    pane_ids: HashMap<String, String>,
    pane_requested: HashSet<String>,
    effects: Vec<Effect>,
    loaded: bool,
    layout_key: String,
    puffs: HashMap<String, f64>,
    prev_state: HashMap<String, String>,
    wheel: f32,
    animating_until: f64,
    quit: bool,
    reduced: bool,
    font: Handle<Font>,
    fontdue: fontdue::Font,
    walker_ents: HashMap<String, Entity>,
    shot_frame: u32,
    shot_t0: Option<f64>,
    /// the earliest a monitor or pane wants the next frame (ms)
    wake_ms: Option<i64>,
    shot_stage: u8,
    chrome_key: String,
}

/// Seconds of wall time since the first frame. Not Bevy's virtual clock:
/// that one caps each frame at 250 ms, so after the app sleeps (reactive mode)
/// animations would lag the wall-clock deadlines they are compared with.
fn now_s(t: &Time<Real>) -> f64 {
    t.elapsed_secs_f64()
}

fn setup(
    mut c: Commands,
    cli: Res<CliRes>,
    proxy: Res<EventLoopProxyWrapper>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut fonts: ResMut<Assets<Font>>,
) {
    let cli = &cli.0;
    let p = Palette::default();
    let proxy = (*proxy).clone();
    let wake: core_io::Wake = Arc::new(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    let io = core_io::spawn(
        nkc::StartOpts {
            demo: cli.demo,
            connect: cli.connect.clone(),
            nk_bin: cli.nk_bin.clone(),
            ..Default::default()
        },
        wake,
    );
    c.insert_resource(IoRes(io));
    let kit = Kit::new(&mut meshes);
    let m = Mats::new(&p, &mut mats);
    scene::spawn_room(&mut c, &kit, &m);
    // camera + light
    c.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: strip::FOV,
            ..default()
        }),
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Sample4,
        Transform::from_translation(strip::office_pose(0.0, 1.6).eye)
            .looking_at(strip::office_pose(0.0, 1.6).look, Vec3::Y),
        CamMarker,
    ));
    c.spawn((
        DirectionalLight {
            illuminance: 3800.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-3.0, 9.0, 7.0).looking_at(Vec3::new(1.0, 0.0, -1.0), Vec3::Y),
        // the office is a few metres deep: one cascade is plenty (the default
        // four, out to 1000 units, cost most of each frame)
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.5,
            maximum_distance: 22.0,
            first_cascade_far_bound: 22.0,
            overlap_proportion: 0.0,
        }
        .build(),
    ));
    let font = fonts.add(Font::from_bytes(raster::FONT_BYTES.to_vec()));
    let fd = Face::load_font();
    c.insert_resource(Faces {
        mon: Face::new(&fd, MON_FONT),
        term: Face::new(&fd, TERM_FONT),
        term_scale: 1.0,
        vp: Vec2::new(1440.0, 860.0),
    });
    let binds = keys::fallback();
    c.insert_resource(St {
        palette: p,
        sessions: vec![],
        office: Office::default(),
        xs: vec![],
        mail: HashMap::new(),
        usage_5h: None,
        today: 0.0,
        version: String::new(),
        nav: Nav::new(0),
        tween: Tween::still(strip::office_pose(0.0, 1.6)),
        zoom_target: false,
        router: Router::new(&binds, &HashSet::new()),
        binds,
        toast: None,
        prompt: None,
        bars: true,
        status: "conectando ao core…".into(),
        queue: Queue::default(),
        monitors: HashMap::new(),
        panes: HashMap::new(),
        pane_ids: HashMap::new(),
        pane_requested: HashSet::new(),
        effects: vec![],
        loaded: false,
        layout_key: String::new(),
        puffs: HashMap::new(),
        prev_state: HashMap::new(),
        wheel: 0.0,
        animating_until: 0.0,
        quit: false,
        reduced: cli.reduced_motion,
        font,
        fontdue: fd,
        walker_ents: HashMap::new(),
        shot_frame: 0,
        shot_t0: None,
        wake_ms: None,
        shot_stage: 0,
        chrome_key: String::new(),
    });
    c.insert_resource(kit);
    c.insert_resource(m);
}

fn tfont(st: &St, size: f32) -> TextFont {
    TextFont {
        font: st.font.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn setup_ui(mut c: Commands, st: Res<St>) {
    let p = &st.palette;
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            right: px(0),
            height: px(TOP_H),
            padding: UiRect::horizontal(px(12)),
            align_items: AlignItems::Center,
            column_gap: px(18),
            ..default()
        },
        BackgroundColor(rgb(p.crust).with_alpha(0.92)),
        GlobalZIndex(10),
        TopBar,
    ));
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(0),
            left: px(0),
            right: px(0),
            height: px(STATUS_H),
            padding: UiRect::horizontal(px(10)),
            align_items: AlignItems::Center,
            column_gap: px(14),
            ..default()
        },
        BackgroundColor(rgb(p.crust).with_alpha(0.92)),
        GlobalZIndex(10),
        StatusBar,
    ));
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            right: px(0),
            bottom: px(0),
            ..default()
        },
        GlobalZIndex(1),
        LabelLayer,
    ));
    c.spawn((
        ImageNode::default(),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: px(10),
            height: px(10),
            ..default()
        },
        BackgroundColor(rgb(p.bg)),
        Visibility::Hidden,
        GlobalZIndex(5),
        TermOverlay,
    ));
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(14),
            bottom: px(STATUS_H + 10.0),
            padding: UiRect::axes(px(10), px(4)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(rgb(p.blocked)),
        Visibility::Hidden,
        GlobalZIndex(6),
        OverflowBadge,
        children![(Text::new(""), tfont(&st, 15.0), TextColor(rgb(p.crust)))],
    ));
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(42),
            margin: UiRect::left(px(-260)),
            width: px(520),
            padding: UiRect::all(px(18)),
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            ..default()
        },
        BackgroundColor(rgb(p.surface)),
        BorderColor::all(rgb(p.blocked)),
        Visibility::Hidden,
        GlobalZIndex(20),
        PromptBox,
        children![
            (Text::new(""), tfont(&st, 17.0), TextColor(rgb(p.text))),
            (
                Text::new("y sim  ·  n não"),
                tfont(&st, 14.0),
                TextColor(rgb(p.subtle))
            ),
        ],
    ));
}

// ---------------------------------------------------------------- core I/O

#[allow(clippy::too_many_arguments)]
fn pump_io(
    io: Res<IoRes>,
    mut st: ResMut<St>,
    time: Res<Time<Real>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut mats_res: ResMut<Mats>,
    mut clear: ResMut<ClearColor>,
    mut images: ResMut<Assets<Image>>,
    faces: Res<Faces>,
    mut cats: Query<&mut CatAnim>,
) {
    let t = now_s(&time);
    while let Ok(msg) = io.0.rx.try_recv() {
        match msg {
            Resp::Init {
                palette,
                binds,
                client_cmds,
                version,
            } => {
                clear.0 = rgb(palette.bg);
                mats_res.recolor(&palette, &mut mats);
                st.palette = palette;
                let binds = if binds.is_empty() {
                    keys::fallback()
                } else {
                    binds
                };
                st.router = Router::new(&binds, &client_cmds);
                st.binds = binds;
                st.version = version;
                st.status.clear();
                st.chrome_key.clear();
            }
            Resp::World(w) => apply_world(&mut st, w, t, &mut cats),
            Resp::Tail { id, lines } => {
                let reduced = st.reduced;
                if let Some(m) = st.monitors.get_mut(&id) {
                    let first = m.feed.lines.is_empty();
                    m.feed.update(lines, !first && !reduced);
                    m.dirty = true;
                }
            }
            Resp::Event { name, data } => on_event(&mut st, &io.0, &name, &data, t),
            Resp::Cmd {
                toast,
                err,
                pending,
                data,
            } => {
                if let Some(p) = pending {
                    st.prompt = Some(p);
                } else if !toast.is_empty() {
                    st.toast = Some((toast, err, t));
                }
                if let Some(ids) = data.get("close_panes").and_then(|v| v.as_array()) {
                    for id in ids.iter().filter_map(|v| v.as_str()) {
                        let sid = id.to_string();
                        close_pane(&mut st, &sid, false);
                    }
                }
            }
            Resp::Pane { session, conn } => {
                st.pane_requested.remove(&session);
                let (cols, rows) = expected_cells(&st, &faces);
                let term = nkc::Term::new(cols, rows);
                let image = images.add(blank_image(
                    cols as usize * faces.term.cw,
                    rows as usize * faces.term.ch,
                    st.palette.bg,
                ));
                st.pane_ids.insert(conn.id().to_string(), session.clone());
                conn.resize(cols, rows);
                if let Some(m) = st.monitors.get(&session)
                    && let Some(mut mm) = mats.get_mut(&m.mat)
                {
                    mm.base_color_texture = Some(image.clone());
                }
                st.panes.insert(
                    session,
                    PaneSt {
                        conn: Mutex::new(conn),
                        term,
                        image,
                        cols,
                        rows,
                        dirty: true,
                        last_draw: 0.0,
                    },
                );
            }
            Resp::PaneMsg { pane, msg } => {
                let Some(sid) = st.pane_ids.get(&pane).cloned() else {
                    continue;
                };
                match msg {
                    nkc::PaneMsg::Data(b) => {
                        if let Some(p) = st.panes.get_mut(&sid) {
                            p.term.feed(&b);
                            p.dirty = true;
                        }
                    }
                    nkc::PaneMsg::Title(_) => {}
                    nkc::PaneMsg::Exit(err) => {
                        let label = st
                            .sessions
                            .iter()
                            .find(|s| s.id == sid)
                            .map(|s| s.label.clone())
                            .unwrap_or_default();
                        st.toast = Some((
                            format!(
                                "painel de {label} saiu{}",
                                err.map(|e| format!(": {e}")).unwrap_or_default()
                            ),
                            false,
                            t,
                        ));
                        drop_pane(&mut st, &sid, &mut mats);
                    }
                    nkc::PaneMsg::Closed => drop_pane(&mut st, &sid, &mut mats),
                }
            }
            Resp::Status(s) => st.status = s,
            Resp::Error(e) => {
                st.status = e;
                st.loaded = true;
            }
        }
    }
    if st.toast.as_ref().is_some_and(|(_, _, t0)| t - t0 > 4.0) {
        st.toast = None;
    }
}

fn apply_world(st: &mut St, w: WorldData, t: f64, cats: &mut Query<&mut CatAnim>) {
    // a session whose state went to error: one-shot puff
    for s in &w.sessions {
        if let Some(prev) = st.prev_state.get(&s.id)
            && prev != &s.state
            && (s.state == "error" || s.state == "failed" || (s.ended && prev != "ended"))
        {
            st.puffs.insert(s.id.clone(), t);
        }
    }
    st.prev_state = w
        .sessions
        .iter()
        .map(|s| {
            (
                s.id.clone(),
                if s.ended {
                    "ended".into()
                } else {
                    s.state.clone()
                },
            )
        })
        .collect();
    let focused = st
        .office
        .columns
        .get(st.nav.focus)
        .map(|c| c.sess.id.clone());
    st.office = Office::build(&w.sessions, &w.rows);
    st.sessions = w.sessions;
    st.mail = w.mail;
    st.usage_5h = w.usage_5h;
    st.today = w.today_cost;
    st.xs = strip::column_xs(&st.office.ctxs());
    let keep = focused.and_then(|id| st.office.index_of(&id));
    let len = st.office.columns.len();
    st.nav.resize(len, keep);
    if !st.loaded {
        st.loaded = true;
    }
    for mut a in cats.iter_mut() {
        if let Some(p) = st.puffs.get(&a.sid)
            && a.puff_t0 != Some(*p)
        {
            a.puff_t0 = Some(*p);
        }
    }
    st.chrome_key.clear();
}

fn on_event(st: &mut St, io: &Io, name: &str, data: &serde_json::Value, t: f64) {
    let s = |k: &str| {
        data.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match name {
        "inbox.summoned" => {
            let id = s("session_id");
            if let Some(i) = st.office.index_of(&id) {
                st.queue = inbox::summon(&st.queue, &id, now_ms());
                let fx = st.nav.apply(Act::FocusZoom(i));
                st.effects.extend(fx);
            }
        }
        "peer.message" => {
            st.queue = inbox::note_peer(&st.queue, &s("to"), &s("from"), &s("text"));
        }
        "toast" => {
            st.toast = Some((
                s("text"),
                data.get("err").and_then(|v| v.as_bool()).unwrap_or(false),
                t,
            ))
        }
        "notice" => {
            let msg = [s("toast"), s("toast_err"), s("meow")]
                .into_iter()
                .find(|x| !x.is_empty());
            if let Some(m) = msg {
                st.toast = Some((m, !s("toast_err").is_empty(), t));
            }
        }
        "confirm.resolved"
            if st
                .prompt
                .as_ref()
                .is_some_and(|(tok, _)| *tok == s("token")) =>
        {
            st.prompt = None;
        }
        _ => {}
    }
    let _ = io;
}

fn blank_image(w: usize, h: usize, bg: Rgb) -> Image {
    let cv = Canvas::new(w.max(1), h.max(1), bg);
    canvas_image(cv)
}

fn canvas_image(cv: Canvas) -> Image {
    Image::new(
        Extent3d {
            width: cv.w as u32,
            height: cv.h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        cv.px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn close_pane(st: &mut St, sid: &str, kill: bool) {
    if let Some(p) = st.panes.get(sid) {
        p.conn.lock().unwrap().close(kill);
    }
}

fn drop_pane(st: &mut St, sid: &str, mats: &mut Assets<StandardMaterial>) {
    if let Some(p) = st.panes.remove(sid) {
        let id = p.conn.lock().unwrap().id().to_string();
        st.pane_ids.remove(&id);
    }
    if let Some(m) = st.monitors.get_mut(sid) {
        if let Some(mut mm) = mats.get_mut(&m.mat) {
            mm.base_color_texture = Some(m.image.clone());
        }
        m.dirty = true;
    }
    if st.nav.zoomed()
        && st
            .office
            .columns
            .get(st.nav.focus)
            .is_some_and(|c| c.sess.id == sid)
    {
        st.nav.mode = Mode::Office;
    }
}

/// Terminal size for the zoomed monitor in this window (cells).
fn expected_cells(st: &St, faces: &Faces) -> (u16, u16) {
    let _ = st;
    let vp = faces_viewport(faces);
    let rect = zoom_rect(vp);
    let cell = Vec2::new(faces.term.cw as f32, faces.term.ch as f32);
    strip::fit_cells(rect * faces.term_scale, cell)
}

/// Viewport size in logical px (stored on Faces by `overlay` each frame).
fn faces_viewport(f: &Faces) -> Vec2 {
    f.vp.max(Vec2::new(400.0, 300.0))
}

/// Logical size of the zoomed screen for a viewport (camera at zoom_pose).
fn zoom_rect(vp: Vec2) -> Vec2 {
    let aspect = vp.x / vp.y;
    let pose = strip::zoom_pose(0.0, aspect, ZOOM_FILL);
    let d = pose.eye.z - strip::SCREEN_Z;
    let vis_h = 2.0 * d * (strip::FOV / 2.0).tan();
    let h = strip::SCREEN_H / vis_h * vp.y;
    let w = strip::SCREEN_W / (vis_h * aspect) * vp.x;
    Vec2::new(w, h)
}

// ---------------------------------------------------------------- input

fn keyname(k: &nkc::Key, mods: nkc::Mods) -> String {
    nkc::key_name(k, mods)
}

/// Bevy key → nk key (None: a modifier or something we don't send).
fn nk_key(k: &Key) -> Option<nkc::Key> {
    Some(match k {
        Key::Character(s) => nkc::Key::Char(s.chars().next()?),
        Key::Space => nkc::Key::Char(' '),
        Key::Enter => nkc::Key::Enter,
        Key::Tab => nkc::Key::Tab,
        Key::Backspace => nkc::Key::Backspace,
        Key::Escape => nkc::Key::Esc,
        Key::ArrowUp => nkc::Key::Up,
        Key::ArrowDown => nkc::Key::Down,
        Key::ArrowLeft => nkc::Key::Left,
        Key::ArrowRight => nkc::Key::Right,
        Key::Home => nkc::Key::Home,
        Key::End => nkc::Key::End,
        Key::PageUp => nkc::Key::PageUp,
        Key::PageDown => nkc::Key::PageDown,
        Key::Insert => nkc::Key::Insert,
        Key::Delete => nkc::Key::Delete,
        Key::F1 => nkc::Key::F(1),
        Key::F2 => nkc::Key::F(2),
        Key::F3 => nkc::Key::F(3),
        Key::F4 => nkc::Key::F(4),
        Key::F5 => nkc::Key::F(5),
        Key::F6 => nkc::Key::F(6),
        Key::F7 => nkc::Key::F(7),
        Key::F8 => nkc::Key::F(8),
        Key::F9 => nkc::Key::F(9),
        Key::F10 => nkc::Key::F(10),
        Key::F11 => nkc::Key::F(11),
        Key::F12 => nkc::Key::F(12),
        _ => return None,
    })
}

fn focused_sid(st: &St) -> Option<String> {
    st.office
        .columns
        .get(st.nav.focus)
        .map(|c| c.sess.id.clone())
}

fn keyboard(
    mut ev: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    mut st: ResMut<St>,
    io: Res<IoRes>,
    time: Res<Time<Real>>,
) {
    let t = now_s(&time);
    let ctrl = held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let alt = held.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let shift = held.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for e in ev.read() {
        if e.state != ButtonState::Pressed {
            continue;
        }
        let Some(k) = nk_key(&e.logical_key) else {
            continue;
        };
        // a pending confirmation takes the next y/n
        if let Some((token, _)) = st.prompt.clone() {
            if let nkc::Key::Char(c) = k {
                let yes = matches!(c, 'y' | 'Y' | 's' | 'S');
                io.0.tx.send(Req::Confirm { token, yes }).ok();
                st.prompt = None;
            } else if matches!(k, nkc::Key::Esc) {
                io.0.tx.send(Req::Confirm { token, yes: false }).ok();
                st.prompt = None;
            }
            continue;
        }
        if st.nav.mode == Mode::Insert {
            insert_key(&mut st, &k, e.text.as_deref(), ctrl, alt, shift);
            continue;
        }
        let is_char = matches!(k, nkc::Key::Char(c) if c != ' ');
        let mods = nkc::Mods {
            ctrl,
            alt,
            shift: shift && !is_char,
        };
        let name = keyname(&k, mods);
        match st.router.feed(&name) {
            Route::Client(a) => client(&mut st, a, &io.0, t),
            Route::Core(cmd) => {
                let session = focused_sid(&st);
                io.0.tx.send(Req::Run { name: cmd, session }).ok();
            }
            Route::Unsupported(cmd) => {
                st.toast = Some((format!("{cmd}: ainda não no nkg"), false, t))
            }
            Route::Pending | Route::None => {}
        }
    }
}

fn insert_key(st: &mut St, k: &nkc::Key, text: Option<&str>, ctrl: bool, alt: bool, shift: bool) {
    let sid = match focused_sid(st) {
        Some(s) => s,
        None => return,
    };
    if ctrl && let nkc::Key::Char(c) = k {
        if *c == ']' {
            st.nav.apply(Act::Normal);
            return;
        }
        if shift && (*c == 'v' || *c == 'V') {
            if let Some(p) = st.panes.get(&sid)
                && let Ok(txt) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text())
            {
                p.conn.lock().unwrap().write(txt.as_bytes());
            }
            return;
        }
    }
    let Some(p) = st.panes.get(&sid) else { return };
    let bytes = match (k, text) {
        (nkc::Key::Char(_), Some(txt)) if !ctrl && !alt && !txt.chars().any(|c| c.is_control()) => {
            txt.as_bytes().to_vec()
        }
        _ => {
            let key = match k {
                nkc::Key::Char(c) => nkc::Key::Char(*c),
                other => clone_key(other),
            };
            nkc::encode_key(key, nkc::Mods { ctrl, alt, shift })
        }
    };
    if !bytes.is_empty() {
        p.conn.lock().unwrap().write(&bytes);
    }
}

fn clone_key(k: &nkc::Key) -> nkc::Key {
    match k {
        nkc::Key::Char(c) => nkc::Key::Char(*c),
        nkc::Key::Enter => nkc::Key::Enter,
        nkc::Key::Tab => nkc::Key::Tab,
        nkc::Key::Backspace => nkc::Key::Backspace,
        nkc::Key::Esc => nkc::Key::Esc,
        nkc::Key::Up => nkc::Key::Up,
        nkc::Key::Down => nkc::Key::Down,
        nkc::Key::Left => nkc::Key::Left,
        nkc::Key::Right => nkc::Key::Right,
        nkc::Key::Home => nkc::Key::Home,
        nkc::Key::End => nkc::Key::End,
        nkc::Key::PageUp => nkc::Key::PageUp,
        nkc::Key::PageDown => nkc::Key::PageDown,
        nkc::Key::Insert => nkc::Key::Insert,
        nkc::Key::Delete => nkc::Key::Delete,
        nkc::Key::F(n) => nkc::Key::F(*n),
    }
}

fn client(st: &mut St, a: Client, io: &Io, t: f64) {
    let ctxs: Vec<String> = st.office.ctxs().iter().map(|s| s.to_string()).collect();
    let refs: Vec<&str> = ctxs.iter().map(|s| s.as_str()).collect();
    let act = match a {
        Client::Left => Some(Act::Left),
        Client::Right => Some(Act::Right),
        Client::NextCtx => Some(Act::Focus(strip::ctx_jump(&refs, st.nav.focus, 1))),
        Client::PrevCtx => Some(Act::Focus(strip::ctx_jump(&refs, st.nav.focus, -1))),
        Client::First => Some(Act::Focus(0)),
        Client::Last => Some(Act::Focus(st.nav.len.saturating_sub(1))),
        Client::Zoom => Some(Act::Zoom),
        Client::Back => Some(Act::Back),
        Client::Close => Some(Act::Close),
        Client::Insert => Some(Act::Insert),
        Client::ToggleBars => {
            st.bars = !st.bars;
            st.chrome_key.clear();
            None
        }
        Client::NextAttention | Client::PrevAttention => {
            let n = st.office.columns.len();
            let dir = if a == Client::NextAttention {
                1
            } else {
                n.saturating_sub(1)
            };
            (1..=n)
                .map(|k| (st.nav.focus + k * dir) % n.max(1))
                .find(|&i| {
                    st.office.columns[i].sess.attention == crate::model::ATT_BLOCKED
                        || st.office.columns[i].sess.attention == crate::model::ATT_RETURNED
                })
                .map(Act::Focus)
        }
        Client::Quit => {
            st.quit = true;
            None
        }
    };
    if let Some(act) = act {
        let fx = st.nav.apply(act);
        st.effects.extend(fx);
    }
    let _ = (io, t);
}

#[derive(Default)]
struct MouseState {
    last_click: f64,
}

#[allow(clippy::too_many_arguments)]
fn mouse(
    mut wheel: MessageReader<MouseWheel>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<CamMarker>>,
    mut st: ResMut<St>,
    time: Res<Time<Real>>,
    walkers: Query<(&WalkerRig, &GlobalTransform)>,
    mut ms: Local<MouseState>,
) {
    for w in wheel.read() {
        let d = match w.unit {
            MouseScrollUnit::Line => w.y + w.x,
            MouseScrollUnit::Pixel => (w.y + w.x) / 40.0,
        };
        st.wheel += d;
    }
    if st.nav.mode != Mode::Insert {
        while st.wheel >= 1.0 {
            st.wheel -= 1.0;
            let fx = st.nav.apply(Act::Left);
            st.effects.extend(fx);
        }
        while st.wheel <= -1.0 {
            st.wheel += 1.0;
            let fx = st.nav.apply(Act::Right);
            st.effects.extend(fx);
        }
    } else {
        st.wheel = 0.0;
    }
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(pos) = window.cursor_position() else {
        return;
    };
    let (camera, ctf) = *cam;
    let Ok(ray) = camera.viewport_to_world(ctf, pos) else {
        return;
    };
    let t = now_s(&time);
    let dbl = t - ms.last_click < 0.35;
    ms.last_click = t;
    // inbox cats first (they're in front)
    let mut best: Option<(f32, Act)> = None;
    for (w, gt) in &walkers {
        let c = gt.translation() + Vec3::new(0.0, 0.4, 0.0);
        if let Some(d) = ray_aabb(
            ray.origin,
            *ray.direction,
            c - Vec3::new(0.5, 0.45, 0.35),
            c + Vec3::new(0.5, 0.55, 0.35),
        ) && let Some(sid) = st
            .queue
            .walkers
            .iter()
            .find(|x| x.key == w.key)
            .map(|x| x.session_id.clone())
            && let Some(i) = st.office.index_of(&sid)
            && best.is_none_or(|b| d < b.0)
        {
            best = Some((d, Act::FocusZoom(i)));
        }
    }
    if best.is_none() {
        for (i, x) in st.xs.iter().enumerate() {
            let boxes = [
                (
                    Vec3::new(x - 1.3, strip::SCREEN_Y - 0.8, strip::SCREEN_Z - 0.1),
                    Vec3::new(x + 1.3, strip::SCREEN_Y + 0.8, strip::SCREEN_Z + 0.05),
                ),
                (
                    Vec3::new(x - 0.5, strip::SEAT_Y, strip::CAT_Z - 0.5),
                    Vec3::new(x + 0.5, strip::SEAT_Y + 1.25, strip::CAT_Z + 0.45),
                ),
                (
                    Vec3::new(x - 1.4, 0.0, strip::DESK_Z - 0.6),
                    Vec3::new(x + 1.4, strip::DESK_TOP, strip::DESK_Z + 0.6),
                ),
            ];
            for (lo, hi) in boxes {
                if let Some(d) = ray_aabb(ray.origin, *ray.direction, lo, hi) {
                    let act = if (i == st.nav.focus && st.nav.mode == Mode::Office) || dbl {
                        Act::FocusZoom(i)
                    } else {
                        Act::Focus(i)
                    };
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, act));
                    }
                }
            }
        }
    }
    if let Some((_, act)) = best {
        let fx = st.nav.apply(act);
        st.effects.extend(fx);
    }
}

/// Ray vs axis-aligned box: distance along the ray, if it hits.
pub fn ray_aabb(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let inv = Vec3::ONE / d;
    let t1 = (lo - o) * inv;
    let t2 = (hi - o) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    if tmax >= tmin.max(0.0) {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

fn apply_effects(
    mut st: ResMut<St>,
    io: Res<IoRes>,
    faces: Res<Faces>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut exit: MessageWriter<AppExit>,
) {
    let fx = std::mem::take(&mut st.effects);
    for e in fx {
        match e {
            Effect::OpenPane(i) => {
                let Some(sid) = st.office.columns.get(i).map(|c| c.sess.id.clone()) else {
                    continue;
                };
                st.queue = inbox::dismiss_session(&st.queue, &sid);
                if st.panes.contains_key(&sid) || st.pane_requested.contains(&sid) {
                    continue;
                }
                let (cols, rows) = expected_cells(&st, &faces);
                st.pane_requested.insert(sid.clone());
                io.0.tx
                    .send(Req::OpenPane {
                        session: sid,
                        cols,
                        rows,
                    })
                    .ok();
            }
            Effect::ClosePane(i) => {
                let Some(sid) = st.office.columns.get(i).map(|c| c.sess.id.clone()) else {
                    continue;
                };
                close_pane(&mut st, &sid, true);
                drop_pane(&mut st, &sid, &mut mats);
            }
        }
    }
    if st.quit {
        st.quit = false;
        exit.write(AppExit::Success);
    }
}

// ---------------------------------------------------------------- scene sync

#[allow(clippy::too_many_arguments)]
fn sync_columns(
    mut c: Commands,
    mut st: ResMut<St>,
    kit: Res<Kit>,
    m: Res<Mats>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    io: Res<IoRes>,
    roots: Query<(Entity, &ColumnRoot)>,
    rugs: Query<Entity, With<scene::Rug>>,
    labels: Query<Entity, With<CtxLabel>>,
    layer: Single<Entity, With<LabelLayer>>,
    mut tfs: Query<&mut Transform>,
    time: Res<Time<Real>>,
) {
    let key = layout_key(&st);
    if key == st.layout_key {
        return;
    }
    st.layout_key = key;
    let t = now_s(&time);
    let existing: HashMap<String, (Entity, String)> = roots
        .iter()
        .map(|(e, r)| (r.sid.clone(), (e, r.look.clone())))
        .collect();
    let wanted: HashSet<String> = st
        .office
        .columns
        .iter()
        .map(|c| c.sess.id.clone())
        .collect();
    for (sid, (e, _)) in &existing {
        if !wanted.contains(sid) {
            c.entity(*e).despawn();
        }
    }
    st.monitors.retain(|sid, _| wanted.contains(sid));
    let p = st.palette.clone();
    let cols = st.office.columns.clone();
    for (i, col) in cols.iter().enumerate() {
        let x = st.xs[i];
        let look = look_key(col);
        if let Some((e, old)) = existing.get(&col.sess.id) {
            if *old == look {
                if let Ok(mut tf) = tfs.get_mut(*e) {
                    tf.translation.x = x;
                }
                continue;
            }
            c.entity(*e).despawn();
        }
        // monitor texture + material (kept across respawns)
        if !st.monitors.contains_key(&col.sess.id) {
            let image = images.add(blank_image(MON_W, MON_H, p.crust));
            let mat = mats.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                unlit: true,
                ..default()
            });
            st.monitors.insert(
                col.sess.id.clone(),
                Mon {
                    feed: Feed::default(),
                    image,
                    mat,
                    dirty: true,
                },
            );
            io.0.tx.send(Req::Tail(col.sess.id.clone())).ok();
        }
        let mon_mat = st.monitors[&col.sess.id].mat.clone();
        let seed = col
            .sess
            .id
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
        let (root, parts, _rig) = scene::spawn_column(
            &mut c,
            &kit,
            &m,
            x,
            &col.sess.id,
            mon_mat,
            col.sess.running_subs,
            seed,
        );
        let pose = col.sess.pose();
        let phase = catpose::phase_of(&col.sess.id);
        c.entity(parts.cat).insert(CatAnim {
            sid: col.sess.id.clone(),
            pose,
            from: catpose::pose_params(pose),
            t0: t - 10.0,
            phase,
            puff_t0: None,
            kitten: false,
        });
        for (k, ke) in parts.kittens.iter().enumerate() {
            c.entity(*ke).insert(CatAnim {
                sid: col.sess.id.clone(),
                pose: Pose::Idle,
                from: catpose::SIT,
                t0: t - 10.0,
                phase: phase + 1.3 * (k + 1) as f32,
                puff_t0: None,
                kitten: true,
            });
        }
        c.entity(root).insert(ColumnRoot {
            sid: col.sess.id.clone(),
            look,
        });
        c.entity(root).insert(parts);
        // collar colour + dim fur
        let _ = &col.collar;
    }
    // rugs and context labels: rebuilt with the layout
    for e in &rugs {
        c.entity(e).despawn();
    }
    for e in &labels {
        c.entity(e).despawn();
    }
    let ctxs = st.office.ctxs();
    let spans = strip::ctx_spans(&ctxs, &st.xs);
    for (i, s) in spans.iter().enumerate() {
        let color = st.office.columns[s.first].ctx_color;
        let mat = mats.add(scene::rug_material(&p, color));
        scene::spawn_rug(&mut c, &kit, mat, s.x0, s.x1);
        let name = if s.ctx.is_empty() {
            "—".to_string()
        } else {
            s.ctx.clone()
        };
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(px(10), px(3)),
                border_radius: BorderRadius::all(px(9)),
                ..default()
            },
            BackgroundColor(rgb(p.context(color))),
            CtxLabel(i),
            ChildOf(*layer),
            children![(Text::new(name), tfont(&st, 15.0), TextColor(rgb(p.crust)))],
        ));
    }
    st.chrome_key.clear();
}

fn look_key(c: &crate::model::Column) -> String {
    format!(
        "{}|{:?}|{}|{}",
        c.sess.running_subs.min(3),
        c.collar,
        c.sess.dim,
        c.ctx_color
    )
}

fn layout_key(st: &St) -> String {
    let mut k = String::new();
    for (i, c) in st.office.columns.iter().enumerate() {
        k.push_str(&format!("{}@{:.2}:{};", c.sess.id, st.xs[i], look_key(c)));
    }
    k
}

// ---------------------------------------------------------------- camera

fn camera(
    mut st: ResMut<St>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<&mut Transform, With<CamMarker>>,
    mut resized: MessageReader<WindowResized>,
) {
    let t = now_s(&time);
    let x = st.xs.get(st.nav.focus).copied().unwrap_or(0.0);
    let aspect = window.width() / window.height().max(1.0);
    let target = if st.nav.zoomed() {
        strip::zoom_pose(x, aspect, ZOOM_FILL)
    } else {
        strip::office_pose(x, aspect)
    };
    let dur = if st.reduced { 0.0 } else { CAM_S };
    let resized_now = resized.read().count() > 0;
    if resized_now {
        st.tween = Tween::still(target);
    } else {
        st.tween = st.tween.retarget(t, target, dur);
    }
    st.zoom_target = st.nav.zoomed();
    let pose: CamPose = st.tween.at(t);
    **cam = Transform::from_translation(pose.eye).looking_at(pose.look, Vec3::Y);
    if !st.tween.done(t) {
        st.animating_until = st.animating_until.max(t + 0.05);
    }
}

// ---------------------------------------------------------------- cats

#[allow(clippy::too_many_arguments)]
fn animate_cats(
    st: Res<St>,
    time: Res<Time<Real>>,
    m: Res<Mats>,
    mut cats: Query<(&mut CatAnim, &Rig)>,
    parts: Query<&ColumnParts>,
    mut tfs: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut matq: Query<&mut MeshMaterial3d<StandardMaterial>>,
    icons: Query<&scene::Icon>,
) {
    let t = now_s(&time);
    let by_id: HashMap<&str, &crate::model::Column> = st
        .office
        .columns
        .iter()
        .map(|c| (c.sess.id.as_str(), c))
        .collect();
    let focus_sid = focused_sid(&st);
    for (mut a, rig) in &mut cats {
        let Some(col) = by_id.get(a.sid.as_str()) else {
            continue;
        };
        let pose = if a.kitten {
            Pose::Idle
        } else {
            col.sess.pose()
        };
        if pose != a.pose {
            // tween from wherever we are
            let cur = current_params(&a, t, st.reduced);
            a.from = cur;
            a.pose = pose;
            a.t0 = t;
        }
        if let Some(p) = st.puffs.get(&a.sid)
            && a.puff_t0.is_none_or(|x| x < *p)
            && !a.kitten
        {
            a.puff_t0 = Some(*p);
        }
        let mut params = current_params(&a, t, st.reduced);
        let calm = matches!(pose, Pose::Idle | Pose::Sleeping | Pose::Puff);
        if !st.reduced && (!calm || catpose::in_bout(t as f32, a.phase)) {
            params = catpose::ambient(pose, params, t as f32, a.phase);
        }
        let mut puffing = false;
        if let Some(p0) = a.puff_t0 {
            let u = ((t - p0) as f32) / catpose::PUFF_S;
            if u < 1.0 {
                params = catpose::puff_at(params, u);
                puffing = true;
            }
        }
        let rp = scene::rig_transforms(&params);
        if let Ok(mut tf) = tfs.get_mut(rig.body) {
            *tf = rp.body;
        }
        if let Ok(mut tf) = tfs.get_mut(rig.head) {
            *tf = rp.head;
        }
        for (i, e) in rig.ears.iter().enumerate() {
            if let Ok(mut tf) = tfs.get_mut(*e) {
                *tf = rp.ears[i];
            }
        }
        for (i, e) in rig.legs.iter().enumerate() {
            if let Ok(mut tf) = tfs.get_mut(*e) {
                tf.rotation = rp.legs[i];
            }
        }
        for (i, e) in rig.tail.iter().enumerate() {
            if let Ok(mut tf) = tfs.get_mut(*e) {
                tf.rotation = if i == 0 { rp.tail0 } else { rp.tail_curl };
                tf.scale = rp.tail_scale;
            }
        }
        for e in rig.eyes {
            if let Ok(mut tf) = tfs.get_mut(e) {
                tf.scale = rp.eye_scale;
            }
        }
        // collar & fur material
        if let Ok(mut v) = vis.get_mut(rig.collar) {
            *v = if col.collar.is_some() && !a.kitten {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        let fur = if puffing {
            &m.fur_puff
        } else if col.sess.dim {
            &m.fur_dim
        } else {
            &m.fur
        };
        for e in &rig.fur {
            if let Ok(mut mm) = matq.get_mut(*e)
                && mm.0 != *fur
            {
                mm.0 = fur.clone();
            }
        }
        let _ = &icons;
    }
    // per column: icons, keyboard glow, focus bezel, collar colour
    for parts in &parts {
        let Some(col) = by_id.get(parts.sid.as_str()) else {
            continue;
        };
        let pose = col.sess.pose();
        let bob = if st.reduced {
            0.0
        } else {
            0.05 * (t as f32 * 3.0 + catpose::phase_of(&parts.sid)).sin()
        };
        for (e, show) in [
            (parts.bang, pose == Pose::Alert),
            (parts.zz, pose == Pose::Sleeping),
        ] {
            if let Ok(mut v) = vis.get_mut(e) {
                let want = if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *v != want {
                    *v = want;
                }
            }
            if show && let (Ok(icon), Ok(mut tf)) = (icons.get(e), tfs.get_mut(e)) {
                tf.translation.y = icon.base_y + bob;
            }
        }
        if let Ok(mut mm) = matq.get_mut(parts.keyboard) {
            let want = if pose == Pose::Typing {
                &m.kb_on
            } else {
                &m.kb
            };
            if mm.0 != *want {
                mm.0 = want.clone();
            }
        }
        if let Ok(mut mm) = matq.get_mut(parts.bezel) {
            let want = if focus_sid.as_deref() == Some(parts.sid.as_str()) {
                &m.bezel_focus
            } else {
                &m.bezel
            };
            if mm.0 != *want {
                mm.0 = want.clone();
            }
        }
    }
}

fn current_params(a: &CatAnim, t: f64, reduced: bool) -> CatParams {
    let target = catpose::pose_params(a.pose);
    if reduced {
        return target;
    }
    let u = ((t - a.t0) as f32 / catpose::TWEEN_S).clamp(0.0, 1.0);
    catpose::lerp(a.from, target, strip::ease_in_out(u))
}

// ---------------------------------------------------------------- monitors

fn role_color(p: &Palette, r: Role) -> Rgb {
    match r {
        Role::Tool => p.accent,
        Role::Text => p.text,
        Role::User => p.subtle,
        Role::Result => p.muted,
        Role::Sub => p.accent2,
        Role::Add => p.done,
        Role::Del => p.blocked,
    }
}

fn state_color(p: &Palette, s: &Sess) -> Rgb {
    match s.pose() {
        Pose::Typing => p.busy,
        Pose::Alert => p.blocked,
        Pose::Sleeping => p.done,
        Pose::Puff => p.error,
        Pose::Idle => p.idle,
    }
}

fn state_word(s: &Sess) -> &'static str {
    match s.pose() {
        Pose::Typing => "rodando",
        Pose::Alert => "te esperando",
        Pose::Sleeping => "voltou",
        Pose::Puff => "erro",
        Pose::Idle => "parada",
    }
}

fn monitors(mut st: ResMut<St>, mut faces: ResMut<Faces>, mut images: ResMut<Assets<Image>>) {
    let now = now_ms();
    let mut st_wake: Option<i64> = None;
    let p = st.palette.clone();
    let cols = st.office.columns.clone();
    for col in &cols {
        let has_pane = st.panes.contains_key(&col.sess.id);
        let Some(m) = st.monitors.get_mut(&col.sess.id) else {
            continue;
        };
        if m.feed.tick(now) {
            m.dirty = true;
        }
        if let Some(at) = m.feed.next_reveal(now) {
            st_wake = Some(st_wake.map_or(at, |w: i64| w.min(at)));
        }
        if !m.dirty || has_pane {
            continue;
        }
        m.dirty = false;
        let face = &mut faces.mon;
        let mut cv = Canvas::new(MON_W, MON_H, mix(p.bg, p.cat_body, 0.5));
        let cols_n = MON_W / face.cw - 2;
        let rows_n = MON_H / face.ch;
        // header: state pill + label
        let sc = state_color(&p, &col.sess);
        cv.rect(0, 0, MON_W as i32, face.ch as i32 + 6, mix(p.bg, sc, 0.18));
        cv.rect(0, face.ch as i32 + 6, MON_W as i32, 2, sc);
        let word = state_word(&col.sess);
        let used = cv.text(face, 1, 0, &format!("● {word}"), sc, true);
        let label = monitor::clip(&col.sess.label, cols_n.saturating_sub(used + 3));
        cv.text(face, used + 3, 0, &label, p.text, true);
        // body: the tail, bottom-aligned
        let body_rows = rows_n.saturating_sub(2);
        let lines = m.feed.visible(body_rows);
        let y0 = rows_n - lines.len();
        for (i, l) in lines.iter().enumerate() {
            let txt = monitor::clip(&l.text, cols_n);
            let row = y0 + i;
            draw_line(&mut cv, face, row, &txt, role_color(&p, l.role), 6);
        }
        if lines.is_empty() {
            cv.text(face, 1, 3, "(sem atividade ainda)", p.muted, false);
        }
        // a blinking-ish cursor block after the last line when busy
        if col.sess.pose() == Pose::Typing {
            let row = rows_n - 1;
            let x = (lines
                .last()
                .map(|l| monitor::clip(&l.text, cols_n).chars().count())
                .unwrap_or(0)
                + 1)
            .min(cols_n)
                * face.cw;
            cv.rect(
                x as i32,
                (row * face.ch) as i32 + 3,
                face.cw as i32,
                face.ch as i32 - 4,
                p.busy,
            );
        }
        if let Some(mut img) = images.get_mut(&m.image) {
            img.data = Some(cv.px);
        }
    }
    st.wake_ms = min_wake(st.wake_ms, st_wake);
}

/// The earlier of two wake-up times (ms).
fn min_wake(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// One tail line at a pixel offset (rows start a few px below the header).
fn draw_line(cv: &mut Canvas, face: &mut Face, row: usize, txt: &str, color: Rgb, dy: i32) {
    let mut x = face.cw as i32;
    let y = (row * face.ch) as i32 - dy + 8;
    for ch in txt.chars() {
        cv.glyph(face, x, y, ch, color, false);
        x += (monitor::char_width(ch) * face.cw) as i32;
    }
}

fn panes(
    mut st: ResMut<St>,
    mut faces: ResMut<Faces>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time<Real>>,
) {
    let t = now_s(&time);
    let focus = focused_sid(&st);
    let zoomed = st.nav.zoomed();
    let mode = st.nav.mode;
    let accent = st.palette.accent;
    let mut busy = false;
    let mut wake: Option<i64> = None;
    let sids: Vec<String> = st.panes.keys().cloned().collect();
    for sid in sids {
        let is_focus = focus.as_deref() == Some(sid.as_str());
        let p = st.panes.get_mut(&sid).unwrap();
        let changed = p.term.changed();
        p.dirty |= changed;
        if !p.dirty {
            continue;
        }
        let interval = if is_focus && zoomed { 1.0 / 60.0 } else { 0.1 };
        if t - p.last_draw < interval {
            let due = now_ms() + ((p.last_draw + interval - t) * 1000.0).ceil() as i64;
            wake = min_wake(wake, Some(due));
            continue;
        }
        p.dirty = false;
        p.last_draw = t;
        let g = p.term.grid();
        let face = &mut faces.term;
        let (w, h) = (g.cols as usize * face.cw, g.rows as usize * face.ch);
        let mut cv = Canvas::new(w.max(1), h.max(1), [0, 0, 0]);
        let cells: Vec<TCell> = g
            .cells
            .iter()
            .map(|c| TCell {
                ch: c.ch,
                fg: c.fg,
                bg: c.bg,
                bold: c.bold,
                underline: c.underline,
                inverse: c.inverse,
                wide: c.wide,
            })
            .collect();
        raster::draw_grid(
            &mut cv,
            face,
            g.cols as usize,
            g.rows as usize,
            &cells,
            g.cursor.map(|(x, y)| (x as usize, y as usize)),
            accent,
            is_focus && mode == Mode::Insert,
        );
        if let Some(mut img) = images.get_mut(&p.image) {
            if img.width() as usize == cv.w && img.height() as usize == cv.h {
                img.data = Some(cv.px);
            } else {
                *img = canvas_image(cv);
            }
        }
        busy = true;
    }
    if busy {
        st.animating_until = st.animating_until.max(t + 0.12);
    }
    st.wake_ms = min_wake(st.wake_ms, wake);
}

// ---------------------------------------------------------------- inbox cats

#[allow(clippy::too_many_arguments)]
fn walkers(
    mut c: Commands,
    mut st: ResMut<St>,
    kit: Res<Kit>,
    m: Res<Mats>,
    time: Res<Time<Real>>,
    rigs: Query<(Entity, &WalkerRig)>,
    mut tfs: Query<&mut Transform>,
    mut badge: Single<(&mut Visibility, &Children), With<OverflowBadge>>,
    mut texts: Query<&mut Text>,
) {
    let now = now_ms();
    let t = now_s(&time);
    let cam_x = st.tween.at(t).eye.x;
    let start_x = if st.reduced { f32::NAN } else { cam_x - 4.2 };
    let xs: HashMap<String, f32> = st
        .office
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.sess.id.clone(), st.xs[i]))
        .collect();
    let x_of = |id: &str| xs.get(id).copied();
    let src: Vec<inbox::Source> = st
        .sessions
        .iter()
        .filter(|s| xs.contains_key(&s.id))
        .map(|s| inbox::Source {
            session_id: s.id.clone(),
            label: s.label.clone(),
            attention: s.attention,
            archived: s.archived,
            last_at_ms: s.last_at_ms,
            needs: s.needs.clone(),
        })
        .collect();
    let mail: inbox::Mail = st
        .mail
        .iter()
        .filter(|(k, _)| xs.contains_key(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let mut q = inbox::step(
        &st.queue,
        &src,
        &mail,
        now,
        if start_x.is_nan() { 0.0 } else { start_x },
        &x_of,
    );
    if st.reduced {
        // no walking: everyone is already at their stop
        for w in &mut q.walkers {
            if w.start_at.is_some() {
                w.from = w.to;
            }
        }
    }
    // never show a walker for the session we're zoomed into
    if st.nav.zoomed()
        && let Some(sid) = focused_sid(&st)
    {
        q = inbox::dismiss_session(&q, &sid);
    }
    st.queue = q;
    // entities: one per visible walker
    let vis: Vec<inbox::Walker> = st.queue.visible().into_iter().cloned().collect();
    let keys: HashSet<&str> = vis.iter().map(|w| w.key.as_str()).collect();
    let mut have: HashMap<String, Entity> = HashMap::new();
    for (e, r) in &rigs {
        if keys.contains(r.key.as_str()) {
            have.insert(r.key.clone(), e);
        } else {
            c.entity(e).despawn();
        }
    }
    for (i, w) in vis.iter().enumerate() {
        let x = inbox::x_at(w, now);
        let z = strip::AISLE_Z + (i % 2) as f32 * 0.25;
        let phase = inbox::phase_of(w, now);
        let walking = matches!(phase, inbox::Phase::Walking);
        let e = match have.get(&w.key) {
            Some(e) => *e,
            None => {
                let icon = match w.kind {
                    inbox::Kind::Blocked => IconKind::Bang,
                    inbox::Kind::Returned => IconKind::BangDone,
                    inbox::Kind::Mail => IconKind::Mail,
                };
                scene::spawn_walker(&mut c, &kit, &m, &w.key, icon, Vec3::new(x, 0.0, z))
            }
        };
        if let Ok(mut tf) = tfs.get_mut(e) {
            let (_, bob) = catpose::walk_cycle(t as f32);
            tf.translation = Vec3::new(x, if walking { bob } else { 0.0 }, z);
            let dir = (w.to - w.from).signum();
            let yaw = match phase {
                inbox::Phase::Arrived | inbox::Phase::Queued => -std::f32::consts::FRAC_PI_2 * 0.8,
                _ if dir < 0.0 => std::f32::consts::PI,
                _ => 0.0,
            };
            tf.rotation = Quat::from_rotation_y(yaw);
            let hl = st
                .queue
                .highlight
                .as_ref()
                .is_some_and(|h| h.0 == w.session_id);
            tf.scale = Vec3::splat(if hl { WALKER_HL } else { WALKER_SCALE });
        }
        st.walker_ents.insert(w.key.clone(), e);
    }
    // legs
    for (_, r) in &rigs {
        let Some(w) = vis.iter().find(|w| w.key == r.key) else {
            continue;
        };
        let walking = matches!(inbox::phase_of(w, now), inbox::Phase::Walking);
        let (legs, _) = if walking {
            catpose::walk_cycle(t as f32)
        } else {
            ([0.0; 4], 0.0)
        };
        for (i, e) in r.legs.iter().enumerate() {
            if let Ok(mut tf) = tfs.get_mut(*e) {
                tf.rotation = Quat::from_rotation_z(legs[i]);
            }
        }
        if let Ok(mut tf) = tfs.get_mut(r.tail) {
            let sway = if st.reduced {
                0.0
            } else {
                0.3 * (t as f32 * 4.0).sin()
            };
            tf.rotation = Quat::from_rotation_z(0.5) * Quat::from_rotation_x(sway);
        }
        if let Ok(mut tf) = tfs.get_mut(r.icon) {
            tf.rotation = Quat::from_rotation_y(-tf_parent_yaw(w, now));
        }
    }
    // +N
    let n = st.queue.overflow();
    let (vis_b, children) = &mut *badge;
    **vis_b = if n > 0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Some(ch) = children.first()
        && let Ok(mut tx) = texts.get_mut(*ch)
    {
        let s = format!("+{n} na inbox");
        if tx.0 != s {
            tx.0 = s;
        }
    }
    // walking cats: WALK_FPS is smooth enough for low-poly legs, and half the
    // cost of redrawing every vsync
    if inbox::moving(&st.queue, now) {
        st.wake_ms = min_wake(st.wake_ms, Some(now + 1000 / WALK_FPS));
    }
}

const WALK_FPS: i64 = 30;

/// The yaw the walker root has (so its icon can face the camera).
fn tf_parent_yaw(w: &inbox::Walker, now: i64) -> f32 {
    match inbox::phase_of(w, now) {
        inbox::Phase::Arrived | inbox::Phase::Queued => -std::f32::consts::FRAC_PI_2 * 0.8,
        _ if w.to < w.from => std::f32::consts::PI,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------- chrome

fn chrome(
    mut c: Commands,
    mut st: ResMut<St>,
    top: Single<(Entity, &mut Visibility), (With<TopBar>, Without<StatusBar>)>,
    status: Single<(Entity, &mut Visibility), (With<StatusBar>, Without<TopBar>)>,
    mut prompt: Single<
        (&mut Visibility, &Children),
        (With<PromptBox>, Without<TopBar>, Without<StatusBar>),
    >,
    mut texts: Query<&mut Text>,
) {
    let p = st.palette.clone();
    let (running, waiting, back) = Office::counts(&st.sessions);
    let mode = match st.nav.mode {
        Mode::Office => "NORMAL",
        Mode::Zoom => "ZOOM",
        Mode::Insert => "INSERT",
    };
    let hints = Router::hints(&st.binds, st.nav.zoomed());
    let toast = st.toast.clone();
    let key = format!(
        "{running}|{waiting}|{back}|{:?}|{:.2}|{mode}|{:?}|{:?}|{}|{}|{:?}|{}",
        st.usage_5h,
        st.today,
        st.router.pending(),
        toast.as_ref().map(|t| (&t.0, t.1)),
        st.status,
        st.bars,
        st.prompt.as_ref().map(|p| &p.1),
        st.version
    );
    // prompt
    {
        let (vis, children) = &mut *prompt;
        **vis = if st.prompt.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let (Some(q), Some(ch)) = (st.prompt.as_ref(), children.first())
            && let Ok(mut t) = texts.get_mut(*ch)
            && t.0 != q.1
        {
            t.0 = q.1.clone();
        }
    }
    if key == st.chrome_key {
        return;
    }
    st.chrome_key = key;
    let (top_e, mut top_v) = top.into_inner();
    let (status_e, mut status_v) = status.into_inner();
    *top_v = if st.bars {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    *status_v = if st.bars || st.nav.mode == Mode::Insert {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    c.entity(top_e).despawn_related::<Children>();
    c.entity(status_e).despawn_related::<Children>();
    let f = |sz: f32| tfont(&st, sz);
    let txt = |s: String, col: Rgb, sz: f32| (Text::new(s), f(sz), TextColor(rgb(col)));
    c.entity(top_e).with_children(|b| {
        b.spawn(txt("=^..^= nekuroddo".into(), p.accent, 16.0));
        b.spawn(txt(format!("● {running} rodando"), p.busy, 15.0));
        b.spawn(txt(
            format!("● {waiting} te esperando"),
            if waiting > 0 { p.blocked } else { p.muted },
            15.0,
        ));
        b.spawn(txt(
            format!("● {back} voltaram"),
            if back > 0 { p.done } else { p.muted },
            15.0,
        ));
        b.spawn((Node {
            flex_grow: 1.0,
            ..default()
        },));
        if !st.status.is_empty() {
            b.spawn(txt(st.status.clone(), p.error, 14.0));
        }
        if let Some(pct) = st.usage_5h {
            let n = (pct.clamp(0, 100) as usize + 5) / 10;
            let bar: String = "▮".repeat(n) + &"▯".repeat(10 - n);
            let col = mix(p.done, p.blocked, pct as f32 / 100.0);
            b.spawn(txt(format!("5h {bar} {pct}%"), col, 15.0));
        }
        if st.today > 0.0 {
            b.spawn(txt(format!("${:.2} hoje", st.today), p.subtle, 15.0));
        }
    });
    c.entity(status_e).with_children(|b| {
        let pill = match st.nav.mode {
            Mode::Office => p.accent,
            Mode::Zoom => p.accent2,
            Mode::Insert => p.done,
        };
        b.spawn((
            Node {
                padding: UiRect::axes(px(9), px(1)),
                border_radius: BorderRadius::all(px(7)),
                ..default()
            },
            BackgroundColor(rgb(pill)),
            children![(Text::new(mode), f(14.0), TextColor(rgb(p.crust)))],
        ));
        if st.nav.mode == Mode::Insert {
            b.spawn(txt(
                "ctrl+] sai do INSERT · ctrl+shift+v cola".into(),
                p.subtle,
                14.0,
            ));
        } else {
            for (k, d) in &hints {
                b.spawn(txt(format!("{k} {d}"), p.subtle, 14.0));
            }
        }
        if !st.router.pending().is_empty() {
            b.spawn(txt(st.router.pending().join(""), p.accent, 14.0));
        }
        b.spawn((Node {
            flex_grow: 1.0,
            ..default()
        },));
        if let Some((msg, err, _)) = &toast {
            b.spawn(txt(msg.clone(), if *err { p.error } else { p.text }, 14.0));
        }
    });
}

/// Context labels follow the camera; the zoomed terminal sits on its monitor.
#[allow(clippy::too_many_arguments)]
fn overlay(
    mut st: ResMut<St>,
    mut faces: ResMut<Faces>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<CamMarker>>,
    mut labels: Query<(&CtxLabel, &mut Node, &mut Visibility), Without<TermOverlay>>,
    term: Single<
        (
            &mut ImageNode,
            &mut Node,
            &mut BackgroundColor,
            &mut Visibility,
        ),
        With<TermOverlay>,
    >,
) {
    let t = now_s(&time);
    let (camera, ctf) = *cam;
    let vp = Vec2::new(window.width(), window.height());
    faces.vp = vp;
    let sf = window.scale_factor();
    if (faces.term_scale - sf).abs() > 1e-3 {
        faces.term_scale = sf;
        let fd = st.fontdue.clone();
        faces.term = Face::new(&fd, TERM_FONT * sf);
        faces.mon = Face::new(&fd, MON_FONT);
    }
    let ctxs = st.office.ctxs();
    let spans = strip::ctx_spans(&ctxs, &st.xs);
    let zoomed = st.nav.zoomed();
    for (l, mut node, mut v) in &mut labels {
        let Some(s) = spans.get(l.0) else { continue };
        let at = Vec3::new(s.x0 + 0.15, 0.03, strip::CHAIR_Z + 0.15);
        match camera.world_to_viewport(ctf, at) {
            Ok(p) if !zoomed && p.x > -200.0 && p.x < vp.x => {
                node.left = px(p.x.max(8.0));
                node.top = px((p.y - 12.0).min(vp.y - STATUS_H - 60.0));
                *v = Visibility::Inherited;
            }
            _ => *v = Visibility::Hidden,
        }
    }
    // the crisp terminal, once the camera has arrived
    let (mut img, mut node, mut bg, mut v) = term.into_inner();
    let sid = focused_sid(&st);
    let pane = sid.as_ref().and_then(|s| st.panes.get(s));
    let arrived = st.tween.done(t) && zoomed;
    let Some(p) = pane.filter(|_| arrived) else {
        *v = Visibility::Hidden;
        return;
    };
    let x = st.xs.get(st.nav.focus).copied().unwrap_or(0.0);
    let pts: Vec<Vec2> = strip::screen_corners(x)
        .iter()
        .filter_map(|c| camera.world_to_viewport(ctf, *c).ok())
        .collect();
    if pts.len() < 4 {
        *v = Visibility::Hidden;
        return;
    }
    let (lo, hi) = strip::bounds(&pts);
    let rect = hi - lo;
    // resize the pane to the screen rect (in cells)
    let cell = Vec2::new(faces.term.cw as f32, faces.term.ch as f32);
    let (cols, rows) = strip::fit_cells(rect * sf, cell);
    let image = p.image.clone();
    if cols != p.cols || rows != p.rows {
        let sid = sid.clone().unwrap();
        let p = st.panes.get_mut(&sid).unwrap();
        p.cols = cols;
        p.rows = rows;
        p.term.resize(cols, rows);
        p.conn.lock().unwrap().resize(cols, rows);
        p.dirty = true;
    }
    // the node covers the whole screen face (so the 3D monitor, which shows
    // the same grid stretched, never peeks out at the edges); the whole-cell
    // grid sits centred in it, the margin painted the terminal's background
    let size = Vec2::new(cols as f32 * cell.x, rows as f32 * cell.y) / sf;
    let pad = ((rect - size) / 2.0).max(Vec2::ZERO).floor();
    node.left = px(lo.x.floor());
    node.top = px(lo.y.floor());
    node.width = px(size.x + 2.0 * pad.x);
    node.height = px(size.y + 2.0 * pad.y);
    node.padding = UiRect::axes(px(pad.x), px(pad.y));
    let want = rgb(st.palette.bg);
    if bg.0 != want {
        bg.0 = want;
    }
    if img.image != image {
        img.image = image;
    }
    *v = Visibility::Inherited;
}

// ---------------------------------------------------------------- power + shots

fn power(
    mut st: ResMut<St>,
    time: Res<Time<Real>>,
    mut settings: ResMut<WinitSettings>,
    mut redraw: MessageWriter<RequestRedraw>,
    cli: Res<CliRes>,
) {
    let t = now_s(&time);
    if cli.0.shot.is_some() {
        redraw.write(RequestRedraw);
        return;
    }
    if t < st.animating_until {
        redraw.write(RequestRedraw);
        return;
    }
    // ambient motion: busy/alert cats move at AMBIENT_FPS, calm ones in bouts
    let mut wait = Duration::from_secs(30);
    if !st.reduced {
        let mut active = false;
        let mut next_bout = f32::MAX;
        for c in &st.office.columns {
            match c.sess.pose() {
                Pose::Typing | Pose::Alert => active = true,
                _ => {
                    let ph = catpose::phase_of(&c.sess.id);
                    if catpose::in_bout(t as f32, ph) {
                        active = true;
                    } else {
                        // time until this cat's next bout
                        let off = ph / std::f32::consts::TAU * catpose::BOUT_EVERY_S;
                        let r = (t as f32 + off).rem_euclid(catpose::BOUT_EVERY_S);
                        next_bout = next_bout.min(catpose::BOUT_EVERY_S - r);
                    }
                }
            }
            if c.sess.running_subs > 0 {
                active = true;
            }
        }
        if !st.queue.walkers.is_empty() {
            active = true; // walkers' tails and the icons bob
        }
        if active {
            wait = Duration::from_secs_f32(1.0 / catpose::AMBIENT_FPS);
        } else if next_bout < 1e6 {
            wait = Duration::from_secs_f32(next_bout.max(0.05));
        }
    }
    if let Some(w) = st.wake_ms.take() {
        wait = wait.min(Duration::from_millis((w - now_ms()).max(10) as u64));
    }
    if let Some(w) = inbox::next_wake(&st.queue, now_ms()) {
        wait = wait.min(Duration::from_millis((w - now_ms()).max(10) as u64));
    }
    if st.toast.is_some() {
        wait = wait.min(Duration::from_millis(500));
    }
    let mode = UpdateMode::reactive_low_power(wait);
    if settings.focused_mode != mode {
        settings.focused_mode = mode;
        settings.unfocused_mode = mode;
    }
    let _ = &mut st;
}

/// How long the inbox cats walk before an inbox shot.
const INBOX_SHOT_S: f64 = 1.8;

fn shot(mut c: Commands, mut st: ResMut<St>, cli: Res<CliRes>, time: Res<Time<Real>>) {
    let Some(path) = cli.0.shot.clone() else {
        return;
    };
    if !st.loaded {
        return;
    }
    st.shot_frame += 1;
    let f = st.shot_frame;
    let t = now_s(&time);
    if st.shot_stage == 0 {
        st.shot_stage = 1;
        if let Some(i) = cli.0.focus {
            let fx = st.nav.apply(Act::Focus(i));
            st.effects.extend(fx);
        }
    }
    if st.shot_stage == 1 && cli.0.shot_zoom && f > cli.0.frames / 2 {
        st.shot_stage = 2;
        let fx = st.nav.apply(Act::Zoom);
        st.effects.extend(fx);
        let fx = st.nav.apply(Act::Insert);
        st.effects.extend(fx);
    }
    // --shot-inbox: once the office is drawn, send the inbox cats out again
    // and shoot while they walk in
    if cli.0.shot_inbox && st.shot_stage < 3 && f > cli.0.frames {
        match st.shot_t0 {
            None => {
                st.shot_t0 = Some(t);
                st.queue = inbox::Queue::default();
                return;
            }
            Some(t0) if t - t0 < INBOX_SHOT_S => return,
            Some(_) => {}
        }
    }
    if st.shot_stage < 3 && f > cli.0.frames {
        st.shot_stage = 3;
        c.spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
}
