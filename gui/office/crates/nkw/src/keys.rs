//! Script keys → key events through the real keymap: a keysym is found on
//! a key and level of the active XKB keymap, the level's modifiers are
//! pressed around it, and characters the layout only reaches through a
//! dead key (`´` + `e` → `é`) are typed that way.

use std::collections::HashMap;

use smithay::input::keyboard::xkb;

use crate::script::{Chord, Mods};

/// One key event: an XKB keycode (evdev + 8) and whether it goes down.
pub type Ev = (u32, bool);

/// Where each keysym lives on the keymap (layout 0).
#[derive(Debug)]
pub struct KeyTable {
    /// keysym → (keycode, modifier mask of its level)
    by_sym: HashMap<u32, (u32, u32)>,
    shift: u32,
    level3: u32,
    /// keycodes of the modifier keys
    shift_l: u32,
    ctrl_l: u32,
    alt_l: u32,
    level3_key: Option<u32>,
}

/// Dead keys and the characters they compose with a base letter.
const DEAD: &[(u32, &str, &str)] = &[
    (0xfe51, "áéíóúýÁÉÍÓÚÝ", "aeiouyAEIOUY"), // dead_acute
    (0xfe50, "àèìòùÀÈÌÒÙ", "aeiouAEIOU"),     // dead_grave
    (0xfe52, "âêîôûÂÊÎÔÛ", "aeiouAEIOU"),     // dead_circumflex
    (0xfe53, "ãõñÃÕÑ", "aonAON"),             // dead_tilde
    (0xfe57, "äëïöüÿÄËÏÖÜ", "aeiouyAEIOU"),   // dead_diaeresis
    (0xfe5b, "çÇ", "cC"),                     // dead_cedilla
];

fn decompose(c: char) -> Option<(u32, char)> {
    DEAD.iter().find_map(|(dead, composed, base)| {
        let i = composed.chars().position(|x| x == c)?;
        Some((*dead, base.chars().nth(i)?))
    })
}

impl KeyTable {
    pub fn new(keymap: &xkb::Keymap) -> KeyTable {
        let bit = |name: &str| {
            let i = keymap.mod_get_index(name);
            if i == xkb::MOD_INVALID { 0 } else { 1u32 << i }
        };
        let shift = bit(xkb::MOD_NAME_SHIFT);
        let level3 = bit(xkb::MOD_NAME_ISO_LEVEL3_SHIFT);
        let usable = shift | level3;
        let mut by_sym: HashMap<u32, (u32, u32)> = HashMap::new();
        let (min, max) = (keymap.min_keycode().raw(), keymap.max_keycode().raw());
        for kc in min..=max {
            let key = xkb::Keycode::new(kc);
            for level in 0..keymap.num_levels_for_key(key, 0) {
                let mut masks = [0u32; 8];
                let n = keymap.key_get_mods_for_level(key, 0, level, &mut masks);
                // the simplest mask we can produce with shift/level3
                let Some(mask) = masks[..n]
                    .iter()
                    .copied()
                    .filter(|m| m & !usable == 0)
                    .min_by_key(|m| m.count_ones())
                else {
                    continue;
                };
                for sym in keymap.key_get_syms_by_level(key, 0, level) {
                    let e = by_sym.entry(sym.raw()).or_insert((kc, mask));
                    // prefer fewer modifiers, then the lower keycode
                    if (mask.count_ones(), kc) < (e.1.count_ones(), e.0) {
                        *e = (kc, mask);
                    }
                }
            }
        }
        let code = |sym: u32, fallback: u32| by_sym.get(&sym).map_or(fallback, |e| e.0);
        KeyTable {
            shift_l: code(0xffe1, 50),
            ctrl_l: code(0xffe3, 37),
            alt_l: code(0xffe9, 64),
            level3_key: by_sym.get(&0xfe03).map(|e| e.0),
            by_sym,
            shift,
            level3,
        }
    }

    /// From a keymap string (what the seat's keyboard hands out).
    pub fn from_string(s: &str) -> Option<KeyTable> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let km = xkb::Keymap::new_from_string(
            &ctx,
            s.to_string(),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        Some(KeyTable::new(&km))
    }

    fn level_keys(&self, mask: u32) -> Result<Vec<u32>, String> {
        let mut v = Vec::new();
        if mask & self.shift != 0 {
            v.push(self.shift_l);
        }
        if mask & self.level3 != 0 && self.level3 != 0 {
            v.push(self.level3_key.ok_or("no ISO_Level3_Shift key")?);
        }
        Ok(v)
    }

    fn sym_of(name: &str) -> Option<u32> {
        let s = xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS).raw();
        if s != 0 {
            return Some(s);
        }
        let mut cs = name.chars();
        match (cs.next(), cs.next()) {
            (Some(c), None) => Some(xkb::utf32_to_keysym(c as u32).raw()).filter(|s| *s != 0),
            _ => None,
        }
    }

    /// The key and the modifier keys of its level for a keysym.
    fn locate(&self, sym: u32) -> Option<(u32, Vec<u32>)> {
        let &(kc, mask) = self.by_sym.get(&sym)?;
        Some((kc, self.level_keys(mask).ok()?))
    }

    fn chord_mods(&self, m: &Mods) -> Vec<u32> {
        let mut v = Vec::new();
        if m.ctrl || m.world {
            v.push(self.ctrl_l);
        }
        if m.alt || m.world {
            v.push(self.alt_l);
        }
        if m.shift {
            v.push(self.shift_l);
        }
        v
    }

    /// The keys a chord holds, modifiers first.
    fn chord_keys(&self, c: &Chord) -> Result<Vec<u32>, String> {
        let sym = KeyTable::sym_of(&c.key).ok_or_else(|| format!("unknown key {:?}", c.key))?;
        let (kc, level) = self
            .locate(sym)
            .ok_or_else(|| format!("key {:?} is not on the keymap", c.key))?;
        let mut keys = self.chord_mods(&c.mods);
        for k in level {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys.push(kc);
        Ok(keys)
    }

    /// `key`: press everything, release in reverse.
    pub fn key(&self, c: &Chord) -> Result<Vec<Ev>, String> {
        let keys = self.chord_keys(c)?;
        let mut ev: Vec<Ev> = keys.iter().map(|&k| (k, true)).collect();
        ev.extend(keys.iter().rev().map(|&k| (k, false)));
        Ok(ev)
    }

    pub fn down(&self, c: &Chord) -> Result<Vec<Ev>, String> {
        Ok(self.chord_keys(c)?.into_iter().map(|k| (k, true)).collect())
    }

    pub fn up(&self, c: &Chord) -> Result<Vec<Ev>, String> {
        Ok(self
            .chord_keys(c)?
            .into_iter()
            .rev()
            .map(|k| (k, false))
            .collect())
    }

    /// `text`: each character as keys; through a dead key when the
    /// layout has no key for it.
    pub fn text(&self, s: &str) -> Result<Vec<Ev>, String> {
        let mut ev = Vec::new();
        for c in s.chars() {
            let tap = |ev: &mut Vec<Ev>, sym: u32| -> bool {
                let Some((kc, level)) = self.locate(sym) else {
                    return false;
                };
                ev.extend(level.iter().map(|&k| (k, true)));
                ev.extend([(kc, true), (kc, false)]);
                ev.extend(level.iter().rev().map(|&k| (k, false)));
                true
            };
            let sym = match c {
                '\n' => 0xff0d,
                '\t' => 0xff09,
                c => xkb::utf32_to_keysym(c as u32).raw(),
            };
            if tap(&mut ev, sym) {
                continue;
            }
            let ok = decompose(c).is_some_and(|(dead, base)| {
                let mut tmp = Vec::new();
                let ok =
                    tap(&mut tmp, dead) && tap(&mut tmp, xkb::utf32_to_keysym(base as u32).raw());
                if ok {
                    ev.extend(tmp);
                }
                ok
            });
            if !ok {
                return Err(format!("cannot type {c:?} with this keymap"));
            }
        }
        Ok(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::parse_chord;

    fn table(layout: &str, variant: &str) -> KeyTable {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let km = xkb::Keymap::new_from_names(
            &ctx,
            "",
            "pc105",
            layout,
            variant,
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("xkeyboard-config");
        KeyTable::new(&km)
    }

    // evdev + 8
    const SHIFT: u32 = 50;
    const CTRL: u32 = 37;
    const ALT: u32 = 64;
    const H: u32 = 43;
    const J: u32 = 44;
    const E: u32 = 26;
    const RETURN: u32 = 36;
    const BRACKET_R: u32 = 35;
    const APOS: u32 = 48;

    #[test]
    fn plain_and_shifted_keys() {
        let t = table("us", "");
        assert_eq!(
            t.key(&parse_chord("h").unwrap()).unwrap(),
            [(H, true), (H, false)]
        );
        assert_eq!(
            t.key(&parse_chord("J").unwrap()).unwrap(),
            [(SHIFT, true), (J, true), (J, false), (SHIFT, false)]
        );
        assert_eq!(
            t.key(&parse_chord("Return").unwrap()).unwrap(),
            [(RETURN, true), (RETURN, false)]
        );
    }

    #[test]
    fn chords_hold_their_modifiers() {
        let t = table("us", "");
        assert_eq!(
            t.key(&parse_chord("ctrl+bracketright").unwrap()).unwrap(),
            [
                (CTRL, true),
                (BRACKET_R, true),
                (BRACKET_R, false),
                (CTRL, false)
            ]
        );
        assert_eq!(
            t.key(&parse_chord("world+h").unwrap()).unwrap(),
            [
                (CTRL, true),
                (ALT, true),
                (H, true),
                (H, false),
                (ALT, false),
                (CTRL, false)
            ]
        );
        assert_eq!(
            t.down(&parse_chord("shift+h").unwrap()).unwrap(),
            [(SHIFT, true), (H, true)]
        );
        assert_eq!(
            t.up(&parse_chord("shift+h").unwrap()).unwrap(),
            [(H, false), (SHIFT, false)]
        );
        assert!(t.key(&parse_chord("nosuchkey").unwrap()).is_err());
    }

    #[test]
    fn text_with_dead_keys() {
        // us(intl): ' is dead_acute, so é is ´ then e
        let t = table("us", "intl");
        let ev = t.text("é").unwrap();
        let downs: Vec<u32> = ev.iter().filter(|e| e.1).map(|e| e.0).collect();
        assert!(
            downs == [APOS, E] || downs.contains(&E),
            "{ev:?}" // either the dead key or a direct level-3 key
        );
        // plain us has no way to type é
        assert!(table("us", "").text("é").is_err());
        assert_eq!(table("us", "").text("a\n").unwrap().len(), 4);
    }

    #[test]
    fn dead_key_decomposition() {
        assert_eq!(decompose('é'), Some((0xfe51, 'e')));
        assert_eq!(decompose('Ã'), Some((0xfe53, 'A')));
        assert_eq!(decompose('ç'), Some((0xfe5b, 'c')));
        assert_eq!(decompose('x'), None);
    }
}
