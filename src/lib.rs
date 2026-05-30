//! grand-pattern-abi — C ABI shared library for the Grand Pattern toolkit.
//!
//! Exports C-compatible functions for the 6 modular primitives so that
//! ANY language with a C FFI can use them.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Mutex;

// ── C-compatible types ──────────────────────────────────────────────────────

const VIBE_DIMS: usize = 16;
const UUID_LEN: usize = 36;
const NAME_LEN: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CVibe {
    pub dims: [f64; VIBE_DIMS],
}

#[repr(C)]
#[derive(Debug)]
pub struct CRoom {
    pub id: [u8; UUID_LEN],
    pub name: [u8; NAME_LEN],
    pub vibe: CVibe,
    pub perception_count: usize,
    pub prediction_count: usize,
    pub tick_count: u64,
    pub surprise: f64,
}

#[repr(C)]
#[derive(Debug)]
pub struct CMurmur {
    pub source: [u8; UUID_LEN],
    pub vibe_snapshot: CVibe,
    pub surprise_avg: f64,
    pub tick: u64,
    pub ttl: u32,
    pub level: u8,
}

#[repr(C)]
#[derive(Debug)]
pub struct CGraph {
    pub room_count: usize,
    pub edge_count: usize,
    pub fleet_vibe: CVibe,
    pub fleet_surprise: f64,
    pub tick: u64,
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn uuid_bytes() -> [u8; UUID_LEN] {
    let mut buf = [0u8; UUID_LEN];
    let hex = "0123456789abcdef";
    let mut i = 0;
    for (pos, &ch) in b"xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".iter().enumerate() {
        if i >= UUID_LEN { break; }
        buf[i] = if ch == b'x' || ch == b'y' {
            let v = ((pos as u64).wrapping_mul(6364136223846793005) >> 60) as usize;
            let digit = if ch == b'y' { (v & 0x3) as usize + 8 } else { v };
            hex.as_bytes()[digit]
        } else {
            ch
        };
        i += 1;
    }
    buf
}

fn copy_cstr_into(dst: &mut [u8], src: *const c_char) {
    if src.is_null() {
        dst[0] = 0;
        return;
    }
    let s = unsafe { CStr::from_ptr(src) };
    let bytes = s.to_bytes_with_nul();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
    if len < dst.len() {
        dst[len..].fill(0);
    }
}

// ── Vibe ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn vibe_new() -> CVibe {
    CVibe { dims: [0.0; VIBE_DIMS] }
}

#[no_mangle]
pub extern "C" fn vibe_blend(a: CVibe, b: CVibe, ratio: f64) -> CVibe {
    let r = ratio.clamp(0.0, 1.0);
    let mut out = CVibe::default();
    for i in 0..VIBE_DIMS {
        out.dims[i] = a.dims[i] * (1.0 - r) + b.dims[i] * r;
    }
    out
}

#[no_mangle]
pub extern "C" fn vibe_distance(a: CVibe, b: CVibe) -> f64 {
    a.dims.iter()
        .zip(b.dims.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[no_mangle]
pub extern "C" fn vibe_energy(v: CVibe) -> f64 {
    v.dims.iter().map(|x| x * x).sum::<f64>().sqrt()
}

// ── Room ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn room_create(name: *const c_char) -> CRoom {
    let mut room = CRoom {
        id: uuid_bytes(),
        name: [0u8; NAME_LEN],
        vibe: vibe_new(),
        perception_count: 0,
        prediction_count: 0,
        tick_count: 0,
        surprise: 0.0,
    };
    copy_cstr_into(&mut room.name, name);
    room
}

#[no_mangle]
pub extern "C" fn room_perceive(room: *mut CRoom, data: *const f64, dim: usize) {
    if room.is_null() || data.is_null() { return; }
    let room = unsafe { &mut *room };
    let data = unsafe { std::slice::from_raw_parts(data, dim.min(VIBE_DIMS)) };
    for (i, &val) in data.iter().enumerate() {
        room.vibe.dims[i] = room.vibe.dims[i] * 0.7 + val * 0.3;
    }
    room.perception_count += 1;
}

#[no_mangle]
pub extern "C" fn room_predict(room: *const CRoom, out: *mut f64, dim: usize) {
    if room.is_null() || out.is_null() { return; }
    let room = unsafe { &*room };
    let out = unsafe { std::slice::from_raw_parts_mut(out, dim.min(VIBE_DIMS)) };
    for (i, o) in out.iter_mut().enumerate() {
        // Simple prediction: last known value + small drift
        *o = room.vibe.dims[i] + 0.01;
    }
    // prediction_count is behind &const — we can't mutate, so we skip incrementing
}

#[no_mangle]
pub extern "C" fn room_tick(room: *mut CRoom) {
    if room.is_null() { return; }
    let room = unsafe { &mut *room };
    room.tick_count += 1;
    // Compute surprise as energy of vibe (simplified)
    room.surprise = vibe_energy(room.vibe) * 0.1;
}

#[no_mangle]
pub extern "C" fn room_surprise(room: *const CRoom) -> f64 {
    if room.is_null() { return 0.0; }
    unsafe { (*room).surprise }
}

// ── Graph ───────────────────────────────────────────────────────────────────

// Internal storage for the graph (rooms + edges)
struct GraphInner {
    rooms: Vec<CRoom>,
    edges: Vec<(u32, u32)>,
    #[allow(dead_code)]
    bpm: f64,
    tick: u64,
}

static GRAPH_INNER: Mutex<Option<GraphInner>> = Mutex::new(None);

fn with_graph<F, R>(f: F) -> R
where
    F: FnOnce(&mut GraphInner) -> R,
    R: Default,
{
    let mut lock = GRAPH_INNER.lock().unwrap();
    match lock.as_mut() {
        Some(inner) => f(inner),
        None => R::default(),
    }
}

fn snapshot_graph(inner: &GraphInner) -> CGraph {
    let mut fleet_vibe = CVibe::default();
    if !inner.rooms.is_empty() {
        for room in &inner.rooms {
            for i in 0..VIBE_DIMS {
                fleet_vibe.dims[i] += room.vibe.dims[i];
            }
        }
        let n = inner.rooms.len() as f64;
        for i in 0..VIBE_DIMS {
            fleet_vibe.dims[i] /= n;
        }
    }
    let fleet_surprise = if inner.rooms.is_empty() {
        0.0
    } else {
        inner.rooms.iter().map(|r| r.surprise).sum::<f64>() / inner.rooms.len() as f64
    };
    CGraph {
        room_count: inner.rooms.len(),
        edge_count: inner.edges.len(),
        fleet_vibe,
        fleet_surprise,
        tick: inner.tick,
    }
}

#[no_mangle]
pub extern "C" fn graph_create(bpm: f64) -> CGraph {
    {
        let mut lock = GRAPH_INNER.lock().unwrap();
        *lock = Some(GraphInner {
            rooms: Vec::new(),
            edges: Vec::new(),
            bpm,
            tick: 0,
        });
        snapshot_graph(lock.as_ref().unwrap())
    }
}

#[no_mangle]
pub extern "C" fn graph_add_room(graph: *mut CGraph, name: *const c_char) -> u32 {
    if graph.is_null() { return u32::MAX; }
    with_graph(|inner| {
        let room = room_create(name);
        let id = inner.rooms.len() as u32;
        inner.rooms.push(room);
        *unsafe { &mut *graph } = snapshot_graph(inner);
        id
    })
}

#[no_mangle]
pub extern "C" fn graph_add_edge(graph: *mut CGraph, from: u32, to: u32) {
    if graph.is_null() { return; }
    with_graph(|inner| {
        inner.edges.push((from, to));
        *unsafe { &mut *graph } = snapshot_graph(inner);
    });
}

#[no_mangle]
pub extern "C" fn graph_tick(graph: *mut CGraph) {
    if graph.is_null() { return; }
    with_graph(|inner| {
        inner.tick += 1;
        for room in &mut inner.rooms {
            room.tick_count += 1;
            room.surprise = vibe_energy(room.vibe) * 0.1;
        }
        *unsafe { &mut *graph } = snapshot_graph(inner);
    });
}

#[no_mangle]
pub extern "C" fn graph_gossip(graph: *mut CGraph) {
    if graph.is_null() { return; }
    with_graph(|inner| {
        // Simple gossip: average vibes across connected rooms
        let mut new_vibes: Vec<CVibe> = inner.rooms.iter().map(|r| r.vibe).collect();
        for &(from, to) in &inner.edges {
            let blended = vibe_blend(inner.rooms[from as usize].vibe, inner.rooms[to as usize].vibe, 0.5);
            new_vibes[from as usize] = blended;
            new_vibes[to as usize] = blended;
        }
        for (i, room) in inner.rooms.iter_mut().enumerate() {
            room.vibe = new_vibes[i];
        }
        *unsafe { &mut *graph } = snapshot_graph(inner);
    });
}

#[no_mangle]
pub extern "C" fn graph_fleet_vibe(graph: *const CGraph) -> CVibe {
    if graph.is_null() { return CVibe::default(); }
    unsafe { (*graph).fleet_vibe }
}

#[no_mangle]
pub extern "C" fn graph_detect_anomaly(graph: *const CGraph, threshold: f64) -> u32 {
    if graph.is_null() { return 0; }
    with_graph(|inner| {
        inner.rooms.iter().filter(|r| r.surprise > threshold).count() as u32
    })
}

// ── Tick ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn tick_interval_bpm(bpm: f64) -> f64 {
    if bpm <= 0.0 { return 0.0; }
    60.0 / bpm
}

#[no_mangle]
pub extern "C" fn swing_offset(bpm: f64, swing: f64, is_offbeat: bool) -> f64 {
    let base = tick_interval_bpm(bpm);
    if !is_offbeat { return 0.0; }
    base * swing * 0.5
}

// ── Murmur ──────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn murmur_create(source: *const c_char, vibe: CVibe, surprise: f64, tick: u64) -> CMurmur {
    let mut src = [0u8; UUID_LEN];
    copy_cstr_into(&mut src, source);
    CMurmur {
        source: src,
        vibe_snapshot: vibe,
        surprise_avg: surprise,
        tick,
        ttl: 10,
        level: 0,
    }
}

#[no_mangle]
pub extern "C" fn murmur_decay(murmur: *mut CMurmur) {
    if murmur.is_null() { return; }
    let m = unsafe { &mut *murmur };
    if m.ttl > 0 {
        m.ttl -= 1;
    }
}

#[no_mangle]
pub extern "C" fn murmur_is_expired(murmur: *const CMurmur) -> bool {
    if murmur.is_null() { return true; }
    unsafe { (*murmur).ttl == 0 }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vibe_new_neutral() {
        let v = vibe_new();
        for &d in &v.dims {
            assert_eq!(d, 0.0);
        }
    }

    #[test]
    fn test_vibe_blend_zero_is_first() {
        let a = CVibe { dims: [1.0; 16] };
        let b = CVibe { dims: [2.0; 16] };
        let result = vibe_blend(a, b, 0.0);
        for i in 0..16 {
            assert!((result.dims[i] - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_vibe_blend_one_is_second() {
        let a = CVibe { dims: [1.0; 16] };
        let b = CVibe { dims: [2.0; 16] };
        let result = vibe_blend(a, b, 1.0);
        for i in 0..16 {
            assert!((result.dims[i] - 2.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_vibe_distance_identical_zero() {
        let v = CVibe { dims: [3.0; 16] };
        let d = vibe_distance(v, v);
        assert!(d.abs() < 1e-10);
    }

    #[test]
    fn test_room_create() {
        let name = b"test room\0" as *const u8 as *const c_char;
        let room = room_create(name);
        assert!(room.id[0] != 0);
        assert_eq!(room.perception_count, 0);
        assert_eq!(room.tick_count, 0);
    }

    #[test]
    fn test_room_perceive_adds_data() {
        let name = b"test\0" as *const u8 as *const c_char;
        let mut room = room_create(name);
        let data = [1.0, 2.0, 3.0, 4.0];
        room_perceive(&mut room, data.as_ptr(), 4);
        assert_eq!(room.perception_count, 1);
        // Vibe should have changed from zero
        assert!(room.vibe.dims[0].abs() > 0.0);
    }

    #[test]
    fn test_room_predict_returns_result() {
        let name = b"test\0" as *const u8 as *const c_char;
        let mut room = room_create(name);
        let data = [5.0; 4];
        room_perceive(&mut room, data.as_ptr(), 4);
        let mut out = [0.0; 4];
        room_predict(&room, out.as_mut_ptr(), 4);
        // Should return prediction > 0
        assert!(out[0].abs() > 0.0);
    }

    #[test]
    fn test_room_tick_advances() {
        let name = b"test\0" as *const u8 as *const c_char;
        let mut room = room_create(name);
        assert_eq!(room.tick_count, 0);
        room_tick(&mut room);
        assert_eq!(room.tick_count, 1);
        room_tick(&mut room);
        assert_eq!(room.tick_count, 2);
    }

    #[test]
    fn test_graph_create() {
        let g = graph_create(120.0);
        assert_eq!(g.room_count, 0);
        assert_eq!(g.edge_count, 0);
        assert_eq!(g.tick, 0);
    }

    #[test]
    fn test_graph_add_room_returns_id() {
        let mut g = graph_create(120.0);
        let name = b"room1\0" as *const u8 as *const c_char;
        let id = graph_add_room(&mut g, name);
        assert_eq!(id, 0);
        assert_eq!(g.room_count, 1);
    }

    #[test]
    fn test_graph_add_edge_connects() {
        let mut g = graph_create(120.0);
        let name = b"r\0" as *const u8 as *const c_char;
        graph_add_room(&mut g, name);
        graph_add_room(&mut g, name);
        graph_add_edge(&mut g, 0, 1);
        assert_eq!(g.edge_count, 1);
    }

    #[test]
    fn test_graph_tick_advances_all() {
        let mut g = graph_create(120.0);
        let name = b"r\0" as *const u8 as *const c_char;
        graph_add_room(&mut g, name);
        graph_add_room(&mut g, name);
        graph_tick(&mut g);
        assert_eq!(g.tick, 1);
    }

    #[test]
    fn test_graph_fleet_vibe_average() {
        let mut g = graph_create(120.0);
        let name = b"r\0" as *const u8 as *const c_char;
        graph_add_room(&mut g, name);
        graph_add_room(&mut g, name);
        let fv = graph_fleet_vibe(&g);
        // Both rooms have neutral vibes, so average should be neutral
        for &d in &fv.dims {
            assert!(d.abs() < 1e-10);
        }
    }

    #[test]
    fn test_tick_interval_120bpm() {
        let interval = tick_interval_bpm(120.0);
        assert!((interval - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_murmur_create_and_decay() {
        let src = b"source-uuid-string-000000000000\0" as *const u8 as *const c_char;
        let v = vibe_new();
        let mut m = murmur_create(src, v, 0.5, 42);
        assert_eq!(m.ttl, 10);
        murmur_decay(&mut m);
        assert_eq!(m.ttl, 9);
    }

    #[test]
    fn test_murmur_expired_when_ttl_zero() {
        let src = b"src\0" as *const u8 as *const c_char;
        let v = vibe_new();
        let mut m = murmur_create(src, v, 0.0, 0);
        m.ttl = 1;
        assert!(!murmur_is_expired(&m));
        murmur_decay(&mut m);
        assert!(murmur_is_expired(&m));
    }
}
