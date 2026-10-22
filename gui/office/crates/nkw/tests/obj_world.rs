//! P5 end to end on the headless backend (`--demo`: the fixture objects, no
//! D-Bus, no PipeWire; nothing reaches the user's players or volumes).

mod common;

use common::run;

const POWER_12: &str =
    r#"{"Power":{"present":true,"pct":12,"charging":false,"time_to_empty_s":1800}}"#;

#[test]
fn battery_at_12_percent_shows_the_badge() {
    let r = run(
        &format!(
            "wait 100
expect json /ext/objects/badges []
inject objects {POWER_12}
expect json /ext/objects/badges/0 {{\"BatteryLow\":12}}
expect json /ext/objects/sys/mpris \"off\"
quit
"
        ),
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn a_closed_door_stops_the_walker_and_interact_opens_it() {
    let r = run(
        "wait 100
key w
expect mode ANDAR
inject objects {\"Door\":{\"id\":0,\"toggle\":true}}
wait json /ext/objects/doors/0/passable false 1000
wait json /ext/objects/visible_rooms [0] 1000
expect json /ext/objects/visible_rooms [0]
down s
wait 1500
up s
wait 200
dump {dir}/closed.json
down e
wait 1640
up e
wait 300
inject objects {\"Command\":{\"name\":\"world.interact\"}}
expect json /ext/objects/visible_rooms/1 1
wait json /ext/objects/doors/0/passable true 1000
dump {dir}/open.json
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
    let d = r.json("closed.json");
    let z = d["camera"]["pos"][2].as_f64().unwrap();
    // the reception is north of the hall: its door is at z = −1.5
    assert!(
        z < -1.5 - 0.2,
        "the walker stopped short of the leaf: z = {z}"
    );
    assert!(z > -2.6, "it did walk back towards the door: z = {z}");
    let o = r.json("open.json");
    assert_eq!(o["ext"]["objects"]["doors"][0]["passable"], true);
}

#[test]
fn at_rest_with_the_turntable_playing_out_of_view_nothing_animates() {
    // (the cats animate on their own: this is the objects' share only)
    let r = run(
        "wait 1500
expect json /ext/objects/rate_wishes 0
expect json /ext/objects/objects/0/kind \"Turntable\"
expect json /ext/objects/objects/0/tier \"Far\"
wait 1000
expect json /ext/objects/rate_wishes 0
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}

#[test]
fn the_clock_changes_the_day_band() {
    let r = run(
        "wait 100
expect json /ext/objects/band 3
inject objects {\"Clock\":{\"unix_s\":0,\"hour\":21,\"minute\":0,\"band\":6}}
wait json /ext/objects/band 6 1000
quit
",
        &[],
        &[],
    );
    assert_eq!(r.code(), 0, "{}", r.stderr());
}
