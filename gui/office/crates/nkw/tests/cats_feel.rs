//! The cats react to you (FEEL §3.5 of the objects plan), headless with the
//! `--demo` fixture: a notice when you come within 2.5 m, the tail within
//! 1.3 m, nothing at rest.

mod common;

use common::run;

#[test]
fn close_to_a_cat_its_tail_sways() {
    let r = run(
        "key J\n\
         wait 500\n\
         inject cats {\"near\":{\"cat\":3,\"m\":1.2}}\n\
         wait json /ext/cats/feel/0/state \"tail\" 1000\n\
         expect json /ext/cats/feel/0/cat 3\n\
         dump {dir}/d.json\n\
         quit\n",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    assert!(
        d["wants"].to_string().contains("rate:15"),
        "the tail sways: {}",
        d["wants"]
    );
}

#[test]
fn walking_up_to_a_cat_it_notices_you() {
    // at the reception's desk the receptionist was there first (no jump);
    // step back out of reach, then walk up to her
    let r = run(
        "wait 300\n\
         expect json /ext/cats/feel/0/state \"work\"\n\
         key w\n\
         expect mode ANDAR\n\
         down s\n\
         wait 900\n\
         up s\n\
         wait 200\n\
         dump {dir}/far.json\n\
         down w\n\
         wait json /ext/cats/feel/0/state \"notice\" 1500\n\
         up w\n\
         dump {dir}/near.json\n\
         quit\n",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let far = r.json("far.json");
    assert!(
        far["ext"]["cats"]["feel"]
            .as_array()
            .is_some_and(|f| f.iter().all(|x| x["state"] == "work")),
        "{}",
        far["ext"]["cats"]["feel"]
    );
    let near = r.json("near.json");
    assert_eq!(near["ext"]["cats"]["feel"][0]["cat"], 1, "the receptionist");
}

#[test]
fn at_rest_by_the_cats_nothing_ticks() {
    // the reception, the receptionist within reach, for 3 s: the cats ask
    // for no rate (other plugins' ambient objects may: only ours counted)
    let r = run("wait 3000\ndump {dir}/d.json\nquit\n", &[], &[]);
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("d.json");
    let wants = d["wants"].to_string();
    assert!(
        !wants.contains("rate:15") && !wants.contains("rate:30"),
        "{wants}"
    );
    let feel = d["ext"]["cats"]["feel"].as_array().unwrap();
    assert!(feel.iter().all(|f| f["state"] == "work"), "{feel:?}");
}
