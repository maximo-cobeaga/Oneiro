//! Motion recording ("puppeteering"): drive a character with the keyboard, record the path,
//! and turn it into script lines (`Marta walks_to (1.20, 0.50)`).

use oneiro_core::math::*;

#[derive(Clone, Debug)]
pub struct Puppet {
    pub pos: V2,
    pub yaw: f64,
    pub odo: f64,
    pub speed: f64,
    pub running: bool,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Sample {
    pub t: f64,
    pub pos: V2,
    pub running: bool,
}

pub const WALK_SPEED: f64 = 1.25;
pub const RUN_SPEED: f64 = 3.0;

/// Advances the puppet by `dt` seconds. `input` is (right, forward) in -1..1; the directions are
/// relative to the camera so that "up" always means "away from the viewer".
pub fn step_puppet(
    p: &mut Puppet,
    input: V2,
    run: bool,
    cam_fwd: V2,
    cam_right: V2,
    dt: f64,
    bounds: (f64, f64),
) {
    let fwd = norm2(cam_fwd);
    let right = norm2(cam_right);
    let d = [right[0] * input[0] + fwd[0] * input[1], right[1] * input[0] + fwd[1] * input[1]];
    let mag = (d[0] * d[0] + d[1] * d[1]).sqrt();
    let target = if mag > 0.05 { if run { RUN_SPEED } else { WALK_SPEED } } else { 0.0 };
    let accel = 14.0 * dt;
    p.speed += (target - p.speed).clamp(-accel * 1.5, accel);
    p.running = run && target > 0.0;
    if mag > 0.05 {
        let dir = [d[0] / mag, d[1] / mag];
        let want = (dir[0]).atan2(dir[1]);
        let diff = angle_diff(want, p.yaw);
        p.yaw += diff.clamp(-9.0 * dt, 9.0 * dt);
        let step = p.speed * dt;
        let nx = (p.pos[0] + dir[0] * step).clamp(-bounds.0, bounds.0);
        let nz = (p.pos[1] + dir[1] * step).clamp(-bounds.1, bounds.1);
        p.odo += dist2(p.pos, [nx, nz]);
        p.pos = [nx, nz];
    } else {
        // Coasting to a stop keeps the legs moving a little.
        p.odo += p.speed * dt * 0.0;
    }
}

fn point_line_distance(p: V2, a: V2, b: V2) -> f64 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dz * dz;
    if len2 < 1e-12 {
        return dist2(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / len2).clamp(0.0, 1.0);
    dist2(p, [a[0] + dx * t, a[1] + dz * t])
}

/// Ramer-Douglas-Peucker: indices of the points to keep (always includes first and last).
pub fn simplify(points: &[V2], eps: f64) -> Vec<usize> {
    if points.len() < 3 {
        return (0..points.len()).collect();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut worst = 0.0;
        let mut idx = 0;
        for i in a + 1..b {
            let d = point_line_distance(points[i], points[a], points[b]);
            if d > worst {
                worst = d;
                idx = i;
            }
        }
        if worst > eps {
            keep[idx] = true;
            stack.push((a, idx));
            stack.push((idx, b));
        }
    }
    keep.iter().enumerate().filter(|(_, k)| **k).map(|(i, _)| i).collect()
}

fn fmt(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Turns recorded samples into `walks_to` / `runs_to` statements for `name`.
pub fn emit(name: &str, samples: &[Sample]) -> Vec<String> {
    if samples.len() < 2 {
        return vec![];
    }
    let pts: Vec<V2> = samples.iter().map(|s| s.pos).collect();
    let kept = simplify(&pts, 0.12);
    let mut waypoints: Vec<usize> = Vec::new();
    let mut last = 0usize;
    for (k, &i) in kept.iter().enumerate().skip(1) {
        let is_last = k == kept.len() - 1;
        if dist2(pts[last], pts[i]) >= 0.25 || (is_last && dist2(pts[0], pts[i]) >= 0.25) {
            if is_last && !waypoints.is_empty() && dist2(pts[last], pts[i]) < 0.25 {
                // Too close to the previous waypoint: move it instead of adding a stub.
                *waypoints.last_mut().unwrap() = i;
            } else {
                waypoints.push(i);
            }
            last = i;
        }
    }
    let mut out = Vec::new();
    let mut prev = 0usize;
    for &i in &waypoints {
        let running = samples[prev..=i].iter().filter(|s| s.running).count() * 2 > (i - prev + 1);
        let verb = if running { "runs_to" } else { "walks_to" };
        out.push(format!("{} {} ({}, {})", name, verb, fmt(pts[i][0]), fmt(pts[i][1])));
        prev = i;
    }
    out
}


/// A recording session in progress.
#[allow(dead_code)]
pub struct Recorder {
    pub actor: String,
    pub actor_idx: usize,
    pub scene_idx: usize,
    /// Scene-local time at which the recording starts (the end of the statement we insert after).
    pub t_start_local: f64,
    pub t_start_global: f64,
    pub start_pos: V2,
    pub puppet: Puppet,
    pub samples: Vec<Sample>,
    pub clock: f64,
    pub last_sample: f64,
    pub insert_after_line: u32,
    pub after_label: String,
}
#[cfg(test)]
mod tests {
    use super::*;

    fn walk(points: &[V2], running: bool) -> Vec<Sample> {
        points.iter().enumerate().map(|(i, p)| Sample { t: i as f64 / 30.0, pos: *p, running }).collect()
    }

    fn line(a: V2, b: V2, n: usize) -> Vec<V2> {
        (0..=n).map(|i| [a[0] + (b[0] - a[0]) * i as f64 / n as f64, a[1] + (b[1] - a[1]) * i as f64 / n as f64]).collect()
    }

    #[test]
    fn straight_line_is_one_waypoint() {
        let s = walk(&line([0.0, 0.0], [2.0, 1.0], 60), false);
        assert_eq!(emit("Marta", &s), vec!["Marta walks_to (2, 1)".to_string()]);
    }

    #[test]
    fn corner_keeps_the_corner() {
        let mut pts = line([0.0, 0.0], [2.0, 0.0], 40);
        pts.extend(line([2.0, 0.0], [2.0, 2.0], 40));
        let out = emit("Blob", &walk(&pts, false));
        assert_eq!(out, vec!["Blob walks_to (2, 0)".to_string(), "Blob walks_to (2, 2)".to_string()]);
    }

    #[test]
    fn running_is_detected_and_noise_is_ignored() {
        let pts = line([0.0, 0.0], [3.0, 0.0], 30);
        let jittery: Vec<V2> = pts.iter().enumerate().map(|(i, p)| [p[0], p[1] + if i % 2 == 0 { 0.02 } else { -0.02 }]).collect();
        let out = emit("Marta", &walk(&jittery, true));
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("runs_to"), "{:?}", out);
    }

    #[test]
    fn tiny_movements_emit_nothing() {
        assert!(emit("Marta", &walk(&line([0.0, 0.0], [0.1, 0.05], 10), false)).is_empty());
        assert!(emit("Marta", &[]).is_empty());
    }

    #[test]
    fn puppet_moves_relative_to_the_camera_and_respects_bounds() {
        let mut p = Puppet { pos: [0.0, 0.0], yaw: 0.0, odo: 0.0, speed: 0.0, running: false };
        // camera looks north (+z), right is east
        for _ in 0..120 {
            step_puppet(&mut p, [0.0, 1.0], false, [0.0, 1.0], [1.0, 0.0], 1.0 / 60.0, (3.0, 2.0));
        }
        assert!(p.pos[1] > 1.0 && p.pos[0].abs() < 1e-6, "{:?}", p);
        for _ in 0..600 {
            step_puppet(&mut p, [1.0, 0.0], true, [0.0, 1.0], [1.0, 0.0], 1.0 / 60.0, (3.0, 2.0));
        }
        assert!((p.pos[0] - 3.0).abs() < 1e-9, "{:?}", p);
        assert!(p.running && p.odo > 3.0);
    }
}
