//! Launching apps the right way (S22) and the WM's two phone apps: every
//! child gets the launcher's environment (`launch_env`), X11 through the
//! satellite on demand (`xwayland`), the "já está aberto fora do mundo"
//! warning for single-instance apps and their isolated profiles; the
//! phone's "apps" (Apps) and "janelas" (Janelas).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use nkw_core::{ScreenId, Time};

use super::Wm;
use super::launch_env::{self, env_for, isolatable, isolated_argv, program};
use super::xwayland::{self, Ensure, Poll};
use crate::ext::{Ctx, Req, ToastKind};
use crate::ui::Icon;
use crate::ui::phone::{Act, Item, PhoneApp, PhonePage, PhoneState, SearchSpec};

/// The phone's apps of the WM.
pub const APP_APPS: &str = "apps";
pub const APP_JANELAS: &str = "janelas";
/// `world.launch_isolated {id}`.
pub const LAUNCH_ISOLATED: &str = "world.launch_isolated";
/// `world.xwayland {on}`.
pub const XWAYLAND: &str = "world.xwayland";
/// `world.tour_done` (nkw-only): the receptionist's tour was seen.
pub const TOUR_DONE: &str = "world.tour_done";
/// `world.close_screen {screen}` (nkw-only: the Janelas app's `Q`).
pub const CLOSE_SCREEN: &str = "world.close_screen";
/// `world.send_screen {screen}` (nkw-only: the Janelas app's `M`): select
/// it, then the palette asks the room of `world.send_room`.
pub const SEND_SCREEN: &str = "world.send_screen";
/// The tour starts this long after the first tick (S24).
pub const TOUR_AFTER_S: Time = 1.5;
/// The Apps page's "aberto lá fora" is re-checked at most this often.
pub const OUTSIDE_EVERY_S: Time = 2.0;

/// The toast of a launch that will open outside.
pub fn outside_toast(app: &str) -> String {
    format!(
        "o {app} já está aberto fora do mundo: a janela vai abrir lá fora · \
         telefone › Apps › abrir isolado"
    )
}

/// A desktop id as a directory name (no separators, no dot-only names).
pub fn profile_name(id: &str) -> Option<String> {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    (!s.is_empty() && s.chars().any(|c| c != '.')).then_some(s)
}

/// `<root>/profiles/<id>/`, made 0700 (both levels).
pub fn profile_dir(root: &Path, id: &str) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let name = profile_name(id)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "id"))?;
    let profiles = root.join("profiles");
    let dir = profiles.join(name);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    for d in [&profiles, &dir] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

fn toast(text: String, kind: ToastKind) -> Req {
    Req::Toast {
        text,
        kind,
        ms: 6000,
    }
}

impl Wm {
    /// Spawn `argv` with the launcher's env; X11 first when it is on the
    /// way (the child then waits for the satellite's socket).
    pub(super) fn spawn_reqs(&mut self, argv: Vec<String>, now: Time) -> Vec<Req> {
        let cfg = &self.config.launch;
        match self.xwayland.ensure(now) {
            Ensure::Ready(d) => vec![Req::SpawnWith {
                env: env_for(cfg, d.as_deref()),
                argv,
            }],
            Ensure::Wait => {
                self.x_waiting.push(argv);
                Vec::new()
            }
            Ensure::Start(sat) => {
                self.x_waiting.push(argv);
                vec![Req::SpawnWith {
                    argv: sat,
                    env: Vec::new(),
                }]
            }
            Ensure::Missing(t) => vec![
                toast(t, ToastKind::Error),
                Req::SpawnWith {
                    env: env_for(cfg, None),
                    argv,
                },
            ],
        }
    }

    /// `world.launch:<id>` / `world.launch {exec}`: the outside warning for
    /// an isolatable app already running elsewhere (the spawn still
    /// happens: the user asked for it).
    pub(super) fn launch(
        &mut self,
        id: &str,
        name: &str,
        argv: Vec<String>,
        now: Time,
    ) -> Vec<Req> {
        let mut v = Vec::new();
        if isolatable(&self.config.launch, id, &argv)
            && let (Some(root), Some(prog)) = (self.proc_root.as_deref(), program(&argv))
            && launch_env::running_outside(prog, &self.socket, root)
        {
            v.push(toast(outside_toast(name), ToastKind::Info));
        }
        v.extend(self.spawn_reqs(argv, now));
        v
    }

    /// `world.launch_isolated {id}`: the app with a profile of its own
    /// under `<state-dir>/profiles/<id>/` (never under `--demo`).
    pub(super) fn launch_isolated(
        &mut self,
        args: &BTreeMap<String, String>,
        now: Time,
    ) -> Option<Vec<Req>> {
        let id = args.get("id")?.trim().to_string();
        let app = self.apps().iter().find(|a| a.id == id)?.clone();
        if self.demo {
            return Some(vec![toast(
                "no --demo não há perfis isolados".into(),
                ToastKind::Info,
            )]);
        }
        let Some(root) = self.state_root.clone() else {
            return Some(vec![toast(
                "sem diretório de estado para o perfil isolado".into(),
                ToastKind::Error,
            )]);
        };
        let dir = match profile_dir(&root, &id) {
            Ok(d) => d,
            Err(e) => {
                return Some(vec![toast(
                    format!("perfil isolado de {}: {e}", app.name),
                    ToastKind::Error,
                )]);
            }
        };
        let Some(argv) = isolated_argv(&app.argv, &dir) else {
            return Some(vec![toast(
                format!("{} não tem perfil isolado", app.name),
                ToastKind::Info,
            )]);
        };
        Some(self.spawn_reqs(argv, now))
    }

    /// `world.xwayland {on}`: start or stop the satellite now.
    pub(super) fn xwayland_cmd(
        &mut self,
        args: &BTreeMap<String, String>,
        now: Time,
    ) -> Option<Vec<Req>> {
        let on = match args.get("on").map(|s| s.trim()) {
            Some("true" | "on" | "1" | "sim") => true,
            Some("false" | "off" | "0" | "não") => false,
            _ => return None,
        };
        if !on {
            if let Some(pid) = self.xwayland.stop()
                && let Ok(pid) = i32::try_from(pid)
            {
                // our own child (found by its argv on our display)
                // SAFETY: a plain kill(2) on a pid we spawned.
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
            let mut v = vec![toast("xwayland desligado".into(), ToastKind::Info)];
            v.extend(self.release_waiting(None));
            return Some(v);
        }
        if !self.xwayland.enabled() {
            let why = if self.xwayland.binary.is_none() {
                xwayland::MISSING_TOAST
            } else {
                "xwayland desligado (--xwayland off ou [launch] xwayland = \"off\")"
            };
            return Some(vec![toast(why.into(), ToastKind::Error)]);
        }
        if matches!(self.xwayland.state, xwayland::XState::Failed(_)) {
            self.xwayland.stop();
        }
        Some(
            self.xwayland
                .start(now)
                .map(|argv| {
                    vec![Req::SpawnWith {
                        argv,
                        env: Vec::new(),
                    }]
                })
                .unwrap_or_default(),
        )
    }

    fn release_waiting(&mut self, display: Option<&str>) -> Vec<Req> {
        let env = env_for(&self.config.launch, display);
        std::mem::take(&mut self.x_waiting)
            .into_iter()
            .map(|argv| Req::SpawnWith {
                argv,
                env: env.clone(),
            })
            .collect()
    }

    /// From `tick`: the satellite came up (release the waiting children
    /// with `DISPLAY`), gave up (a toast; release them without), or died.
    pub(super) fn xwayland_tick(&mut self, now: Time) -> Vec<Req> {
        match self.xwayland.poll(now) {
            Poll::Nothing => Vec::new(),
            Poll::Up(d) => {
                if let (Some(root), Some(n)) = (self.proc_root.as_deref(), self.xwayland.display) {
                    self.xwayland.pid = xwayland::find_pid(root, n, &self.socket);
                }
                self.release_waiting(Some(&d))
            }
            Poll::Failed(msg) => {
                let mut v = vec![toast(msg, ToastKind::Error)];
                v.extend(self.release_waiting(None));
                v
            }
            Poll::Gone => Vec::new(),
        }
    }

    /// The tour (S24): `world.tour` once, 1.5 s after the first tick, on a
    /// state dir whose `world.json` says it was never seen (never `--demo`).
    pub(super) fn tour_tick(&mut self, now: Time) -> Option<Req> {
        if self.demo || self.tour_done || self.tour_asked {
            return None;
        }
        let t0 = *self.tour_at.get_or_insert(now);
        if now < t0 + TOUR_AFTER_S {
            return None;
        }
        self.tour_asked = true;
        Some(Req::Command {
            name: "world.tour".into(),
            args: BTreeMap::new(),
        })
    }

    /// When the tour wants its tick.
    pub(super) fn tour_wish(&self) -> Option<Time> {
        if self.demo || self.tour_done || self.tour_asked {
            return None;
        }
        self.tour_at.map(|t| t + TOUR_AFTER_S)
    }

    /// `world.tour_done`: remembered in `world.json` (the usual debounce).
    pub(super) fn set_tour_done(&mut self) {
        if !self.tour_done {
            self.tour_done = true;
            self.seen_gen = u64::MAX;
        }
    }

    /// The isolatable apps open outside (cached for [`OUTSIDE_EVERY_S`]).
    fn outside_now(&self, now: Time) -> BTreeSet<String> {
        let Some(root) = self.proc_root.as_deref() else {
            return BTreeSet::new();
        };
        let mut c = self.outside_cache.borrow_mut();
        if let Some((t, set)) = c.as_ref()
            && now >= *t
            && now - *t < OUTSIDE_EVERY_S
        {
            return set.clone();
        }
        let cfg = &self.config.launch;
        let progs: Vec<String> = self
            .apps()
            .iter()
            .filter(|a| isolatable(cfg, &a.id, &a.argv))
            .filter_map(|a| program(&a.argv).map(String::from))
            .collect();
        let refs: Vec<&str> = progs.iter().map(String::as_str).collect();
        let set = launch_env::outside(&refs, &self.socket, root);
        *c = Some((now, set.clone()));
        set
    }

    pub(super) fn phone_apps_of(&self) -> Vec<PhoneApp> {
        vec![
            PhoneApp {
                id: APP_APPS,
                title: "Apps",
                icon: Icon::Grid,
                order: 4,
                badge: None,
            },
            PhoneApp {
                id: APP_JANELAS,
                title: "Janelas",
                icon: Icon::Window,
                order: 5,
                badge: None,
            },
        ]
    }

    pub(super) fn phone_page_of(&self, app: &str, st: &PhoneState, ctx: &Ctx) -> Option<PhonePage> {
        let _ = st;
        match app {
            APP_APPS => Some(self.apps_page(ctx)),
            APP_JANELAS => Some(janelas_page(ctx)),
            _ => None,
        }
    }

    fn apps_page(&self, ctx: &Ctx) -> PhonePage {
        let out = self.outside_now(ctx.now);
        let cfg = &self.config.launch;
        let mut items = vec![Item {
            id: "world.terminal".into(),
            icon: Some(Icon::Terminal),
            title: "terminal novo".into(),
            on_enter: Some(Act::new("world.terminal", &[], true)),
            ..Item::default()
        }];
        for a in self.apps() {
            let iso = isolatable(cfg, &a.id, &a.argv);
            let prog = program(&a.argv).map(|p| p.rsplit('/').next().unwrap_or(p));
            let outside = iso && prog.is_some_and(|p| out.contains(p));
            let detail = match (outside, iso) {
                (true, _) => Some("aberto lá fora · i abre isolado".to_string()),
                (false, true) => Some("i abre isolado".to_string()),
                _ => None,
            };
            items.push(Item {
                id: a.id.clone(),
                icon: Some(Icon::Grid),
                title: a.name.clone(),
                detail,
                badge: outside.then(|| "fora".to_string()),
                on_enter: Some(Act::new(&format!("world.launch:{}", a.id), &[], true)),
                on_keys: if iso {
                    vec![(
                        "i".to_string(),
                        Act::new(LAUNCH_ISOLATED, &[("id", &a.id)], true),
                    )]
                } else {
                    Vec::new()
                },
                ..Item::default()
            });
        }
        PhonePage {
            title: "Apps".into(),
            items,
            empty: Some("nenhum app (.desktop)".into()),
            search: Some(SearchSpec {
                placeholder: "buscar app".into(),
                live: true,
                run: None,
            }),
            keys: vec![
                ("enter".into(), "abre".into()),
                ("i".into(), "abrir isolado".into()),
            ],
            ..PhonePage::default()
        }
    }
}

/// Janelas: every screen with its room · desk · app_id, in strip order.
fn janelas_page(ctx: &Ctx) -> PhonePage {
    let w = ctx.world;
    let mut rows: Vec<((usize, usize, usize), ScreenId)> = w
        .screens
        .keys()
        .map(|id| (w.locate(*id).unwrap_or((usize::MAX, 0, 0)), *id))
        .collect();
    rows.sort();
    let items = rows
        .into_iter()
        .filter_map(|((r, d, _), id)| {
            let s = w.screens.get(&id)?;
            let room = w.rooms.get(r).map_or("no ar", |x| x.name.as_str());
            let title = if s.title.is_empty() {
                s.app_id.clone()
            } else {
                s.title.clone()
            };
            let at = if r == usize::MAX {
                room.to_string()
            } else {
                format!("{room} · mesa {}", d + 1)
            };
            let sid = id.0.to_string();
            Some(Item {
                id: format!("screen:{}", id.0),
                icon: Some(Icon::Window),
                title,
                detail: Some(format!("{at} · {}", s.app_id)),
                on_enter: Some(Act::new(&format!("world.goto_screen:{}", id.0), &[], true)),
                on_keys: vec![
                    (
                        "Q".into(),
                        Act::new(CLOSE_SCREEN, &[("screen", &sid)], false),
                    ),
                    (
                        "M".into(),
                        Act::new(SEND_SCREEN, &[("screen", &sid)], false),
                    ),
                ],
                ..Item::default()
            })
        })
        .collect();
    PhonePage {
        title: "Janelas".into(),
        items,
        empty: Some("nenhuma janela no mundo".into()),
        search: Some(SearchSpec {
            placeholder: "buscar janela".into(),
            live: true,
            run: None,
        }),
        keys: vec![
            ("enter".into(), "ir até".into()),
            ("Q".into(), "fecha".into()),
            ("M".into(), "outra sala".into()),
        ],
        ..PhonePage::default()
    }
}

#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;

/// The palette's rows of the WM's own commands with arguments (S26:
/// palette-only).
pub(super) fn palette_items() -> Vec<crate::ext::PaletteItem> {
    use crate::ext::{ArgKind, ArgSpec, PaletteItem, Source};
    let arg = |name: &str, prompt: &str| ArgSpec {
        name: name.into(),
        prompt: prompt.into(),
        kind: ArgKind::Text,
        required: true,
    };
    vec![
        PaletteItem {
            name: LAUNCH_ISOLATED.into(),
            title: "abrir isolado (perfil próprio)".into(),
            keys: Vec::new(),
            source: Source::World,
            args: vec![arg("id", "app (id) › ")],
        },
        PaletteItem {
            name: XWAYLAND.into(),
            title: "X11 (xwayland-satellite) liga/desliga".into(),
            keys: Vec::new(),
            source: Source::World,
            args: vec![arg("on", "true ou false › ")],
        },
    ]
}

/// `world.close_screen {screen}` / `world.send_screen {screen}`.
pub(super) fn screen_cmd(
    name: &str,
    args: &BTreeMap<String, String>,
    ctx: &Ctx,
) -> Option<Vec<Req>> {
    let id = ScreenId(args.get("screen")?.trim().parse().ok()?);
    if !ctx.world.screens.contains_key(&id) {
        return Some(Vec::new());
    }
    Some(match name {
        CLOSE_SCREEN => vec![Req::Close(id)],
        _ => vec![
            Req::Cmd(crate::world::Cmd::GotoScreen(id)),
            Req::Command {
                name: "world.palette_run".into(),
                args: BTreeMap::from([("name".to_string(), "world.send_room".to_string())]),
            },
        ],
    })
}
