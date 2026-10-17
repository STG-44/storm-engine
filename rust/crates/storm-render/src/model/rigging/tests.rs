use super::*;
use crate::model::{SailOrigin, SailShape};
use storm_sim::SailTier;
use storm_sim::{FlagCell, FlagNation, FlagSelection, FlagTexture, ShipFlagRole, ShipFlagState};

#[test]
fn named_flags_preserve_special_groups_and_sample_inside_the_selected_cell() {
    let mut asset = crate::model::ModelAsset::rigging_demo().unwrap();
    asset
        .set_flags(FlagSelection::Ship(ShipFlagState {
            nation: FlagNation::France,
            role: ShipFlagRole::Merchant,
            ..Default::default()
        }))
        .unwrap();
    let mut count = 0;
    for mesh in &asset.meshes {
        if let Some(RigMesh::Flag(f)) = &mesh.rigging {
            let o = f.origin.as_ref().unwrap();
            let row = if o.special { 3 } else { 0 };
            assert_eq!(o.special, o.group.starts_with("sflag"));
            assert_eq!(f.texture(), FlagTexture::Cell(FlagCell { column: 2, row }));
            assert!(mesh.vertices.iter().all(|v| v.uv[0] > 0.5
                && v.uv[0] < 0.75
                && v.uv[1] > row as f32 / 8.0
                && v.uv[1] < (row + 1) as f32 / 8.0));
            count += 1;
        }
    }
    assert_eq!(count, 2);
}

#[test]
fn mixed_atlas_updates_reject_out_of_range_cells_atomically_and_hide_reversibly() {
    let patch = |columns, rows| {
        FlagPatch::new(
            Vec3::Y,
            Vec3::ZERO,
            Vec3::new(3.0, 1.0, 0.0),
            None,
            columns,
            rows,
        )
        .unwrap()
    };
    let mut meshes = [RigMesh::Flag(patch(4, 8)), RigMesh::Flag(patch(1, 1))];
    let cell = FlagTexture::Cell(FlagCell { column: 2, row: 7 });
    assert!(select_flags(meshes.iter_mut(), FlagSelection::Override(cell)).is_err());
    for m in &meshes {
        if let RigMesh::Flag(f) = m {
            assert_eq!(f.texture(), FlagTexture::Cell(FlagCell::default()));
        }
    }
    select_flags(
        meshes.iter_mut(),
        FlagSelection::Override(FlagTexture::Hidden),
    )
    .unwrap();
    for m in &meshes {
        if let RigMesh::Flag(f) = m {
            assert!(!f.is_visible());
        }
    }
    select_flags(meshes.iter_mut(), FlagSelection::Fallback).unwrap();
    for m in &meshes {
        if let RigMesh::Flag(f) = m {
            assert!(f.is_visible());
        }
    }
    assert!(patch(4, 8).set_texture_size(1, 1).is_err());
}

fn label(group: &str, name: &str, position: Vec3) -> Label {
    Label {
        group: group.into(),
        name: name.into(),
        matrix: Mat4::from_translation(position).to_cols_array(),
    }
}

#[test]
fn quad_flag_uses_original_f1_f4_attached_edge() {
    let p = [
        Vec3::Y * 2.0,
        Vec3::new(4.0, 2.0, 0.0),
        Vec3::new(4.0, 1.0, 0.0),
        Vec3::Y,
    ];
    let labels = std::array::from_fn::<_, 4, _>(|i| label("flag1", &format!("f{}", i + 1), p[i]));
    let mut collector = Collector::default();
    collector.add(&labels, Mat4::IDENTITY, "mast");
    let (_, flags, stats) = collector.build(&[], &RiggingConfig::default()).unwrap();
    assert_eq!(stats.flags, 1);
    let mesh = RigMesh::Flag(flags[0].clone());
    let vertices = mesh
        .vertices(SailWind::default(), 3.0, RigState::default())
        .unwrap();
    assert_eq!(vertices[0].position, p[0].to_array());
    assert_eq!(vertices[1].position, p[3].to_array());
    assert_eq!(mesh.indices().len(), mesh.index_count());
}

#[test]
fn ini_selects_original_textures_widths_and_atlas_without_executing_scripts() {
    let c=RiggingConfig::from_ini("[ROPES]\nTextureName = sail_rope.tga\nfWIDTH = 0.04\nfSTAY_WIDTH=0.12\n[FLAGS]\nTextureName=flags\\flagall_ship_b.tga\nTextureCountColumn=4\nTextureCountRow=8\n[VANTS]\nfRopeQuant=5\n").unwrap();
    assert_eq!(c.rope_texture, "sail_rope.tga");
    assert_eq!(c.flag_texture, "flags\\flagall_ship_b.tga");
    assert_eq!((c.flag_columns, c.flag_rows), (4, 8));
    assert!(RiggingConfig::from_ini("[ROPES]\nfWIDTH=nan").is_err());
    assert!(RiggingConfig::from_ini("[FLAGS]\nTextureCountRow=0").is_err());
}

#[test]
fn ropes_pair_across_nodes_and_follow_the_moving_sail_corner() {
    let points = [
        Vec3::new(-1.0, 3.0, 0.0),
        Vec3::new(1.0, 3.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-1.0, 0.0, 0.0),
    ];
    let mut sail = SailPatch::new(SailShape::Quad(points)).unwrap();
    sail.tier = SailTier::FullOnly;
    sail.origin = Some(SailOrigin {
        group: "sails1".into(),
        node: "rey_a2".into(),
        parent: "mast2".into(),
    });
    let mut collector = Collector::default();
    collector.add(
        &[label("sails1", "ropeb1", points[2])],
        Mat4::IDENTITY,
        "rey_a2",
    );
    let end = Vec3::new(4.0, 0.0, 0.0);
    collector.add(
        &[
            label("hull", "ropee1", end),
            label("hull", "fale2", Vec3::ZERO),
        ],
        Mat4::IDENTITY,
        "hull",
    );
    let (lines, flags, stats) = collector
        .build(std::slice::from_ref(&sail), &RiggingConfig::default())
        .unwrap();
    assert_eq!(stats.ropes, 1);
    assert_eq!(stats.unmatched_ropes, 1);
    assert!(flags.is_empty());
    let mesh = RigMesh::Ropes(lines);
    let wind = SailWind {
        speed: 12.0,
        direction: 0.5,
    };
    let full = mesh.vertices(wind, 0.0, RigState::uniform(1.0)).unwrap();
    let normal = RigState {
        normal: 1.0,
        full_only: 0.0,
    };
    let gathered = mesh.vertices(wind, 2.0, normal).unwrap();
    let center = |v: &[ModelVertex]| {
        v.iter().map(|v| Vec3::from_array(v.position)).sum::<Vec3>() / SIDES as f32
    };
    assert!(center(&full[..SIDES]).distance(points[2]) < 1e-5);
    assert!(center(&gathered[..SIDES]).distance(sail.gather_anchor(points[2], 0.0)) < 1e-5);
    assert!(center(&gathered[gathered.len() - SIDES..]).distance(end) < 1e-5);
    assert!(center(&gathered[..SIDES]).y > 2.9);
    assert!(mesh.indices().iter().all(|&i| (i as usize) < full.len()));
    assert!(
        gathered
            .iter()
            .all(|v| (Vec3::from_array(v.normal).length() - 1.0).abs() < 1e-5)
    );
}

#[test]
fn both_flag_shapes_keep_the_pole_fixed_and_wave_only_when_windy() {
    for lower in [None, Some(Vec3::new(3.0, 0.0, 0.0))] {
        let flag =
            FlagPatch::new(Vec3::Y, Vec3::ZERO, Vec3::new(3.0, 1.0, 0.0), lower, 4, 8).unwrap();
        let mesh = RigMesh::Flag(flag);
        let wind = SailWind {
            speed: 12.0,
            direction: 1.3,
        };
        let calm = SailWind { speed: 0.0, ..wind };
        let a = mesh.vertices(wind, 0.0, RigState::uniform(1.0)).unwrap();
        let b = mesh.vertices(wind, 1.0, RigState::uniform(0.0)).unwrap();
        for frame in [&a, &b] {
            assert_eq!(frame[0].position, Vec3::Y.to_array());
            assert_eq!(frame[1].position, Vec3::ZERO.to_array());
            assert!(
                frame
                    .iter()
                    .all(|v| (Vec3::from_array(v.normal).length() - 1.0).abs() < 1e-4)
            );
            assert!(
                frame.iter().all(|v| v.uv[0] >= 0.0
                    && v.uv[0] <= 0.25
                    && v.uv[1] >= 0.0
                    && v.uv[1] <= 0.125)
            );
        }
        assert_ne!(a.last().unwrap().position, b.last().unwrap().position);
        let c = mesh.vertices(calm, 0.0, RigState::default()).unwrap();
        let d = mesh.vertices(calm, 12.0, RigState::default()).unwrap();
        for (c, d) in c.iter().zip(d) {
            assert_eq!(c.position, d.position);
        }
        for tri in mesh.indices().chunks_exact(3) {
            let [p, q, r] =
                [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(a[i as usize].position));
            assert!(
                (q - p).cross(r - p).length() > 1e-5,
                "flag tip must not create collapsed triangles"
            );
        }
        assert!(mesh.vertices(wind, f32::NAN, RigState::default()).is_err());
    }
}

#[test]
fn shrouds_use_three_transformed_points_and_incomplete_groups_are_reported() {
    let mut collector = Collector::default();
    let world = Mat4::from_translation(Vec3::new(4.0, 0.0, -2.0));
    collector.add(
        &[
            label("vants", "vant1l", Vec3::ZERO),
            label("vants", "vant1r", Vec3::X * 2.0),
            label("flags", "f1", Vec3::Y),
        ],
        world,
        "hull",
    );
    collector.add(
        &[
            label("vants", "vant1u", Vec3::new(1.0, 6.0, 0.0)),
            label("vants", "vanx2u", Vec3::Y),
        ],
        world,
        "mast",
    );
    let (lines, _, stats) = collector.build(&[], &RiggingConfig::default()).unwrap();
    assert_eq!(stats.shrouds, 1);
    assert_eq!(stats.incomplete_shrouds, 1);
    assert_eq!(stats.incomplete_flags, 1);
    assert!(lines.len() > 10);
    let vertices = RigMesh::Ropes(lines)
        .vertices(SailWind::default(), 0.0, RigState::default())
        .unwrap();
    assert!(
        vertices
            .iter()
            .all(|v| v.position[0] > 3.0 && v.position[2] < -1.0)
    );
}
