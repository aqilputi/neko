//! `wp_single_pixel_buffer_manager_v1`: a 1×1 solid-colour buffer, stretched
//! by a viewport (players' black bars, backgrounds). Smithay's state is
//! enough: `on_commit_buffer_handler` sizes it, and the FOCO composition
//! (`WaylandSurfaceRenderElement`) draws it as a solid colour.
//!
//! Known gap: the world pass reads each window's *texture*, and Smithay never
//! imports a single-pixel buffer into one (`import_surface` skips it), so a
//! window made only of such a buffer shows as an empty screen in NAV/ANDAR
//! and correct in FOCO. Real apps put it under or beside a real buffer.
