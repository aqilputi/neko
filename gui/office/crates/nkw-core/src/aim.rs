//! What is in the aim (§3.5): the generic "nearest candidate inside the
//! cone" over spheres, pure. The World and the plugins build the candidates.

use glam::Vec3;

/// Something aimable: `what`, a sphere at `centre` of `radius` metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate<T> {
    pub what: T,
    pub centre: Vec3,
    pub radius: f32,
}

/// The nearest candidate within `reach` metres whose sphere the cone of
/// half-angle `cone` (radians) around `ray` (origin, unit direction) touches.
pub fn pick<T: Copy>(
    ray: (Vec3, Vec3),
    cands: &[Candidate<T>],
    reach: f32,
    cone: f32,
) -> Option<T> {
    let (o, dir) = ray;
    let mut best: Option<(f32, T)> = None;
    for c in cands {
        let v = c.centre - o;
        let dist = v.length();
        let near = if dist <= c.radius {
            0.0
        } else {
            let along = v.dot(dir);
            if along <= 0.0 || dist - c.radius > reach {
                continue;
            }
            // the angle to its centre less the angle its radius covers
            let off = (along / dist).clamp(-1.0, 1.0).acos();
            let half = (c.radius / dist).clamp(0.0, 1.0).asin();
            if off - half > cone {
                continue;
            }
            dist - c.radius
        };
        if best.is_none_or(|(b, _)| near < b) {
            best = Some((near, c.what));
        }
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONE: f32 = 8.0 * std::f32::consts::PI / 180.0;

    fn c(what: u32, x: f32, z: f32, r: f32) -> Candidate<u32> {
        Candidate {
            what,
            centre: Vec3::new(x, 0.0, z),
            radius: r,
        }
    }

    #[test]
    fn the_cone_and_reach_table() {
        let ray = (Vec3::ZERO, Vec3::NEG_Z);
        let tan = |deg: f32| deg.to_radians().tan();
        // (candidates, want)
        let rows: Vec<(Vec<Candidate<u32>>, Option<u32>)> = vec![
            (vec![c(1, 0.0, -2.0, 0.1)], Some(1)),             // dead ahead
            (vec![c(1, 0.0, 2.0, 0.1)], None),                 // behind
            (vec![c(1, 2.0 * tan(7.0), -2.0, 0.0)], Some(1)),  // inside the cone
            (vec![c(1, 2.0 * tan(9.0), -2.0, 0.0)], None),     // outside it
            (vec![c(1, 2.0 * tan(12.0), -2.0, 0.2)], Some(1)), // its radius reaches in
            (vec![c(1, 0.0, -4.5, 0.4)], None),                // beyond the reach
            (vec![c(1, 0.0, -4.3, 0.4)], Some(1)),             // its near side within it
            (vec![c(1, 0.0, -3.0, 0.1), c(2, 0.0, -1.0, 0.1)], Some(2)), // the nearer wins
            (vec![c(1, 0.0, -1.0, 0.1), c(2, 0.0, -3.0, 0.1)], Some(1)), // whatever the order
            (vec![c(1, 0.05, 0.0, 0.3)], Some(1)),             // standing inside it
            (vec![], None),
        ];
        for (i, (cands, want)) in rows.into_iter().enumerate() {
            assert_eq!(pick(ray, &cands, 4.0, CONE), want, "row {i}");
        }
    }
}
