//! Direction, distance and height from your pose to the gargoyle. Used raw, like TF2's own CTF flag
//! compass (CTFArrowPanel::GetAngleRotation), so the arrow stays snappy.

use crate::radar::rcon::Pose;

const HU_TO_METRES: f32 = 0.01905; // 1 Hammer unit = 0.75 inch

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// Signed horizontal angle from where you look to the gargoyle; 0 = ahead, positive = to your right.
    pub angle_deg: f32,
    pub dist_m: f32,
    /// Gargoyle height relative to your feet; positive = above you.
    pub height_m: f32,
}

/// Wraps an angle into (-180, 180].
fn wrap(deg: f32) -> f32 {
    let a = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if a == -180.0 {
        180.0
    } else {
        a
    }
}

pub fn reading(pose: &Pose, gargoyle: [f32; 3]) -> Reading {
    let (dx, dy) = (gargoyle[0] - pose.eye[0], gargoyle[1] - pose.eye[1]);
    // TF2 yaw grows counter-clockwise from +X, so a target clockwise of your view (to your right) has a
    // smaller bearing than your yaw
    let bearing = dy.atan2(dx).to_degrees();
    let height = gargoyle[2] - pose.feet[2];
    Reading {
        angle_deg: wrap(pose.yaw - bearing),
        dist_m: (dx * dx + dy * dy + height * height).sqrt() * HU_TO_METRES,
        height_m: height * HU_TO_METRES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(yaw: f32) -> Pose {
        Pose {
            eye: [0.0, 0.0, 64.0],
            pitch: 0.0,
            yaw,
            feet: [0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn directions_follow_tf2_yaw() {
        // facing +Y (yaw 90): +X is to the right, -X to the left, -Y behind
        assert!((reading(&pose(90.0), [100.0, 0.0, 0.0]).angle_deg - 90.0).abs() < 1e-3);
        assert!((reading(&pose(90.0), [-100.0, 0.0, 0.0]).angle_deg + 90.0).abs() < 1e-3);
        assert!((reading(&pose(90.0), [0.0, -100.0, 0.0]).angle_deg.abs() - 180.0).abs() < 1e-3);
        assert!(reading(&pose(90.0), [0.0, 100.0, 0.0]).angle_deg.abs() < 1e-3);
        // wrap-around: facing yaw -170, target at bearing 170 is 20 degrees to the right
        let r = reading(
            &pose(-170.0),
            [
                (170f32).to_radians().cos() * 100.0,
                (170f32).to_radians().sin() * 100.0,
                0.0,
            ],
        );
        assert!((r.angle_deg + 20.0).abs() < 1e-3 || (r.angle_deg - 20.0).abs() < 1e-3);
        assert!((r.angle_deg - (-170.0 - 170.0 + 360.0)).abs() < 1e-3);
    }

    #[test]
    fn distance_and_height_use_the_feet() {
        let r = reading(&pose(0.0), [300.0, 400.0, 105.0]);
        assert!(
            (r.dist_m - (300f32 * 300.0 + 400.0 * 400.0 + 105.0 * 105.0).sqrt() * HU_TO_METRES)
                .abs()
                < 1e-4
        );
        assert!((r.height_m - 105.0 * HU_TO_METRES).abs() < 1e-5);
    }
}
