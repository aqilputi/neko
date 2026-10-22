//! Pure per-frame packing: DrawList → GPU-layout instance data, the
//! preset-64 target size, the MSAA sample count, the screen order.

use glam::Vec3;

use crate::RenderOpts;
use crate::draw::{Blob, Instance, ScreenQuad};

/// One kit instance as the shader reads it (80 bytes, attributes 3..8).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct InstanceData {
    pub model: [f32; 16],
    pub tint: [u8; 4],
    pub emissive: f32,
    pub fade: f32,
    pub _pad: f32,
}

/// One blob (20 bytes: centre + radius, alpha).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct BlobData {
    pub center_radius: [f32; 4],
    pub alpha: f32,
}

/// A contiguous range of `InstanceData` drawn with one call.
pub(crate) type Run = (u32, u32);

/// Packs `instances` grouped by mesh (counting sort, stable inside a mesh)
/// into `out`; `runs[mesh]` = (first, count). Reuses `out`'s allocation.
pub(crate) fn pack_instances(
    instances: &[Instance],
    out: &mut Vec<InstanceData>,
    runs: &mut [Run; 15],
) {
    let mut counts = [0u32; 15];
    for i in instances {
        counts[i.mesh.index()] += 1;
    }
    let mut first = 0;
    for (run, &n) in runs.iter_mut().zip(&counts) {
        *run = (first, n);
        first += n;
    }
    let mut next = runs.map(|r| r.0);
    out.clear();
    out.resize(instances.len(), InstanceData::default());
    for i in instances {
        let slot = &mut next[i.mesh.index()];
        out[*slot as usize] = InstanceData {
            model: i.model.to_cols_array(),
            tint: i.tint,
            emissive: i.emissive,
            fade: i.fade,
            _pad: 0.0,
        };
        *slot += 1;
    }
}

pub(crate) fn pack_blobs(blobs: &[Blob], out: &mut Vec<BlobData>) {
    out.clear();
    out.extend(blobs.iter().map(|b| BlobData {
        center_radius: b.center.extend(b.radius).to_array(),
        alpha: b.alpha,
    }));
}

/// The preset-64 internal size for a `dst` target and the integer factor
/// it is upscaled by: ~240..270 px high, never less than 2× down.
pub(crate) fn low_res(dst: [u32; 2]) -> ([u32; 2], u32) {
    let div = dst[1].div_ceil(270).max(2);
    (dst.map(|v| v.div_ceil(div).max(1)), div)
}

/// MSAA samples for the scene target: 4 when asked and supported, else 0;
/// preset 64 never multisamples (the jaggies are the look).
pub(crate) fn samples(opts: &RenderOpts, max_samples: i32) -> i32 {
    if opts.preset64 || opts.msaa == 0 || max_samples < 4 {
        0
    } else {
        4
    }
}

/// Screens that need blending (translucent or outlined): indices into
/// `screens`, farthest first.
pub(crate) fn blended_back_to_front(screens: &[ScreenQuad], eye: Vec3, out: &mut Vec<u32>) {
    out.clear();
    out.extend((0..screens.len() as u32).filter(|&i| !is_opaque(&screens[i as usize])));
    let dist = |i: &u32| {
        screens[*i as usize]
            .model
            .w_axis
            .truncate()
            .distance_squared(eye)
    };
    out.sort_unstable_by(|a, b| dist(b).total_cmp(&dist(a)));
}

/// Whether a screen goes in the opaque pass.
pub(crate) fn is_opaque(s: &ScreenQuad) -> bool {
    s.tex.opaque && !s.selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{ClientTex, MeshId};
    use glam::Mat4;

    #[test]
    fn instance_layout_matches_the_shader() {
        assert_eq!(std::mem::size_of::<InstanceData>(), 80);
        assert_eq!(std::mem::align_of::<InstanceData>(), 4);
        assert_eq!(std::mem::offset_of!(InstanceData, tint), 64);
        assert_eq!(std::mem::offset_of!(InstanceData, emissive), 68);
        assert_eq!(std::mem::offset_of!(InstanceData, fade), 72);
        assert_eq!(std::mem::size_of::<BlobData>(), 20);
        assert_eq!(std::mem::offset_of!(BlobData, alpha), 16);
    }

    fn inst(mesh: MeshId, x: f32) -> Instance {
        Instance {
            mesh,
            model: Mat4::from_translation(Vec3::X * x),
            tint: [1, 2, 3, 4],
            emissive: 0.5,
            fade: 0.25,
        }
    }

    #[test]
    fn instances_group_by_mesh_in_kit_order() {
        let list = [
            inst(MeshId::Chair, 1.0),
            inst(MeshId::Box, 2.0),
            inst(MeshId::Chair, 3.0),
            inst(MeshId::CatTail, 4.0),
        ];
        let mut out = vec![InstanceData::default(); 9];
        let mut runs = [(7, 7); 15];
        pack_instances(&list, &mut out, &mut runs);
        assert_eq!(out.len(), 4);
        let xs: Vec<f32> = out.iter().map(|d| d.model[12]).collect();
        assert_eq!(
            xs,
            [2.0, 1.0, 3.0, 4.0],
            "Box first, the chairs in input order, the tail last"
        );
        assert_eq!(runs[MeshId::Box.index()], (0, 1));
        assert_eq!(runs[MeshId::Chair.index()], (1, 2));
        assert_eq!(runs[MeshId::CatTail.index()], (3, 1));
        assert_eq!(runs[MeshId::Desk.index()].1, 0);
        let d = out[0];
        assert_eq!((d.tint, d.emissive, d.fade), ([1, 2, 3, 4], 0.5, 0.25));
        assert_eq!(
            d.model,
            Mat4::from_translation(Vec3::X * 2.0).to_cols_array()
        );
    }

    #[test]
    fn blobs_pack_centre_radius_alpha() {
        let mut out = Vec::new();
        pack_blobs(
            &[Blob {
                center: Vec3::new(1.0, 2.0, 3.0),
                radius: 0.5,
                alpha: 0.25,
            }],
            &mut out,
        );
        assert_eq!(
            out,
            [BlobData {
                center_radius: [1.0, 2.0, 3.0, 0.5],
                alpha: 0.25
            }]
        );
    }

    #[test]
    fn preset64_targets_about_270_lines_with_integer_blocks() {
        for (dst, low, div) in [
            ([320, 180], [160, 90], 2),
            ([1280, 720], [427, 240], 3),
            ([1920, 1080], [480, 270], 4),
            ([2560, 1440], [427, 240], 6),
            ([3840, 2160], [480, 270], 8),
            ([1, 1], [1, 1], 2),
        ] {
            assert_eq!(low_res(dst), (low, div), "{dst:?}");
        }
    }

    #[test]
    fn samples_follow_the_options_and_the_hardware() {
        let o = RenderOpts::default();
        assert_eq!(samples(&o, 8), 4);
        assert_eq!(samples(&o, 2), 0, "falls back when GL_MAX_SAMPLES < 4");
        assert_eq!(samples(&RenderOpts { msaa: 0, ..o }, 8), 0);
        assert_eq!(
            samples(
                &RenderOpts {
                    preset64: true,
                    ..o
                },
                8
            ),
            0
        );
    }

    #[test]
    fn blended_screens_sort_far_to_near() {
        let tex = |opaque| ClientTex {
            id: 1,
            external: false,
            size: [1, 1],
            y_flip: false,
            opaque,
        };
        let s = |z: f32, opaque, selected| ScreenQuad {
            model: Mat4::from_translation(Vec3::Z * z),
            tex: tex(opaque),
            dim: 0.0,
            selected,
        };
        let screens = [
            s(-1.0, false, false),
            s(-5.0, true, false),
            s(-3.0, true, true),
            s(-9.0, false, false),
        ];
        let mut out = vec![42];
        blended_back_to_front(&screens, Vec3::ZERO, &mut out);
        assert_eq!(out, [3, 2, 0]);
        assert!(is_opaque(&screens[1]) && !is_opaque(&screens[2]) && !is_opaque(&screens[0]));
    }
}
