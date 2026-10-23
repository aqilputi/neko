//! The S9 table as data: the world's commands, their modes, default keys
//! and pt-BR titles. The router falls back to it when the core's keymap
//! (`/v1/keymap`) is not there; the palette lists the world's commands from
//! it. Copied verbatim from the wave plan (P7 serves the same rows from Go,
//! P2's `Cmd::from_name` runs the ones the World executes).

/// (mode, key, command): every default binding.
pub const FALLBACK: &[(&str, &str, &str)] = &[
    ("normal.world", "h", "world.left"),
    ("normal.world", "left", "world.left"),
    ("normal.world", "l", "world.right"),
    ("normal.world", "right", "world.right"),
    ("normal.world", "j", "world.down"),
    ("normal.world", "down", "world.down"),
    ("normal.world", "k", "world.up"),
    ("normal.world", "up", "world.up"),
    ("normal.world", "J", "world.room_next"),
    ("normal.world", "world+shift+j", "world.room_next"),
    ("normal.world", "K", "world.room_prev"),
    ("normal.world", "world+shift+k", "world.room_prev"),
    ("normal.world", "1", "world.room_1"),
    ("normal.world", "2", "world.room_2"),
    ("normal.world", "3", "world.room_3"),
    ("normal.world", "4", "world.room_4"),
    ("normal.world", "5", "world.room_5"),
    ("normal.world", "6", "world.room_6"),
    ("normal.world", "7", "world.room_7"),
    ("normal.world", "8", "world.room_8"),
    ("normal.world", "9", "world.room_9"),
    ("normal.world", "gg", "world.first"),
    ("normal.world", "G", "world.last"),
    ("normal.world", "enter", "world.focus"),
    ("normal.world", "i", "world.focus"),
    ("normal.world", "t", "world.talk"),
    ("normal.world", "w", "world.walk"),
    ("normal.world", "world+w", "world.walk"),
    ("normal.world", "f", "world.hint"),
    ("normal.world", "world+f", "world.hint"),
    ("normal.world", "F", "world.hint_focus"),
    ("normal.world", "o", "world.overview"),
    ("normal.world", "world+o", "world.overview"),
    ("normal.world", "b", "world.library"),
    ("normal.world", "H", "world.move_left"),
    ("normal.world", "L", "world.move_right"),
    ("normal.world", ",", "world.consume"),
    ("normal.world", ".", "world.expel"),
    ("normal.world", "-", "world.narrower"),
    ("normal.world", "=", "world.wider"),
    ("normal.world", "p", "world.pin"),
    ("normal.world", "M", "world.send_room"),
    ("normal.world", "T", "world.terminal"),
    ("normal.world", "world+enter", "world.terminal"),
    ("normal.world", "Q", "world.close"),
    ("normal.world", "world+q", "world.close"),
    ("normal.world", "shift+space", "world.summon"),
    ("normal.world", "world+space", "world.summon"),
    ("normal.world", ":", "world.palette"),
    ("normal.world", "ctrl+p", "world.palette"),
    ("normal.world", "world+p", "world.palette"),
    ("world", "world+h", "world.focus_left"),
    ("world", "world+l", "world.focus_right"),
    ("world", "world+k", "world.focus_up"),
    ("world", "world+j", "world.focus_down"),
    ("world", "world+d", "world.launcher"),
    ("world", "world+shift+e", "world.quit"),
    ("world", "XF86AudioPlay", "media.play_pause"),
    ("world", "XF86AudioNext", "media.next"),
    ("world", "XF86AudioPrev", "media.prev"),
    ("world", "XF86AudioRaiseVolume", "volume.up"),
    ("world", "XF86AudioLowerVolume", "volume.down"),
    ("world", "XF86AudioMute", "volume.mute"),
    ("world.walk", "w", "world.forward"),
    ("world.walk", "s", "world.back"),
    ("world.walk", "a", "world.strafe_left"),
    ("world.walk", "d", "world.strafe_right"),
    ("world.walk", "q", "world.turn_left"),
    ("world.walk", "left", "world.turn_left"),
    ("world.walk", "e", "world.turn_right"),
    ("world.walk", "right", "world.turn_right"),
    ("world.walk", "up", "world.look_up"),
    ("world.walk", "down", "world.look_down"),
    ("world.walk", "c", "world.recenter"),
    ("world.walk", "enter", "world.interact"),
    ("world.walk", "space", "world.interact"),
    ("world.walk", "g", "world.grab"),
    ("world.walk", "[", "world.nearer"),
    ("world.walk", "]", "world.farther"),
    ("world.walk", "j", "world.adjust_down"),
    ("world.walk", "k", "world.adjust_up"),
    ("world.walk", "h", "world.prev"),
    ("world.walk", "l", "world.next"),
    ("world.walk", "f", "world.hint"),
    // a screen in hand: its width (R7's second-mode rows, the "na mão" hint)
    ("world.walk", "-", "world.narrower"),
    ("world.walk", "=", "world.wider"),
    ("world.walk", "esc", "world.walk_exit"),
    ("world.focus", "ctrl+]", "world.back"),
    ("world.talk", "space", "world.talk_next"),
    ("world.talk", "enter", "world.talk_next"),
    ("world.talk", "i", "world.talk_reply"),
    ("world.talk", "r", "world.talk_reply"),
    ("world.talk", "f", "world.talk_sit"),
    ("world.talk", "esc", "world.talk_end"),
    ("world.talk", "q", "world.talk_end"),
    ("world.talk", "j", "world.choice_down"),
    ("world.talk", "k", "world.choice_up"),
    ("world.talk", "1", "world.choice_1"),
    ("world.talk", "2", "world.choice_2"),
    ("world.talk", "3", "world.choice_3"),
    ("world.talk", "4", "world.choice_4"),
    ("world.talk", "5", "world.choice_5"),
    ("world.talk", "6", "world.choice_6"),
    ("world.talk", "7", "world.choice_7"),
    ("world.talk", "8", "world.choice_8"),
    ("world.talk", "9", "world.choice_9"),
    ("world.hint", "esc", "world.hint_cancel"),
    ("world.hint", "backspace", "world.hint_backspace"),
    ("world.palette", "esc", "world.palette_cancel"),
    ("world.palette", "enter", "world.palette_run"),
    ("world.palette", "tab", "world.palette_complete"),
    ("world.palette", "up", "world.palette_prev"),
    ("world.palette", "ctrl+p", "world.palette_prev"),
    ("world.palette", "down", "world.palette_next"),
    ("world.palette", "ctrl+n", "world.palette_next"),
    ("world.palette", "ctrl+a", "world.line_home"),
    ("world.palette", "ctrl+e", "world.line_end"),
    ("world.palette", "ctrl+w", "world.line_delete_word"),
    ("world.palette", "ctrl+u", "world.line_clear"),
    ("world.line", "ctrl+a", "world.line_home"),
    ("world.line", "ctrl+e", "world.line_end"),
    ("world.line", "ctrl+w", "world.line_delete_word"),
    ("world.line", "ctrl+u", "world.line_clear"),
    ("world.line", "esc", "world.line_cancel"),
    ("world.line", "enter", "world.line_send"),
    ("world.line", "shift+enter", "world.line_newline"),
    // S19: browsing a shelf (`Plugin::submode` = "library")
    ("world.library", "h", "world.book_prev"),
    ("world.library", "left", "world.book_prev"),
    ("world.library", "l", "world.book_next"),
    ("world.library", "right", "world.book_next"),
    ("world.library", "j", "world.shelf_down"),
    ("world.library", "down", "world.shelf_down"),
    ("world.library", "k", "world.shelf_up"),
    ("world.library", "up", "world.shelf_up"),
    ("world.library", "enter", "world.book_open"),
    ("world.library", "backspace", "world.shelf_back"),
    ("world.library", "/", "world.library_search"),
    ("world.library", "e", "world.book_edit"),
    ("world.library", "p", "world.book_preview"),
    ("world.library", "y", "world.book_copy_path"),
    ("world.library", "esc", "world.library_leave"),
];

/// (command, pt-BR title), in the table's order; also the palette-only
/// commands (no keys).
pub const TITLES: &[(&str, &str)] = &[
    ("world.left", "estação anterior"),
    ("world.right", "próxima estação"),
    ("world.down", "tela de baixo"),
    ("world.up", "tela de cima"),
    ("world.room_next", "próxima sala"),
    ("world.room_prev", "sala anterior"),
    ("world.room_1", "sala 1"),
    ("world.room_2", "sala 2"),
    ("world.room_3", "sala 3"),
    ("world.room_4", "sala 4"),
    ("world.room_5", "sala 5"),
    ("world.room_6", "sala 6"),
    ("world.room_7", "sala 7"),
    ("world.room_8", "sala 8"),
    ("world.room_9", "sala 9"),
    ("world.first", "primeira mesa"),
    ("world.last", "última mesa"),
    ("world.focus", "FOCO na tela (ou no terminal do gato)"),
    ("world.talk", "conversa com o gato"),
    ("world.walk", "ANDAR (e volta)"),
    ("world.hint", "SALTO"),
    ("world.hint_focus", "SALTO direto para o FOCO"),
    ("world.overview", "mapa"),
    ("world.library", "biblioteca"),
    ("world.move_left", "tela para a mesa à esquerda"),
    ("world.move_right", "tela para a mesa à direita"),
    ("world.consume", "junta na pilha à esquerda"),
    ("world.expel", "tira para uma mesa só dela"),
    ("world.narrower", "mesa mais estreita"),
    ("world.wider", "mesa mais larga"),
    ("world.pin", "prende a tela no ar"),
    ("world.send_room", "manda a tela para outra sala"),
    ("world.terminal", "terminal novo"),
    ("world.close", "fecha a janela"),
    ("world.summon", "chama o gato mais urgente"),
    ("world.palette", "COMANDOS"),
    ("world.fullscreen", "tela cheia (TV)"),
    ("world.maximize", "maximiza (largura 1)"),
    ("world.room_new", "sala nova"),
    ("world.goto_room", "ir para a sala"),
    ("world.goto_object", "ir até o objeto"),
    ("world.focus_left", "FOCO na vizinha"),
    ("world.focus_right", "FOCO na vizinha"),
    ("world.focus_up", "FOCO na vizinha"),
    ("world.focus_down", "FOCO na vizinha"),
    ("world.launcher", "apps"),
    ("world.quit", "sair do mundo (confirma)"),
    ("media.play_pause", "tocar/pausar"),
    ("media.next", "faixa"),
    ("media.prev", "faixa"),
    ("volume.up", "volume"),
    ("volume.down", "volume"),
    ("volume.mute", "volume"),
    ("brightness.up", "brilho"),
    ("brightness.down", "brilho"),
    ("world.forward", "andar"),
    ("world.back", "sai do FOCO"),
    ("world.strafe_left", "andar"),
    ("world.strafe_right", "andar"),
    ("world.turn_left", "girar"),
    ("world.turn_right", "girar"),
    ("world.look_up", "olhar"),
    ("world.look_down", "olhar"),
    ("world.recenter", "horizonte"),
    ("world.interact", "usar o que está na mira"),
    ("world.grab", "pegar/soltar a tela"),
    ("world.nearer", "tela na mão: mais perto"),
    ("world.farther", "tela na mão: mais longe"),
    ("world.adjust_down", "ajustar o objeto na mira"),
    ("world.adjust_up", "ajustar o objeto na mira"),
    ("world.prev", "anterior/próximo no objeto na mira"),
    ("world.next", "anterior/próximo no objeto na mira"),
    ("world.walk_exit", "volta ao NAV"),
    ("world.talk_next", "avança"),
    ("world.talk_reply", "responder (linha)"),
    ("world.talk_sit", "FOCO no terminal do gato"),
    ("world.talk_end", "encerra"),
    ("world.choice_down", "escolha"),
    ("world.choice_up", "escolha"),
    ("world.choice_1", "escolha 1"),
    ("world.choice_2", "escolha 2"),
    ("world.choice_3", "escolha 3"),
    ("world.choice_4", "escolha 4"),
    ("world.choice_5", "escolha 5"),
    ("world.choice_6", "escolha 6"),
    ("world.choice_7", "escolha 7"),
    ("world.choice_8", "escolha 8"),
    ("world.choice_9", "escolha 9"),
    ("world.hint_cancel", "cancela"),
    ("world.hint_backspace", "apaga"),
    ("world.palette_cancel", "paleta"),
    ("world.palette_run", "paleta"),
    ("world.palette_complete", "paleta"),
    ("world.palette_prev", "paleta"),
    ("world.palette_next", "paleta"),
    ("world.line_home", "edição"),
    ("world.line_end", "edição"),
    ("world.line_delete_word", "edição"),
    ("world.line_clear", "edição"),
    ("world.line_cancel", "linha"),
    ("world.line_send", "linha"),
    ("world.line_newline", "linha"),
    ("world.book_prev", "livro anterior"),
    ("world.book_next", "próximo livro"),
    ("world.shelf_down", "prateleira de baixo"),
    ("world.shelf_up", "prateleira de cima"),
    ("world.book_open", "abre o livro ou entra na pasta"),
    ("world.shelf_back", "sobe um nível"),
    ("world.library_search", "busca na estante"),
    ("world.book_edit", "abre no editor"),
    ("world.book_preview", "prévia no atril"),
    ("world.book_copy_path", "copia o caminho"),
    ("world.library_leave", "sai da estante"),
    ("world.open", "abre um arquivo no mundo"),
    ("world.vault_lock", "tranca o cofre"),
];

/// (command, arg, prompt): the arguments the palette asks, in order.
pub const ARGS: &[(&str, &str, &str)] = &[
    ("world.send_room", "room", "para qual sala?"),
    ("world.room_new", "name", "nome da sala"),
    ("world.goto_room", "room", "qual sala?"),
    ("world.goto_object", "name", "qual objeto?"),
    ("world.open", "path", "qual arquivo?"),
    ("world.library_search", "q", "buscar o quê?"),
];

/// Where a command is listed in COMANDOS: NAV's and the global ones and the
/// palette-only ones, not the keys of the other modes (walking, the
/// dialogue, the palette's own editing).
pub fn in_palette(cmd: &str) -> bool {
    let mode = FALLBACK.iter().find(|r| r.2 == cmd).map(|r| r.0);
    match mode {
        None => true, // palette-only
        Some(m) => {
            (m == "normal.world" || m == "world") && cmd != "world.palette" && cmd != "world.back"
        }
    }
}

/// The pt-BR title of a command of the table.
pub fn title(cmd: &str) -> Option<&'static str> {
    TITLES.iter().find(|t| t.0 == cmd).map(|t| t.1)
}

/// The default keys of a command, as the table spells them.
pub fn keys(cmd: &str) -> Vec<&'static str> {
    FALLBACK
        .iter()
        .filter(|r| r.2 == cmd)
        .map(|r| r.1)
        .collect()
}

/// The arguments of a command: (name, prompt).
pub fn args(cmd: &str) -> Vec<(&'static str, &'static str)> {
    ARGS.iter()
        .filter(|a| a.0 == cmd)
        .map(|a| (a.1, a.2))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_command_has_a_title_and_titles_are_unique() {
        for (_, _, cmd) in FALLBACK {
            assert!(title(cmd).is_some_and(|t| !t.is_empty()), "{cmd}");
        }
        let names: BTreeSet<&str> = TITLES.iter().map(|t| t.0).collect();
        assert_eq!(names.len(), TITLES.len(), "a command listed twice");
        for a in ARGS {
            assert!(title(a.0).is_some(), "{}", a.0);
        }
    }

    #[test]
    fn world_mode_keys_are_chords_or_media() {
        for (mode, key, cmd) in FALLBACK {
            if *mode == "world" {
                assert!(
                    key.contains("world+") || key.starts_with("XF86"),
                    "{cmd} {key}"
                );
            }
        }
    }

    #[test]
    fn no_key_is_bound_twice_in_a_mode() {
        let mut seen = BTreeSet::new();
        for (mode, key, cmd) in FALLBACK {
            assert!(seen.insert((*mode, *key)), "{mode} {key} ({cmd})");
        }
    }

    #[test]
    fn palette_lists_nav_global_and_palette_only_commands() {
        assert!(in_palette("world.room_next"));
        assert!(in_palette("world.fullscreen"));
        assert!(in_palette("media.play_pause"));
        assert!(in_palette("world.launcher"));
        assert!(!in_palette("world.forward"));
        assert!(!in_palette("world.palette_run"));
        assert!(!in_palette("world.talk_next"));
        assert!(!in_palette("world.palette"));
        assert_eq!(keys("world.walk"), ["w", "world+w"]);
        assert_eq!(args("world.room_new"), [("name", "nome da sala")]);
        assert!(args("world.left").is_empty());
    }
}
