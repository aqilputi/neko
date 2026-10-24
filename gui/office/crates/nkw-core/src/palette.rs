//! COMANDOS (§3.9): a line with a fuzzy-filtered list under it
//! (`nucleo-matcher`), and the arguments of the chosen command asked one
//! after the other in the same line, as in the TUI. Pure: the overlay feeds
//! it the items and the keys and draws [`Palette::ranked`].

use std::cell::RefCell;
use std::collections::BTreeMap;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::line::Line;

/// The most items the list shows.
pub const SHOWN: usize = 12;

/// One entry of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    /// pt-BR
    pub title: String,
    pub keys: Vec<String>,
    /// 0 world, 1 core, 2 apps, 3 goto (ties rank by group, then by order)
    pub group: u8,
}

/// Where the palette is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// typing the query
    Open,
    /// asking the arguments of `name`; `asked` of them answered so far
    Args {
        name: String,
        asked: usize,
    },
    /// run this
    Run {
        name: String,
        args: BTreeMap<String, String>,
    },
    Closed,
}

/// The indices of `texts` that match `query` (fuzzy, smart case), best
/// first, ties in their order; an empty query keeps them all in order.
/// The phone's live search filters its lists with it.
pub fn fuzzy_rank(query: &str, texts: &[&str]) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..texts.len()).collect();
    }
    let mut config = Config::DEFAULT;
    config.prefer_prefix = true;
    let mut m = Matcher::new(config);
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = texts
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            pat.score(Utf32Str::new(t, &mut buf), &mut m)
                .map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|t| t.1).collect()
}

pub struct Palette {
    items: Vec<Item>,
    query: Line,
    /// the highlighted row of `ranked`
    cursor: usize,
    matcher: RefCell<Matcher>,
    step: Step,
    /// the arguments still to ask: (name, prompt)
    specs: Vec<(String, String)>,
    answers: BTreeMap<String, String>,
    /// the query, kept while asking the arguments
    asked_from: String,
}

impl Palette {
    /// The palette over `items` with `prefill` already typed (`world+d`
    /// opens it on the apps).
    pub fn new(items: Vec<Item>, prefill: &str) -> Palette {
        let mut config = Config::DEFAULT;
        config.prefer_prefix = true;
        Palette {
            items,
            query: Line::new(prefill),
            cursor: 0,
            matcher: RefCell::new(Matcher::new(config)),
            step: Step::Open,
            specs: Vec::new(),
            answers: BTreeMap::new(),
            asked_from: String::new(),
        }
    }

    /// The palette already on `name`'s first argument prompt (`world.palette_run
    /// {name, …}`): `specs` are its arguments (name, prompt), `preset` the
    /// answers given beforehand (they ride along into `Step::Run`). No
    /// argument to ask: `Step::Run` at once.
    pub fn ask(
        items: Vec<Item>,
        name: &str,
        specs: Vec<(String, String)>,
        preset: BTreeMap<String, String>,
    ) -> Palette {
        let mut one: Vec<Item> = items.into_iter().filter(|i| i.name == name).collect();
        if one.is_empty() {
            one.push(Item {
                name: name.to_string(),
                title: String::new(),
                keys: Vec::new(),
                group: 0,
            });
        }
        let mut p = Palette::new(one, "");
        p.asked_from = name.to_string();
        p.answers = preset;
        p.step = if specs.is_empty() {
            Step::Run {
                name: name.to_string(),
                args: std::mem::take(&mut p.answers),
            }
        } else {
            Step::Args {
                name: name.to_string(),
                asked: 0,
            }
        };
        p.specs = specs;
        p
    }

    /// Edit the line: the query while `Open` (the highlight goes back to the
    /// top), the answer while asking an argument.
    pub fn edit(&mut self, f: impl FnOnce(&mut Line)) {
        let before = self.query.text.clone();
        f(&mut self.query);
        if self.step == Step::Open && self.query.text != before {
            self.cursor = 0;
        }
    }

    /// The line as it is (query or answer).
    pub fn line(&self) -> &Line {
        &self.query
    }

    /// The highlighted row.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The prompt of the argument being asked.
    pub fn prompt(&self) -> Option<&str> {
        match &self.step {
            Step::Args { asked, .. } => self.specs.get(*asked).map(|s| s.1.as_str()),
            _ => None,
        }
    }

    /// The items that match the query, best first, at most [`SHOWN`]. Ties
    /// keep the group order, then the order the items came in.
    pub fn ranked(&self) -> Vec<&Item> {
        let q = if self.step == Step::Open {
            self.query.text.as_str()
        } else {
            self.asked_from.as_str()
        };
        let mut scored: Vec<(u32, usize, &Item)> = if q.trim().is_empty() {
            self.items
                .iter()
                .enumerate()
                .map(|(i, it)| (0, i, it))
                .collect()
        } else {
            let pat = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
            let mut m = self.matcher.borrow_mut();
            let mut buf = Vec::new();
            self.items
                .iter()
                .enumerate()
                .filter_map(|(i, it)| {
                    let a = pat.score(Utf32Str::new(&it.name, &mut buf), &mut m);
                    let b = pat.score(Utf32Str::new(&it.title, &mut buf), &mut m);
                    a.max(b).map(|s| (s, i, it))
                })
                .collect()
        };
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.2.group.cmp(&b.2.group))
                .then(a.1.cmp(&b.1))
        });
        scored.into_iter().take(SHOWN).map(|t| t.2).collect()
    }

    /// `down`/`ctrl+n` (stays on the last row).
    pub fn down(&mut self) {
        let n = self.ranked().len();
        self.cursor = (self.cursor + 1).min(n.saturating_sub(1));
    }

    /// `up`/`ctrl+p` (stays on the first row).
    pub fn up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// `tab`: the highlighted item's name becomes the query.
    pub fn complete(&mut self) {
        if self.step != Step::Open {
            return;
        }
        let name = self.ranked().get(self.cursor).map(|it| it.name.clone());
        if let Some(name) = name {
            self.query = Line::new(&name);
            self.cursor = 0;
        }
    }

    /// `enter`: pick the highlighted item, or answer the argument being
    /// asked. `specs(name)`: the arguments of a command, (name, prompt) in
    /// the order they are asked.
    pub fn enter(&mut self, specs: &dyn Fn(&str) -> Vec<(String, String)>) -> Step {
        match self.step.clone() {
            Step::Open => {
                let Some(name) = self.ranked().get(self.cursor).map(|it| it.name.clone()) else {
                    return self.step.clone();
                };
                self.specs = specs(&name);
                self.answers.clear();
                self.asked_from = self.query.text.clone();
                self.query.clear();
                self.step = if self.specs.is_empty() {
                    Step::Run {
                        name,
                        args: BTreeMap::new(),
                    }
                } else {
                    Step::Args { name, asked: 0 }
                };
            }
            Step::Args { name, asked } => {
                let arg = self.specs[asked].0.clone();
                self.answers.insert(arg, self.query.text.clone());
                self.query.clear();
                self.step = if asked + 1 < self.specs.len() {
                    Step::Args {
                        name,
                        asked: asked + 1,
                    }
                } else {
                    Step::Run {
                        name,
                        args: std::mem::take(&mut self.answers),
                    }
                };
            }
            Step::Run { .. } | Step::Closed => {}
        }
        self.step.clone()
    }

    /// `esc`: closed, whatever the step.
    pub fn close(&mut self) {
        self.step = Step::Closed;
    }

    pub fn step(&self) -> &Step {
        &self.step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, title: &str, group: u8) -> Item {
        Item {
            name: name.into(),
            title: title.into(),
            keys: vec![],
            group,
        }
    }

    fn items() -> Vec<Item> {
        vec![
            item("world.room_next", "próxima sala", 0),
            item("world.room_prev", "sala anterior", 0),
            item("world.room_1", "sala 1", 0),
            item("world.room_new", "sala nova", 0),
            item("world.walk", "ANDAR (e volta)", 0),
            item("session.stop", "parar a sessão", 1),
            item("session.new", "nova sessão", 1),
            item("app.foot", "foot", 2),
            item("goto.acme", "ir para acme", 3),
        ]
    }

    fn names(p: &Palette) -> Vec<&str> {
        p.ranked().iter().map(|i| i.name.as_str()).collect()
    }

    fn no_args(_: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    #[test]
    fn empty_query_lists_everything_in_order_up_to_12() {
        let p = Palette::new(items(), "");
        assert_eq!(names(&p)[0], "world.room_next");
        assert_eq!(names(&p).len(), 9);
        let many: Vec<Item> = (0..30).map(|i| item(&format!("x.{i}"), "", 0)).collect();
        assert_eq!(Palette::new(many, "").ranked().len(), SHOWN);
    }

    #[test]
    fn fuzzy_ranking_is_stable_for_ties() {
        let p = Palette::new(items(), "world.room");
        let n = names(&p);
        assert_eq!(n[0], "world.room_next");
        assert!(n.contains(&"world.room_prev"));
        assert!(!n.contains(&"session.stop"));
        // same input, same order
        assert_eq!(n, names(&Palette::new(items(), "world.room")));
        // titles match too (pt-BR)
        assert_eq!(names(&Palette::new(items(), "parar"))[0], "session.stop");
        // ties by group: identical names in two groups
        let tie = vec![item("b.x", "", 3), item("a.x", "", 1), item("c.x", "", 1)];
        let p = Palette::new(tie, "x");
        assert_eq!(names(&p), ["a.x", "c.x", "b.x"]);
        assert!(names(&Palette::new(items(), "zzzq")).is_empty());
    }

    #[test]
    fn edit_moves_the_query_and_resets_the_highlight() {
        let mut p = Palette::new(items(), "");
        p.down();
        p.down();
        assert_eq!(p.cursor(), 2);
        p.edit(|l| l.insert("sess"));
        assert_eq!(p.cursor(), 0);
        assert_eq!(p.line().text, "sess");
        p.down();
        p.down();
        p.down();
        assert_eq!(p.cursor(), 1, "clamped to the two sessions");
        p.up();
        p.up();
        assert_eq!(p.cursor(), 0);
        // an edit that changes nothing keeps the highlight
        p.down();
        p.edit(|l| l.end());
        assert_eq!(p.cursor(), 1);
    }

    #[test]
    fn complete_takes_the_highlighted_name() {
        let mut p = Palette::new(items(), "sala nov");
        p.complete();
        assert_eq!(p.line().text, "world.room_new");
        assert_eq!(p.line().cursor, "world.room_new".len());
    }

    #[test]
    fn enter_without_args_runs() {
        let mut p = Palette::new(items(), "walk");
        let s = p.enter(&no_args);
        assert_eq!(
            s,
            Step::Run {
                name: "world.walk".into(),
                args: BTreeMap::new()
            }
        );
        assert_eq!(p.step(), &s);
        // nothing matches: stays open
        let mut q = Palette::new(items(), "zzzq");
        assert_eq!(q.enter(&no_args), Step::Open);
    }

    #[test]
    fn args_are_asked_in_sequence_in_the_same_line() {
        let specs = |name: &str| -> Vec<(String, String)> {
            if name == "world.room_new" {
                vec![
                    ("name".into(), "nome da sala".into()),
                    ("dir".into(), "pasta".into()),
                ]
            } else {
                vec![]
            }
        };
        let mut p = Palette::new(items(), "room_new");
        assert_eq!(
            p.enter(&specs),
            Step::Args {
                name: "world.room_new".into(),
                asked: 0
            }
        );
        assert_eq!(p.prompt(), Some("nome da sala"));
        assert_eq!(p.line().text, "", "the line is the answer now");
        assert_eq!(names(&p)[0], "world.room_new", "the list stays");
        p.edit(|l| l.insert("bravo"));
        assert_eq!(
            p.enter(&specs),
            Step::Args {
                name: "world.room_new".into(),
                asked: 1
            }
        );
        assert_eq!(p.prompt(), Some("pasta"));
        p.edit(|l| l.insert("~/src/bravo"));
        let s = p.enter(&specs);
        let Step::Run { name, args } = s else {
            panic!("{s:?}")
        };
        assert_eq!(name, "world.room_new");
        assert_eq!(args["name"], "bravo");
        assert_eq!(args["dir"], "~/src/bravo");
        assert_eq!(p.prompt(), None);
        p.close();
        assert_eq!(p.step(), &Step::Closed);
        p.complete();
        assert_eq!(p.enter(&specs), Step::Closed);
    }

    #[test]
    fn ask_starts_on_the_first_argument_and_carries_presets() {
        let specs = vec![("name".to_string(), "nome › ".to_string())];
        let preset = BTreeMap::from([("cat".to_string(), "7".to_string())]);
        let mut p = Palette::ask(items(), "session.rename", specs, preset.clone());
        assert_eq!(
            p.step(),
            &Step::Args {
                name: "session.rename".into(),
                asked: 0
            }
        );
        assert_eq!(p.prompt(), Some("nome › "));
        assert_eq!(
            names(&p),
            ["session.rename"],
            "the list shows the one command"
        );
        p.edit(|l| l.insert("novo"));
        let Step::Run { name, args } = p.enter(&no_args) else {
            panic!()
        };
        assert_eq!(name, "session.rename");
        assert_eq!(args["name"], "novo");
        assert_eq!(args["cat"], "7", "the preset rides along");
        // no argument to ask: it runs at once
        let q = Palette::ask(items(), "session.stop", vec![], preset);
        assert!(matches!(q.step(), Step::Run { args, .. } if args["cat"] == "7"));
    }

    #[test]
    fn fuzzy_rank_orders_indices_best_first() {
        let texts = ["parar a sessão", "nova sessão", "ir para acme", "foot"];
        assert_eq!(fuzzy_rank("", &texts), [0, 1, 2, 3], "empty: all, in order");
        let r = fuzzy_rank("sess", &texts);
        assert_eq!(r.len(), 2);
        assert!(r.contains(&0) && r.contains(&1));
        assert_eq!(fuzzy_rank("foot", &texts), [3]);
        assert!(fuzzy_rank("zzzq", &texts).is_empty());
        assert_eq!(
            fuzzy_rank("sess", &texts),
            fuzzy_rank("sess", &texts),
            "stable"
        );
    }
}
