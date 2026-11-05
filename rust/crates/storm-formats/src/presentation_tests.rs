use crate::{
    animation::{Animation, Clip},
    geometry::Geometry,
    waypoints::DeckGraph,
};
fn animation_bytes(parents: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [3u32, parents.len() as u32, 15f32.to_bits()] {
        bytes.extend(word.to_le_bytes());
    }
    for p in parents {
        bytes.extend(p.to_le_bytes());
    }
    for _ in 0..parents.len() + 3 {
        for f in [0.0f32, 1.0, 0.0] {
            bytes.extend(f.to_le_bytes());
        }
    }
    for _ in 0..parents.len() * 3 {
        for f in [0.0f32, 0.0, 0.0, 1.0] {
            bytes.extend(f.to_le_bytes());
        }
    }
    bytes
}
#[test]
fn original_an_layout_rejects_truncation_cycles_bad_parents_and_nonfinite_values() {
    let bytes = animation_bytes(&[-1, 0, 1]);
    let a = Animation::read(&bytes).unwrap();
    assert_eq!(a.frames, 3);
    assert_eq!(a.fps, 15.0);
    assert_eq!(a.order, vec![0, 1, 2]);
    assert_eq!(a.rotations.len(), 9);
    for length in 0..bytes.len() {
        assert!(Animation::read(&bytes[..length]).is_err());
    }
    for parents in [&[-1, 2, 1][..], &[-1, 1], &[-1, 99], &[0, 0]] {
        assert!(Animation::read(&animation_bytes(parents)).is_err());
    }
    let mut bad = bytes.clone();
    bad[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(Animation::read(&bad).is_err());
    let mut bad = bytes;
    let n = bad.len();
    bad[n - 16..].fill(0);
    assert!(Animation::read(&bad).is_err());
    assert_eq!(
        Animation::read(&animation_bytes(&[-1, 2, 0]))
            .unwrap()
            .order,
        vec![0, 2, 1]
    );
}
#[test]
fn animation_clips_validate_the_requested_range() {
    let ini = "animation=external.an\n[stand]\nstart_time=0\nend_time=0\n[walk]\nstart_time=1\nend_time=2\n";
    let clip = Clip::from_ini(ini, "walk", 3).unwrap();
    assert_eq!((clip.first, clip.last), (1, 2));
    assert!(Clip::from_ini(ini, "walk", 2).is_err());
    assert!(Clip::from_ini(ini, "missing", 3).is_err());
}
#[test]
fn waypoint_graph_preserves_types_and_links_and_rejects_invalid_input() {
    let ini = "[SIZE]\npoints=3\nlinks=2\n[POINT_DATA]\npoint 0=0,1,0,0\npoint 1=2,1,0,10\npoint 2=2,8,0,5\n[LINK_DATA]\nlink 0=0,1\nlink 1=1,2\n";
    let g = DeckGraph::read(ini).unwrap();
    assert!(DeckGraph::read(&ini.replace("link 1=1,2", "link 1=1,1")).is_ok());
    assert_eq!(g.points[2].kind, 5);
    assert_eq!(g.links, vec![[0, 1], [1, 2]]);
    let short = DeckGraph::read(&ini.replace("links=2", "links=4")).unwrap();
    assert_eq!(short.missing_links, 2);
    assert_eq!(short.links, g.links);
    assert!(DeckGraph::read(&ini.replace("link 0=0,1\n", "")).is_err());
    for bad in [
        ini.replace("1,2", "1,3"),
        ini.replace("2,8,0,5", "2,nan,0,5"),
        ini.replace("points=3", "points=101"),
        ini.replace("2,8,0,5", "2,8,0,11"),
        format!("{ini}link 1=0,2\n"),
    ] {
        assert!(DeckGraph::read(&bad).is_err(), "{bad}");
    }
}
#[test]
fn animated_gm_decodes_two_bone_indices_weight_normals_and_uvs() {
    let mut words = vec![0u32; 18];
    words[0] = u32::from_le_bytes(*b"50.1");
    words[5] = 1;
    words[8] = 1;
    words[9] = 1;
    words[10] = 1;
    words[17] = 1f32.to_bits();
    let mut bytes = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut material = [0u32; 14];
    material[0] = u32::MAX;
    material[1] = u32::MAX;
    material[2] = 1f32.to_bits();
    bytes.extend(material.into_iter().flat_map(u32::to_le_bytes));
    let mut object = [0u32; 26];
    object[0] = u32::MAX;
    object[1] = u32::MAX;
    object[2] = 1;
    object[8] = 1;
    object[10] = 3;
    bytes.extend(object.into_iter().flat_map(u32::to_le_bytes));
    bytes.extend([0u16, 1, 2].into_iter().flat_map(u16::to_le_bytes));
    bytes.extend(4u32.to_le_bytes());
    bytes.extend(132u32.to_le_bytes());
    let start = bytes.len();
    for i in 0..3 {
        for value in [i as f32, 0.0, 0.0, 0.25] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(0x0100u32.to_le_bytes());
        for value in [0.0f32, 1.0, 0.0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(u32::MAX.to_le_bytes());
        for value in [0.2f32, 0.8] {
            bytes.extend(value.to_le_bytes());
        }
    }
    let g = Geometry::read(&bytes).unwrap();
    let v = &g.buffers[0][0];
    let skin = v.skin.unwrap();
    assert_eq!(skin.bones, [0, 1]);
    assert_eq!(skin.weight, 0.25);
    assert_eq!(v.normal, [0.0, 1.0, 0.0]);
    assert_eq!(v.uv, [0.2, 0.8]);
    bytes[start + 12..start + 16].copy_from_slice(&1.5f32.to_le_bytes());
    assert!(Geometry::read(&bytes).is_err());
}
