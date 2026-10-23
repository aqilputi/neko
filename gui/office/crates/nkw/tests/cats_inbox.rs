//! The comfortable inbox (headless, `--demo`): at acme's first desk the
//! busy fixture's three walkers end where the station frames them whole —
//! the `shot-cats-inbox` flow, its dump projected through the camera.

mod common;

use common::run;
use glam::Vec3;
use nkw::cats::inbox::{HEAD_Y_MAX, HEAD_Y_MIN, MIN_DIST_M, framing};
use nkw_core::Pose;

fn pose(v: &serde_json::Value) -> Pose {
    let p = &v["pos"];
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    Pose::new(
        Vec3::new(f(&p[0]), f(&p[1]), f(&p[2])),
        f(&v["yaw"]),
        f(&v["pitch"]),
    )
}

#[test]
fn the_walkers_sit_where_the_station_frames_them() {
    for (w, h) in [(1280, 720), (1920, 1080)] {
        let size = format!("{w}x{h}");
        let r = run(
            "key J\n\
             wait 500\n\
             inject cats {\"fixture\":\"busy\"}\n\
             wait json /ext/cats/inbox/visible 3 4000\n\
             wait 20000\n\
             dump {dir}/d.json\n\
             quit\n",
            &["--size", &size],
            &[],
        );
        assert_eq!(r.code(), 0, "{}", r.stderr());
        let d = r.json("d.json");
        let cam = pose(&d["camera"]);
        let walkers = d["ext"]["cats"]["walkers"].as_array().unwrap();
        let out: Vec<_> = walkers.iter().filter(|x| x["home"] == false).collect();
        assert_eq!(out.len(), 3, "{size}: {walkers:?}");
        assert!(
            out.iter().all(|x| x["moving"] == false),
            "{size}: arrived: {out:?}"
        );
        // the desk tops that frame them (two here); the rest on the floor
        let perched: Vec<_> = out.into_iter().filter(|x| x["perch"] == true).collect();
        assert_eq!(perched.len(), 2, "{size}");
        for x in perched {
            let p = &x["pos"];
            let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
            let base = Vec3::new(f(&p[0]), f(&p[1]), f(&p[2]));
            let fr = framing(&cam, w as f32 / h as f32, base, None);
            assert!(
                (HEAD_Y_MIN..=HEAD_Y_MAX).contains(&fr.head_y),
                "{size}: {} head at {} of the frame",
                x["key"],
                fr.head_y
            );
            assert!(fr.ears_inside, "{size}: {x}");
            assert!(
                fr.dist >= MIN_DIST_M,
                "{size}: {} at {} m",
                x["key"],
                fr.dist
            );
        }
    }
}
