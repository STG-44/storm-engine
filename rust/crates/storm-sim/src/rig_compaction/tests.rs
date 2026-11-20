use super::*;
use serde_json::{Value as Json, json};
fn n(j: &Json) -> u32 {
    j.as_u64().unwrap().try_into().unwrap()
}
fn event(j: &Json) -> BufferEvent {
    match n(&j[0]) {
        0 => BufferEvent::ReleaseVertex(n(&j[1]) as i32),
        1 => BufferEvent::ReleaseIndex(n(&j[1]) as i32),
        2 => BufferEvent::CreateVertex {
            id: n(&j[1]) as i32,
            bytes: n(&j[2]),
            dynamic: n(&j[3]) != 0,
        },
        3 => BufferEvent::CreateIndex {
            id: n(&j[1]) as i32,
            bytes: n(&j[2]),
        },
        4 => BufferEvent::Lock(n(&j[1]) as i32),
        5 => BufferEvent::Unlock(n(&j[1]) as i32),
        6 => BufferEvent::FillSail {
            id: n(&j[1]),
            start: n(&j[2]),
        },
        7 => BufferEvent::TextureSail {
            id: n(&j[1]),
            start: n(&j[2]),
        },
        8 => BufferEvent::SetVertices,
        9 => BufferEvent::SetIndices,
        10 => BufferEvent::SetTriangles,
        11 => BufferEvent::SetTexture,
        _ => panic!("bad reference event"),
    }
}
fn decode(kind: Kind, j: &Json) -> Layout {
    Layout {
        kind,
        groups: j["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| Group {
                owner: g[0].as_u64().unwrap(),
                deleted: g[1].as_bool().unwrap(),
                members: g[2]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| n(i) as usize)
                    .collect(),
                span: (!g[3].is_null()).then(|| std::array::from_fn(|i| n(&g[3][i]))),
            })
            .collect(),
        items: j["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (!i.is_null()).then(|| Item {
                    id: n(&i[0]),
                    group: n(&i[1]) as usize,
                    deleted: i[2].as_bool().unwrap(),
                    vertices: n(&i[3]),
                    triangles: n(&i[4]),
                    start_vertex: n(&i[5]),
                    start_index: n(&i[6]),
                })
            })
            .collect(),
        vertices: n(&j["vertices"]),
        triangles: (!j["triangles"].is_null()).then(|| n(&j["triangles"])),
        active: j["active"].as_bool().unwrap(),
        pending: j["pending"].as_bool().unwrap(),
        last: n(&j["last"]),
        vertices_need_update: j["update"].as_bool().unwrap(),
        buffers: Buffers {
            vertex: j["vertex_buffer"].as_i64().unwrap() as i32,
            index: j["index_buffer"].as_i64().unwrap() as i32,
            next_id: j["next_buffer"].as_i64().unwrap() as i32,
            lock_succeeds: j["lock"].as_bool().unwrap(),
            events: j["events"].as_array().unwrap().iter().map(event).collect(),
        },
    }
}
#[test]
fn native_sail_rope_flag_vant_layouts_and_complete_buffer_intents_match_cpp() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/rig_compaction/cases.json")).unwrap();
    let kinds = [Kind::Sail, Kind::Rope, Kind::Flag, Kind::Vant];
    let mut snapshots = 0;
    for case in data["cases"].as_array().unwrap() {
        let kind = kinds[n(&case["kind"]) as usize];
        for frame in case["snapshots"].as_array().unwrap() {
            let mut actual = decode(kind, &frame["before"]);
            actual.compact().unwrap();
            assert_eq!(
                actual,
                decode(kind, &frame["after"]),
                "kind {:?}, profile {}",
                kind,
                case["profile"]
            );
            assert!(!actual.execute().unwrap());
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 768);
}
#[test]
fn checked_compaction_keeps_metadata_and_buffer_intents_on_bad_membership_or_byte_overflow() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/rig_compaction/cases.json")).unwrap();
    let case = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["kind"] == 1
                && !c["snapshots"][0]["before"]["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        })
        .unwrap();
    let mut bad = decode(Kind::Rope, &case["snapshots"][0]["before"]);
    bad.groups[0].members.push(usize::MAX);
    let before = bad.clone();
    assert!(bad.compact().is_err());
    assert_eq!(bad, before);
    let mut big = Layout {
        kind: Kind::Sail,
        groups: vec![Group {
            owner: 1,
            deleted: false,
            members: (0..100).collect(),
            span: None,
        }],
        items: (0..100)
            .map(|id| {
                Some(Item {
                    id,
                    group: 0,
                    deleted: false,
                    vertices: 1_000_000,
                    triangles: 0,
                    start_vertex: 0,
                    start_index: 0,
                })
            })
            .collect(),
        vertices: 0,
        triangles: None,
        active: true,
        pending: true,
        last: 0,
        vertices_need_update: false,
        buffers: Buffers {
            vertex: 4,
            index: 5,
            next_id: 100,
            lock_succeeds: true,
            events: Vec::new(),
        },
    };
    let before = big.clone();
    assert!(big.compact().is_err());
    assert_eq!(big, before);
    assert_eq!(json!(big.buffers.events.is_empty()), true);
}
