use std::collections::{BTreeMap, BTreeSet};

use aether_world::{Entity, EntityId, Place, Value, World};

use crate::law::Law;
use crate::terrain;

pub const DT: f64 = 0.1;
pub const GRAVITY: f64 = 9.8;
const CELL: f64 = 16.0;
const MARGIN: f64 = 1.0;
const MAX_EXTENT: f64 = 512.0;
const MAX_SPEED: f64 = 200.0;
const REST: f64 = 0.05;

#[derive(Clone, Copy)]
struct Body {
    id: EntityId,
    dim: i64,
    p: [f64; 3],
    v: [f64; 3],
    r: f64,
    inv_mass: f64,
    bounce: f64,
    friction: f64,
}

struct Solid {
    dim: i64,
    kind: Shape,
    c: [f64; 3],
    h: [f64; 3],
    cos: f64,
    sin: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Box,
    Sphere,
}

fn num(e: &Entity, key: &str) -> Option<f64> {
    match e.components.get(key) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    }
}

fn flag(e: &Entity, key: &str) -> Option<bool> {
    match e.components.get(key) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn text<'a>(e: &'a Entity, key: &str) -> Option<&'a str> {
    match e.components.get(key) {
        Some(Value::Text(t)) => Some(t),
        _ => None,
    }
}

fn body(id: EntityId, e: &Entity) -> Option<Body> {
    if flag(e, "body") != Some(true) {
        return None;
    }
    let place = Place::of(e)?;
    let size = num(e, "size").unwrap_or(0.28);
    let mass = num(e, "mass").unwrap_or(1.0);
    Some(Body {
        id,
        dim: place.dim,
        p: [place.x, place.y, num(e, "pos.z").unwrap_or(0.0)],
        v: [
            num(e, "vel.x").unwrap_or(0.0),
            num(e, "vel.y").unwrap_or(0.0),
            num(e, "vel.z").unwrap_or(0.0),
        ],
        r: num(e, "radius").unwrap_or(size).clamp(0.01, 50.0),
        inv_mass: if mass > 0.0 { 1.0 / mass.min(1e9) } else { 0.0 },
        bounce: num(e, "bounce").unwrap_or(0.3).clamp(0.0, 1.0),
        friction: num(e, "friction").unwrap_or(0.5).clamp(0.0, 1.0),
    })
}

fn solid(e: &Entity) -> Option<Solid> {
    if flag(e, "body") == Some(true) || e.components.contains_key("avatar") {
        return None;
    }
    let shape = match text(e, "shape")? {
        "box" if flag(e, "solid") != Some(false) => Shape::Box,
        "sphere" if flag(e, "solid") == Some(true) => Shape::Sphere,
        _ => return None,
    };
    let size = num(e, "size").unwrap_or(0.28);
    let full = if shape == Shape::Sphere { 1.0 } else { 0.5 };
    let default = if shape == Shape::Sphere {
        size
    } else {
        size * 2.0
    };
    let half = |k: &str| num(e, k).unwrap_or(default).abs() * full;
    let yaw = num(e, "rot.z").unwrap_or(0.0);
    Some(Solid {
        dim: Place::of(e)?.dim,
        kind: shape,
        c: [
            num(e, "pos.x")?,
            num(e, "pos.y").unwrap_or(0.0),
            num(e, "pos.z").unwrap_or(0.0),
        ],
        h: [half("scale.x"), half("scale.y"), half("scale.z")],
        cos: libm::cos(yaw),
        sin: libm::sin(yaw),
    })
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn reflect(b: &mut Body, n: [f64; 3]) {
    let vn = dot(b.v, n);
    if vn < 0.0 {
        let bounce = if -vn <= 2.0 * GRAVITY * DT {
            0.0
        } else {
            b.bounce
        };
        for (v, n) in b.v.iter_mut().zip(n) {
            *v -= (1.0 + bounce) * vn * n;
        }
        let k = 1.0 - b.friction * 0.2;
        let vn = dot(b.v, n);
        for (v, n) in b.v.iter_mut().zip(n) {
            *v = vn * n + (*v - vn * n) * k;
        }
    }
}

fn against(b: &mut Body, s: &Solid) {
    let d = [b.p[0] - s.c[0], b.p[1] - s.c[1], b.p[2] - s.c[2]];
    match s.kind {
        Shape::Sphere => {
            let reach = s.h[0].max(s.h[1]).max(s.h[2]) + b.r;
            let dist = dot(d, d).sqrt();
            if dist >= reach || dist < 1e-9 {
                return;
            }
            let n = [d[0] / dist, d[1] / dist, d[2] / dist];
            for (p, n) in b.p.iter_mut().zip(n) {
                *p += (reach - dist) * n;
            }
            reflect(b, n);
        }
        Shape::Box => {
            let local = [
                d[0] * s.cos + d[1] * s.sin,
                -d[0] * s.sin + d[1] * s.cos,
                d[2],
            ];
            let closest = [
                local[0].clamp(-s.h[0], s.h[0]),
                local[1].clamp(-s.h[1], s.h[1]),
                local[2].clamp(-s.h[2], s.h[2]),
            ];
            let off = [
                local[0] - closest[0],
                local[1] - closest[1],
                local[2] - closest[2],
            ];
            let dist = dot(off, off).sqrt();
            let (n, depth) = if dist > 1e-9 {
                if dist >= b.r {
                    return;
                }
                ([off[0] / dist, off[1] / dist, off[2] / dist], b.r - dist)
            } else {
                let gaps = [
                    s.h[0] - local[0].abs(),
                    s.h[1] - local[1].abs(),
                    s.h[2] - local[2].abs(),
                ];
                let axis = if gaps[2] <= gaps[0] && gaps[2] <= gaps[1] {
                    2
                } else if gaps[0] <= gaps[1] {
                    0
                } else {
                    1
                };
                let mut n = [0.0; 3];
                n[axis] = if local[axis] < 0.0 { -1.0 } else { 1.0 };
                (n, gaps[axis] + b.r)
            };
            let world_n = [
                n[0] * s.cos - n[1] * s.sin,
                n[0] * s.sin + n[1] * s.cos,
                n[2],
            ];
            for (p, n) in b.p.iter_mut().zip(world_n) {
                *p += depth * n;
            }
            reflect(b, world_n);
        }
    }
}

fn pairs(bodies: &[Body]) -> Vec<(usize, usize)> {
    let reach = bodies.iter().map(|b| b.r).fold(0.0, f64::max) * 2.0;
    if reach <= 0.0 {
        return Vec::new();
    }
    let cell = |b: &Body| {
        (
            b.dim,
            (b.p[0] / reach).floor() as i64,
            (b.p[1] / reach).floor() as i64,
        )
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, b) in bodies.iter().enumerate() {
        grid.entry(cell(b)).or_default().push(i);
    }
    let mut out = BTreeSet::new();
    for (i, b) in bodies.iter().enumerate() {
        let (dim, cx, cy) = cell(b);
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                for &j in grid.get(&(dim, x, y)).into_iter().flatten() {
                    if j > i {
                        out.insert((i, j));
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

fn collide(a: &mut Body, b: &mut Body) {
    let d = [b.p[0] - a.p[0], b.p[1] - a.p[1], b.p[2] - a.p[2]];
    let dist = dot(d, d).sqrt();
    let reach = a.r + b.r;
    let total = a.inv_mass + b.inv_mass;
    if dist >= reach || dist < 1e-9 || total <= 0.0 {
        return;
    }
    let n = [d[0] / dist, d[1] / dist, d[2] / dist];
    let push = (reach - dist) / total;
    for k in 0..3 {
        a.p[k] -= n[k] * push * a.inv_mass;
        b.p[k] += n[k] * push * b.inv_mass;
    }
    let closing = dot([a.v[0] - b.v[0], a.v[1] - b.v[1], a.v[2] - b.v[2]], n);
    if closing > 0.0 {
        let impulse = (1.0 + a.bounce.min(b.bounce)) * closing / total;
        for k in 0..3 {
            a.v[k] -= impulse * a.inv_mass * n[k];
            b.v[k] += impulse * b.inv_mass * n[k];
        }
    }
}

type Cell = (i64, i64, i64);

fn cells(dim: i64, x: f64, y: f64, reach: f64) -> impl Iterator<Item = Cell> {
    let reach = reach.min(MAX_EXTENT);
    let lo = (
        ((x - reach) / CELL).floor() as i64,
        ((y - reach) / CELL).floor() as i64,
    );
    let hi = (
        ((x + reach) / CELL).floor() as i64,
        ((y + reach) / CELL).floor() as i64,
    );
    (lo.0..=hi.0).flat_map(move |cx| (lo.1..=hi.1).map(move |cy| (dim, cx, cy)))
}

fn statics(world: &World) -> (Vec<Solid>, BTreeMap<Cell, Vec<usize>>) {
    let solids: Vec<Solid> = world
        .having("shape")
        .filter_map(|id| solid(world.entity(id)?))
        .collect();
    let mut grid: BTreeMap<Cell, Vec<usize>> = BTreeMap::new();
    for (i, s) in solids.iter().enumerate() {
        let reach = dot(s.h, s.h).sqrt() + MARGIN;
        for cell in cells(s.dim, s.c[0], s.c[1], reach) {
            grid.entry(cell).or_default().push(i);
        }
    }
    (solids, grid)
}

pub fn step(world: &mut World, law: &Law, touched: &mut BTreeSet<EntityId>) {
    let ids: Vec<EntityId> = world.having("body").take(law.max_bodies as usize).collect();
    let mut bodies: Vec<Body> = ids
        .iter()
        .filter_map(|id| body(*id, world.entity(*id)?))
        .filter(|b| b.inv_mass > 0.0 && b.p.iter().chain(&b.v).all(|x| x.is_finite()))
        .collect();
    if bodies.is_empty() {
        return;
    }
    let before: Vec<([f64; 3], [f64; 3])> = bodies.iter().map(|b| (b.p, b.v)).collect();
    let (solids, grid) = statics(world);
    let mut near = Vec::new();
    for b in &mut bodies {
        b.v[2] -= GRAVITY * DT;
        for v in &mut b.v {
            *v = v.clamp(-MAX_SPEED, MAX_SPEED);
        }
        for k in 0..3 {
            b.p[k] += b.v[k] * DT;
        }
        near.clear();
        for cell in cells(b.dim, b.p[0], b.p[1], b.r) {
            near.extend(grid.get(&cell).into_iter().flatten().copied());
        }
        near.sort_unstable();
        near.dedup();
        for &i in &near {
            against(b, &solids[i]);
        }
    }
    for (i, j) in pairs(&bodies) {
        let (lo, hi) = bodies.split_at_mut(j);
        collide(&mut lo[i], &mut hi[0]);
    }
    let mut txn = world.begin();
    for (b, (p0, v0)) in bodies.iter_mut().zip(before) {
        let ground = if b.dim == 0 {
            terrain::height(&law.terrain, b.p[0], b.p[1])
        } else {
            0.0
        };
        let floor = ground + b.r;
        if b.p[2] <= floor {
            b.p[2] = floor;
            reflect(b, [0.0, 0.0, 1.0]);
            if dot(b.v, b.v) < REST * REST {
                b.v = [0.0; 3];
            }
        }
        if b.p.iter().chain(&b.v).any(|x| !x.is_finite()) {
            continue;
        }
        let keys = [
            ("pos.x", p0[0], b.p[0]),
            ("pos.y", p0[1], b.p[1]),
            ("pos.z", p0[2], b.p[2]),
            ("vel.x", v0[0], b.v[0]),
            ("vel.y", v0[1], b.v[1]),
            ("vel.z", v0[2], b.v[2]),
        ];
        for (key, old, new) in keys {
            if old.to_bits() != new.to_bits() && txn.set(world, b.id, key, Value::Num(new)).is_ok()
            {
                touched.insert(b.id);
            }
        }
    }
    world
        .commit(txn)
        .expect("the physics transaction is built on the current world");
}
