// pathfinder/demo/common/src/camera.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Camera management code for the demo.

// TODO(#140, pcwalton): Move some of this out of the demo and into the library
// proper.

use crate::window::{OcularTransform, View};
use pathfinder_geometry::basic::point::{Point2DF32, Point2DI32, Point3DF32};
use pathfinder_geometry::basic::rect::RectF32;
use pathfinder_geometry::basic::transform2d::Transform2DF32;
use pathfinder_geometry::basic::transform3d::{Perspective, Transform3DF32};
use std::f32::consts::FRAC_PI_4;

const NEAR_CLIP_PLANE: f32 = 0.01;
const FAR_CLIP_PLANE: f32 = 10.0;

// Half of the eye separation distance.
const DEFAULT_EYE_OFFSET: f32 = 0.025;

pub enum Camera {
    TwoD(Transform2DF32),
    ThreeD {
        // The ocular transform used for rendering of the scene to the scene framebuffer. If we are
        // performing stereoscopic rendering, this is then reprojected according to the eye
        // transforms below.
        scene_transform: OcularTransform,
        // For each eye, the perspective from camera coordinates to display coordinates,
        // and the view transform from world coordinates to camera coordinates.
        eye_transforms: Vec<OcularTransform>,
        // The modelview transform from world coordinates to SVG coordinates
        modelview_transform: CameraTransform3D,
        // The camera's velocity (in world coordinates)
        velocity: Point3DF32,
    },
}

impl Camera {
    pub fn new(mode: Mode, view_box: RectF32, viewport_size: Point2DI32) -> Camera {
        if mode == Mode::TwoD {
            Camera::new_2d(view_box, viewport_size)
        } else {
            Camera::new_3d(mode, view_box, viewport_size)
        }
    }

    fn new_2d(view_box: RectF32, viewport_size: Point2DI32) -> Camera {
        let scale = i32::min(viewport_size.x(), viewport_size.y()) as f32
            * scale_factor_for_view_box(view_box);
        let origin = viewport_size.to_f32().scale(0.5) - view_box.size().scale(scale * 0.5);
        Camera::TwoD(Transform2DF32::from_scale(&Point2DF32::splat(scale)).post_translate(origin))
    }

    fn new_3d(mode: Mode, view_box: RectF32, viewport_size: Point2DI32) -> Camera {
        let viewport_count = mode.viewport_count();

        let fov_y = FRAC_PI_4;
        let aspect = viewport_size.x() as f32 / viewport_size.y() as f32;
        let projection =
            Transform3DF32::from_perspective(fov_y, aspect, NEAR_CLIP_PLANE, FAR_CLIP_PLANE);
        let perspective = Perspective::new(&projection, viewport_size);

        // Create a scene transform by moving the camera back from the center of the eyes so that
        // its field of view encompasses the field of view of both eyes.
        let z_offset = -DEFAULT_EYE_OFFSET * projection.c0.x();
        let scene_transform = OcularTransform {
            perspective,
            modelview_to_eye: Transform3DF32::from_translation(0.0, 0.0, z_offset),
        };

        // For now, initialize the eye transforms as copies of the scene transform.
        let eye_offset = DEFAULT_EYE_OFFSET;
        let eye_transforms = (0..viewport_count)
            .map(|viewport_index| {
                let this_eye_offset = if viewport_index == 0 {
                    eye_offset
                } else {
                    -eye_offset
                };
                OcularTransform {
                    perspective,
                    modelview_to_eye: Transform3DF32::from_translation(this_eye_offset, 0.0, 0.0),
                }
            })
            .collect();

        Camera::ThreeD {
            scene_transform,
            eye_transforms,
            modelview_transform: CameraTransform3D::new(view_box),
            velocity: Point3DF32::default(),
        }
    }

    pub fn is_3d(&self) -> bool {
        match *self {
            Camera::ThreeD { .. } => true,
            Camera::TwoD { .. } => false,
        }
    }

    pub fn mode(&self) -> Mode {
        match *self {
            Camera::ThreeD {
                ref eye_transforms, ..
            } if eye_transforms.len() >= 2 => Mode::VR,
            Camera::ThreeD { .. } => Mode::ThreeD,
            Camera::TwoD { .. } => Mode::TwoD,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraTransform3D {
    position: Point3DF32,
    pub yaw: f32,
    pub pitch: f32,
    scale: f32,
}

impl CameraTransform3D {
    fn new(view_box: RectF32) -> CameraTransform3D {
        let scale = scale_factor_for_view_box(view_box);
        CameraTransform3D {
            position: Point3DF32::new(
                0.5 * view_box.max_x(),
                -0.5 * view_box.max_y(),
                1.5 / scale,
                1.0,
            ),
            yaw: 0.0,
            pitch: 0.0,
            scale,
        }
    }

    pub fn offset(&mut self, vector: Point3DF32) -> bool {
        let update = !vector.is_zero();
        if update {
            let rotation = Transform3DF32::from_rotation(-self.yaw, -self.pitch, 0.0);
            self.position = self.position + rotation.transform_point(vector);
        }
        update
    }

    pub fn to_transform(&self) -> Transform3DF32 {
        let mut transform = Transform3DF32::from_rotation(self.yaw, self.pitch, 0.0);
        transform = transform.post_mul(&Transform3DF32::from_uniform_scale(2.0 * self.scale));
        transform = transform.post_mul(&Transform3DF32::from_translation(
            -self.position.x(),
            -self.position.y(),
            -self.position.z(),
        ));

        // Flip Y.
        transform = transform.post_mul(&Transform3DF32::from_scale(1.0, -1.0, 1.0));

        transform
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    TwoD = 0,
    ThreeD = 1,
    VR = 2,
}

impl Mode {
    pub fn viewport_count(self) -> usize {
        match self {
            Mode::TwoD | Mode::ThreeD => 1,
            Mode::VR => 2,
        }
    }

    pub fn view(self, viewport: u32) -> View {
        match self {
            Mode::TwoD | Mode::ThreeD => View::Mono,
            Mode::VR => View::Stereo(viewport),
        }
    }
}

pub fn scale_factor_for_view_box(view_box: RectF32) -> f32 {
    1.0 / f32::min(view_box.size().x(), view_box.size().y())
}