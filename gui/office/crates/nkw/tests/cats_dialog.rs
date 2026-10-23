//! The dialogue regression (wave 2's `shot-cats-talk.png`: the box drew 18
//! of the question's 25 characters and no choices, because nothing composed
//! when the reveal ended). Headless, `--demo` fixture (`--core none`).

mod common;

use common::run;

/// The fixture's question ("Aplicar a migração agora?"), in characters; its
/// choices: the two options, "Outra resposta…" and "Depois".
const QUESTION_CHARS: u64 = 25;

fn talk_script(extra: &str) -> String {
    format!(
        "key J\n\
         wait 500\n\
         key l\n\
         wait 500\n\
         inject cats {{\"cmd\":\"world.talk\"}}\n\
         wait 100\n\
         expect mode CONVERSA\n\
         wait json /ext/cats/dialog/choices/0 \"sim, aplicar agora\" 3000\n\
         wait json /ext/overlay/dialog/choices 4 1000\n\
         expect json /ext/overlay/dialog/chars {QUESTION_CHARS}\n\
         {extra}\
         quit\n"
    )
}

#[test]
fn the_box_shows_the_whole_question_and_both_choices() {
    let r = run(
        &talk_script("shot {dir}/talk.png\ndump {dir}/d.json\n"),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    assert_eq!(
        d["ext"]["cats"]["dialog"]["shown"],
        "Aplicar a migração agora?"
    );
    assert_eq!(d["ext"]["overlay"]["dialog"]["chars"], QUESTION_CHARS);
    assert_eq!(d["ext"]["overlay"]["dialog"]["choices"], 4);
    assert_eq!(
        d["ext"]["cats"]["dialog"]["choices"][1],
        "não, revisar antes"
    );
    assert!(std::fs::metadata(r.path("talk.png")).unwrap().len() > 10_000);
}

#[test]
fn the_box_completes_with_nothing_else_moving() {
    // reduced motion: the reveal is instant and no Rate wish exists at all;
    // the box must still be whole on the first frames
    let r = run(&talk_script(""), &["--reduced-motion"], &[]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
}
