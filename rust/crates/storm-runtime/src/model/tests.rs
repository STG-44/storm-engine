use super::*;
use crate::{Engine, Phase};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use storm_core::{AttributeStore, Value as Field};
use storm_script::{Host, Runtime};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn word(bytes: &[u8], i: usize) -> usize {
    u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()) as usize
}
fn set(bytes: &mut [u8], i: usize, n: usize) {
    bytes[4 * i..4 * i + 4].copy_from_slice(&(n as u32).to_le_bytes());
}

// Extend the licensed upstream cannon fixture with hierarchy/locator metadata.
// Native static models must ignore incomplete cloth locators, while the preview
// continues to diagnose them. Geometry and material records stay original.
fn labelled(source: &[u8]) -> Vec<u8> {
    with_labels(
        source,
        &[
            ("geometry", "yard", [2.0, 0.0, 0.0]),
            ("sailt1", "sail1", [0.0, 0.0, 0.0]),
        ],
    )
}

fn with_labels(source: &[u8], extra: &[(&str, &str, [f32; 3])]) -> Vec<u8> {
    let mut header = source[..72].to_vec();
    let names_end = 72 + word(source, 2);
    let table_end = names_end + word(source, 3) * 4;
    let labels_start =
        table_end + word(source, 4) * 4 + word(source, 5) * 56 + word(source, 6) * 80;
    let labels_end = labels_start + word(source, 7) * 108;
    let mut names = source[72..names_end].to_vec();
    let mut offsets = Vec::new();
    let mut labels = Vec::new();
    for &(group, name, position) in extra {
        let group_id = names.len();
        names.extend_from_slice(group.as_bytes());
        names.push(0);
        let name_id = names.len();
        names.extend_from_slice(name.as_bytes());
        names.push(0);
        offsets.extend_from_slice(&(group_id as u32).to_le_bytes());
        offsets.extend_from_slice(&(name_id as u32).to_le_bytes());
        let mut label = vec![0u8; 108];
        set(&mut label, 0, group_id);
        set(&mut label, 1, name_id);
        let mut matrix = IDENTITY;
        matrix[12..15].copy_from_slice(&position);
        for (i, v) in matrix.into_iter().enumerate() {
            label[(3 + i) * 4..(4 + i) * 4].copy_from_slice(&v.to_le_bytes());
        }
        labels.extend(label);
    }
    set(&mut header, 2, names.len());
    set(&mut header, 3, word(source, 3) + extra.len() * 2);
    set(&mut header, 7, word(source, 7) + extra.len());
    [
        header,
        names,
        source[names_end..table_end].to_vec(),
        offsets,
        source[table_end..labels_end].to_vec(),
        labels,
        source[labels_end..].to_vec(),
    ]
    .concat()
}
fn fixture() -> Result<Files> {
    let root = std::env::temp_dir().join(format!(
        "storm-model-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root)?;
    let files = Files(root);
    std::fs::create_dir_all(files.0.join("RESOURCE/MODELS"))?;
    std::fs::create_dir_all(files.0.join("RESOURCE/Textures/alternate"))?;
    let source = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/modding-tool/Meshes/cannon_x.gm"),
    )?;
    std::fs::write(files.0.join("RESOURCE/MODELS/root.gm"), labelled(&source))?;
    std::fs::write(files.0.join("RESOURCE/MODELS/root_yard.gm"), &source)?;
    // Reuse only texture names from upstream; pixels are newly generated.
    let geometry = storm_formats::geometry::Geometry::read(&source)?;
    for name in geometry.textures {
        let name = if name.ends_with(".tx") {
            name
        } else {
            format!("{name}.tx")
        };
        for (prefix, color) in [("", [0u8, 0, 255, 255]), ("alternate/", [0, 255, 0, 255])] {
            let path = files
                .0
                .join("RESOURCE/Textures")
                .join(format!("{prefix}{}", name.replace('\\', "/")));
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut tx = [0u32, 1, 1, 1, 21, 4]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            tx.extend(color);
            std::fs::write(path, tx)?;
        }
    }
    Ok(files)
}

#[test]
fn configured_sails_and_ropes_follow_cloth_and_have_independent_lifetimes() -> Result<()> {
    let files = fixture()?;
    let root = files.0.join("RESOURCE");
    let body = std::fs::read(root.join("MODELS/root_yard.gm"))?;
    std::fs::write(
        root.join("MODELS/ship.gm"),
        with_labels(
            &body,
            &[
                ("geometry", "spars", [0.0, 6.0, 0.0]),
                ("rigging", "ropeb5", [-1.0, 0.0, 1.0]),
                ("rigging", "falb6", [-1.0, 0.0, -1.0]),
                ("rigging", "ropeb7", [0.0, 2.0, 0.0]),
            ],
        ),
    )?;
    std::fs::write(
        root.join("MODELS/ship_spars.gm"),
        with_labels(
            &body,
            &[
                ("sailt1", "sail1", [0.0, 0.0, 0.0]),
                ("sailt1", "sail2", [4.0, -2.0, 0.0]),
                ("sailt1", "sail3", [0.0, -3.0, 0.0]),
                ("sailn2", "sail1", [0.0, 0.0, 0.0]),
                ("sailn2", "sail2", [4.0, 0.0, 0.0]),
                ("sailn2", "sail3", [4.0, -3.0, 0.0]),
                ("sailn2", "sail4", [0.0, -3.0, 0.0]),
                ("sailn2", "sail0", [2.0, -1.5, 0.4]),
                ("sailn2", "ropee5", [4.0, -3.0, 0.0]),
                ("rigging", "fale6", [1.0, 0.0, 0.0]),
            ],
        ),
    )?;
    std::fs::create_dir(root.join("INI"))?;
    std::fs::write(
        root.join("INI/rigging.ini"),
        "[SAILS]\nWINDVECTOR_QNT = 200\nROLLINGSPEED = 0.01\n",
    )?;
    std::fs::create_dir(root.join("Textures/Ships"))?;
    let mut canvas = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    canvas.extend([240, 240, 240, 255]);
    std::fs::write(root.join("Textures/Ships/parus_common.tga.tx"), &canvas)?;
    std::fs::write(root.join("Textures/sail_rope.tga.tx"), &canvas)?;
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"
        object ship, cloth, lines, missing, weather;
        float maximum, speed, fallback;
        void Main() {
            CreateEntity(&weather,"Weather");
            weather.wind.angle=1.0;
            weather.wind.speed=10.0;
            CreateEntity(&ship,"MODELR");
            CreateEntity(&missing,"MODELR");
            CreateEntity(&cloth,"Sail");
            CreateEntity(&lines,"Rope");
            cloth.MinSpeed=0.05;
            SendMessage(ship,"ls",20500,"ship");
            SendMessage(cloth,"liil",40000,ship,ship,1);
            SendMessage(lines,"lii",40100,ship,ship);
            LayerAddObject(0,cloth,0);
            LayerAddObject(0,lines,1);
            LayerAddObject(1,ship,0);
            LayerAddObject(1,cloth,1);
            LayerAddObject(1,lines,2);
            SetEventHandler("request","Request",0);
            SetEventHandler("remove","Remove",0);
            SetEventHandler("cut","Cut",0);
            SetEventHandler("unrig","Unrig",0);
            SetEventHandler("maximum","MaxSpeed",0);
            SetEventHandler("fallback","FallbackSpeed",0);
        }
        void Request() {
            float value=GetEventData();
            SendMessage(cloth,"lief",40002,ship,&speed,value);
            SendMessage(cloth,"lie",40024,ship,&maximum);
            SendMessage(cloth,"lief",40002,missing,&fallback,1.0);
        }
        void Remove(){ SendMessage(cloth,"li",40030,ship); }
        void Cut(){ SendMessage(lines,"lil",40101,ship,5); }
        void Unrig(){ SendMessage(lines,"li",40102,ship); }
        float MaxSpeed(){ return maximum; }
        float FallbackSpeed(){ return fallback; }
        "#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine =
        crate::Startup::load(&game)?.engine(&game, crate::ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    let step = |engine: &mut Engine, delta| -> Result<(Arc<NativeRigFrame>, Arc<NativeRigFrame>)> {
        engine.frame(delta)?;
        let draws = models.take_draws()?;
        let [body, cloth, ropes] = draws.as_slice() else {
            bail!("expected separate body, cloth and rope draws");
        };
        assert_eq!(body.entity, cloth.entity);
        assert_eq!(body.entity, ropes.entity);
        assert_eq!(
            [body.part, cloth.part, ropes.part],
            [DrawPart::Body, DrawPart::Sails, DrawPart::Ropes]
        );
        assert_eq!(ropes.asset.rigging_stats.ropes, 2);
        assert_eq!(ropes.asset.rigging_stats.unmatched_ropes, 1);
        assert_eq!(body.asset.sails, 0);
        assert_eq!(cloth.asset.sails, 2);
        assert_eq!(cloth.asset.meshes.len(), 2);
        assert!(cloth.asset.meshes.iter().all(|mesh| mesh.sail.is_some()));
        assert_eq!(
            cloth.asset.meshes[0]
                .sail
                .as_ref()
                .unwrap()
                .native_limit_point(),
            Some([2.0, -1.5, 0.4])
        );
        Ok((
            cloth.rig.clone().context("missing cloth frame")?,
            ropes.rig.clone().context("missing rope frame")?,
        ))
    };
    let (full, full_ropes) = step(&mut engine, 16)?;
    assert!(full.sails.values().all(|&value| value == 1.0));
    assert_eq!(
        full.cloth.values().map(Vec::len).collect::<Vec<_>>(),
        [221, 153]
    );
    let (next, _) = step(&mut engine, 16)?;
    assert_ne!(next.cloth, full.cloth);

    let request = |engine: &mut Engine, value| {
        engine.emit_event(
            "request",
            &mut Message::new("f", vec![Field::Float(value)])?,
        )
    };
    let event = |engine: &mut Engine, name| engine.emit_event(name, &mut Message::new("", vec![])?);
    request(&mut engine, 0.5)?;
    let (folding, _) = step(&mut engine, 50)?;
    assert_eq!(
        folding.sails.values().copied().collect::<Vec<_>>(),
        [0.5, 1.0]
    );
    let (partial, partial_ropes) = step(&mut engine, 100)?;
    assert_eq!(
        partial.sails.values().copied().collect::<Vec<_>>(),
        [0.0, 1.0]
    );
    assert_ne!(partial.cloth_indices[&0], full.cloth_indices[&0]);
    assert_eq!(partial.cloth_indices[&1], full.cloth_indices[&1]);
    assert_eq!(full_ropes.ropes[&5].ends[0], [-1.0, 0.0, 1.0]);
    assert_eq!(partial_ropes.ropes[&5].ends[1], [4.0, 6.0, 0.0]);
    assert_ne!(
        full_ropes.ropes[&5].ends[1],
        partial_ropes.ropes[&5].ends[1]
    );
    assert_eq!(
        full_ropes.ropes[&1006].ends,
        partial_ropes.ropes[&1006].ends
    );
    assert!(matches!(
        event(&mut engine, "maximum")?,
        Some(storm_script::Value::Float(1.0))
    ));
    assert!(matches!(
        event(&mut engine, "fallback")?,
        Some(storm_script::Value::Float(0.05))
    ));

    request(&mut engine, 0.0)?;
    let (furled, _) = step(&mut engine, 110)?;
    assert!(furled.sails.values().all(|&value| value == 0.0));
    request(&mut engine, 1.0)?;
    step(&mut engine, 110)?;
    let (reopened, _) = step(&mut engine, 16)?;
    assert!(reopened.sails.values().all(|&value| value == 1.0));

    event(&mut engine, "cut")?;
    let (_, ropes) = step(&mut engine, 16)?;
    assert_eq!(ropes.ropes.keys().copied().collect::<Vec<_>>(), [1006]);
    event(&mut engine, "remove")?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    assert_eq!(draws.len(), 2);
    let ropes = &draws[1].rig.as_ref().unwrap().ropes;
    assert_eq!(ropes.keys().copied().collect::<Vec<_>>(), [1006]);
    event(&mut engine, "unrig")?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    assert_eq!(draws.len(), 1);
    assert!(draws[0].rig.is_none());
    engine.shutdown()
}
const PROGRAM: &str = r#"
object a,b;int frames;
void Main(){
 CreateEntity(&a,"MODELR");CreateEntity(&b,"modelr");
 SendMessage(a,"ls",20500,"ROOT");SendMessage(b,"ls",20500,"root");
 SendMessage(a,"lffffffffffff",20506,1.0,2.0,3.0,1.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,1.0);
 LayerAddObject(1,a,0);LayerAddObject(1,b,1);SetEventHandler("frame","Tick",0);
}
void Tick(){frames++;
 if(frames==2){SendMessage(a,"ls",20509,"alternate\");SendMessage(b,"ls",20500,"root");}
 if(frames==3)DeleteClass(a);
 if(frames==4)LayerFreeze(1,1);
}
"#;

#[test]
fn configured_ship_and_town_flags_use_script_atlas_replies() -> Result<()> {
    let files = fixture()?;
    let root = files.0.join("RESOURCE");
    let body = std::fs::read(root.join("MODELS/root_yard.gm"))?;
    std::fs::write(
        root.join("MODELS/flagship.gm"),
        with_labels(
            &body,
            &[
                ("flag1", "f1", [0.0, 5.0, 0.0]),
                ("flag1", "f2", [2.0, 5.0, 0.0]),
                ("flag1", "f3", [2.0, 4.0, 0.0]),
                ("flag1", "f4", [0.0, 4.0, 0.0]),
                ("sflag2", "f1", [0.0, 6.0, 0.0]),
                ("sflag2", "f2", [2.0, 5.5, 0.0]),
                ("sflag2", "f3", [0.0, 5.0, 0.0]),
            ],
        ),
    )?;
    std::fs::create_dir(root.join("INI"))?;
    std::fs::write(
        root.join("INI/rigging.ini"),
        "[FLAGS]\nTextureName=atlas.tga\nTextureCountColumn=4\nTextureCountRow=8\n",
    )?;
    let mut atlas = [0u32, 4, 8, 1, 21, 128]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    atlas.extend([255; 128]);
    std::fs::write(root.join("Textures/atlas.tga.tx"), atlas)?;
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"
        object ship, town, flag, weather;
        int cell[2], townColumn, shipCalls, townCalls;
        void Main() {
            CreateEntity(&weather,"Weather");
            weather.wind.angle=1.0;
            weather.wind.speed=9.0;
            CreateEntity(&ship,"MODELR");
            CreateEntity(&town,"MODELR");
            CreateEntity(&flag,"Flag");
            ship.index=19;
            SendMessage(ship,"ls",20500,"flagship");
            SendMessage(town,"ls",20500,"flagship");
            SetEventHandler("GetRiggingData","SelectFlag",0);
            SetEventHandler("remove_flags","RemoveFlags",0);
            SetEventHandler("calls","Calls",0);
            SendMessage(flag,"lili",40200,ship,2,ship);
            SendMessage(flag,"lil",40202,town,3);
            LayerAddObject(0,flag,0);
            LayerAddObject(1,ship,0);
            LayerAddObject(1,town,1);
            LayerAddObject(1,flag,2);
        }
        ref SelectFlag() {
            string request=GetEventData();
            int triangle=GetEventData(), nation=GetEventData(), special=GetEventData();
            if(request=="GetShipFlagTexNum") {
                aref character=GetEventData();
                shipCalls=shipCalls+1;
                cell[0]=nation;
                cell[1]=makeint(character.index)-17;
                if(special==1 && triangle==1){cell[1]=-1;}
                return &cell;
            }
            townCalls=townCalls+1;
            townColumn=nation;
            return &townColumn;
        }
        int Calls(){ return shipCalls*10+townCalls; }
        void RemoveFlags(){ SendMessage(flag,"li",40201,ship); }
        "#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine =
        crate::Startup::load(&game)?.engine(&game, crate::ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    let flags = draws
        .iter()
        .filter(|draw| draw.part == DrawPart::Flags)
        .collect::<Vec<_>>();
    let cell = |column, row| storm_sim::FlagTexture::Cell(storm_sim::FlagCell { column, row });
    assert_eq!(flags.len(), 2);
    assert_eq!(flags[0].asset.rigging_stats.flags, 2);
    let ship = flags[0].rig.as_ref().unwrap();
    assert_eq!(ship.flags[&0], cell(2, 2));
    assert_eq!(ship.flags[&1], storm_sim::FlagTexture::Hidden);
    assert_eq!(ship.flag_wind.unwrap().speed, 9.0);
    assert!(
        flags[1]
            .rig
            .as_ref()
            .unwrap()
            .flags
            .values()
            .all(|&t| t == cell(3, 0))
    );
    assert!(matches!(
        engine.emit_event("calls", &mut Message::new("", vec![])?)?,
        Some(storm_script::Value::Int(22))
    ));
    engine.emit_event("remove_flags", &mut Message::new("", vec![])?)?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    assert_eq!(draws.len(), 3);
    assert!(matches!(
        engine.emit_event("calls", &mut Message::new("", vec![])?)?,
        Some(storm_script::Value::Int(22))
    ));
    engine.shutdown()
}

#[test]
fn configured_shroud_variants_follow_models_and_retire_by_mast_and_group() -> Result<()> {
    use storm_render::model::RigMesh;
    use storm_sim::shroud::Kind;

    let files = fixture()?;
    let root = files.0.join("RESOURCE");
    let body = std::fs::read(root.join("MODELS/root_yard.gm"))?;
    std::fs::write(
        root.join("MODELS/rigged.gm"),
        with_labels(
            &body,
            &[
                ("geometry", "mast1", [0.0, 4.0, 0.0]),
                ("geometry", "mast2", [1.0, 8.0, 0.0]),
                ("shrouds", "vant1l", [-2.0, 1.0, 1.0]),
                ("shrouds", "vant1r", [-2.0, 1.0, -1.0]),
                ("shrouds", "vant2l", [2.0, 1.0, 1.0]),
                ("shrouds", "vant2r", [2.0, 1.0, -1.0]),
                ("shrouds", "vanx10l", [-1.0, 1.0, 1.0]),
                ("shrouds", "vanx10r", [-1.0, 1.0, -1.0]),
                ("shrouds", "vanz3l", [1.0, 1.0, 1.0]),
                ("shrouds", "vanz3r", [1.0, 1.0, -1.0]),
                ("shrouds", "vanz4l", [1.0, 1.0, 1.0]),
            ],
        ),
    )?;
    std::fs::write(
        root.join("MODELS/rigged_mast1.gm"),
        with_labels(
            &body,
            &[
                ("shrouds", "vant1u", [0.0, 0.5, 0.0]),
                ("shrouds", "vanx10u", [0.0, 0.0, 0.0]),
            ],
        ),
    )?;
    std::fs::write(
        root.join("MODELS/rigged_mast2.gm"),
        with_labels(
            &body,
            &[
                ("shrouds", "vant2u", [0.0, 0.5, 0.0]),
                ("shrouds", "vanz3u", [0.0, 0.0, 0.0]),
            ],
        ),
    )?;
    std::fs::create_dir(root.join("INI"))?;
    std::fs::write(
        root.join("INI/rigging.ini"),
        "[VANTS]\nTextureName=vant.tga\nfRopeQuant=5\n\
         [VANTS_L]\nTextureName=vanx.tga\nfRopeQuant=3\n\
         [VANTS_Z]\nTextureName=vanz.tga\nfRopeQuant=2\n",
    )?;
    for (name, color) in [
        ("vant", [0, 0, 255, 255]),
        ("vanx", [0, 255, 0, 255]),
        ("vanz", [255, 0, 0, 255]),
    ] {
        let mut tx = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        tx.extend(color);
        std::fs::write(root.join("Textures").join(format!("{name}.tga.tx")), tx)?;
    }
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"
        object ship, standard, narrow, light;
        void Main() {
            CreateEntity(&ship,"MODELR");
            CreateEntity(&standard,"Vant");
            CreateEntity(&narrow,"VantL");
            CreateEntity(&light,"VantZ");
            SendMessage(ship,"ls",20500,"rigged");
            SendMessage(standard,"lii",40300,ship,ship);
            SendMessage(narrow,"lii",40300,ship,ship);
            SendMessage(light,"lii",40300,ship,ship);
            LayerAddObject(0,standard,1);
            LayerAddObject(0,narrow,1);
            LayerAddObject(0,light,1);
            LayerAddObject(1,ship,1);
            LayerAddObject(1,standard,2);
            LayerAddObject(1,narrow,2);
            LayerAddObject(1,light,2);
            SetEventHandler("move_model","Move",0);
            SetEventHandler("detach_mast","Detach",0);
            SetEventHandler("delete_group","DelGroup",0);
            SetEventHandler("reload_model","Reload",0);
        }
        void Move(){ SendMessage(ship,"lffffffffffff",20506,2.0,3.0,4.0,1.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,1.0); }
        void Detach(){ SendMessage(standard,"lis",40302,ship,"MAST1"); }
        void DelGroup(){ SendMessage(narrow,"li",40301,ship); }
        void Reload(){ SendMessage(ship,"ls",20500,"rigged"); }
        "#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine =
        crate::Startup::load(&game)?.engine(&game, crate::ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    let [body, standard, narrow, light] = draws.as_slice() else {
        bail!("expected hull and three independent shroud classes");
    };
    for (draw, kind, vertex_count, color) in [
        (standard, Kind::Vant, 73, [255, 0, 0, 255]),
        (narrow, Kind::VantL, 49, [0, 255, 0, 255]),
        (light, Kind::VantZ, 37, [0, 0, 255, 255]),
    ] {
        assert_eq!(draw.entity, body.entity);
        assert_eq!(draw.part, DrawPart::Shrouds(kind));
        assert_eq!(draw.asset.textures[1].mips[0].rgba, color);
        assert!(
            draw.asset
                .meshes
                .iter()
                .all(|mesh| mesh.vertices.len() == vertex_count)
        );
    }
    let Some(RigMesh::NativeShroud(shroud)) = &narrow.asset.meshes[0].rigging else {
        bail!("expected native shroud mesh");
    };
    assert_eq!(shroud.number, 10);
    assert_eq!(shroud.mast_node, "mast1");
    assert_eq!(
        standard.asset.meshes[1].vertices[0].position,
        [1.0, 8.5, 0.0]
    );
    assert_eq!(light.asset.rigging_stats.incomplete_shrouds, 1);

    let event = |engine: &mut Engine, name| engine.emit_event(name, &mut Message::new("", vec![])?);
    event(&mut engine, "move_model")?;
    event(&mut engine, "detach_mast")?;
    event(&mut engine, "delete_group")?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    let [body, standard, light] = draws.as_slice() else {
        bail!("expected hull and two remaining shroud groups");
    };
    assert_eq!(
        standard
            .rig
            .as_ref()
            .unwrap()
            .removed_shrouds
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0]
    );
    assert_eq!(&body.matrix[12..15], &[2.0, 3.0, 4.0]);
    assert_eq!(body.matrix, standard.matrix);
    assert_eq!(body.matrix, light.matrix);
    event(&mut engine, "reload_model")?;
    engine.frame(16)?;
    let draws = models.take_draws()?;
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].revision, 2);
    engine.shutdown()
}

#[test]
fn configured_renderer_shares_texture_paths_and_keeps_reference_counted_slots() -> Result<()> {
    let files = fixture()?;
    for name in ["ui", "ui/alternate", "ui/top", "ui/corrupt"] {
        let root = files.0.join("RESOURCE/Textures").join(name);
        std::fs::create_dir_all(&root)?;
        let mut bytes = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.extend([255u8, 0, 0, 255]);
        std::fs::write(
            root.join("Panel.tga.tx"),
            if name == "ui/corrupt" {
                &bytes[..1]
            } else {
                &bytes
            },
        )?;
    }
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"#libriary "DX9RENDER_SCRIPT_LIBRIARY"
        native int SetTexturePath(int level, string folder);
        native int GetTexture(string name);
        native void ReleaseTexture(int id);
        object a,b;
        void Main() {
            Trace("bad-level="+SetTexturePath(4,"ignored"));
            SetTexturePath(0,"alternate\");
            SetTexturePath(3,"missing\");
            int first=GetTexture("UI\PANEL.TGA");
            int again=GetTexture("ui\panel.tga");
            Trace("same="+first+":"+again);
            ReleaseTexture(first);
            SetTexturePath(3,"top\");
            int top=GetTexture("ui\panel.tga");
            Trace("top="+top);
            ReleaseTexture(top);
            SetTexturePath(3,"corrupt\");
            int fallback=GetTexture("ui\panel.tga");
            Trace("fallback="+fallback);
            ReleaseTexture(fallback);
            int explicit=GetTexture("ui\panel.tga.tx");
            Trace("suffix="+explicit);
            ReleaseTexture(explicit);
            ReleaseTexture(again);
            int missing=GetTexture("ui\not-present.tga");
            Trace("missing="+missing);
            ReleaseTexture(missing);
            SetTexturePath(3,"");
            CreateEntity(&a,"MODELR");
            SendMessage(a,"ls",20500,"root");
            SetTexturePath(0,"");
            CreateEntity(&b,"MODELR");
            SendMessage(b,"ls",20500,"root");
            int reused=GetTexture("ui\panel.tga");
            Trace("reused="+reused);
            ReleaseTexture(reused);
        }"#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let trace = Rc::new(RefCell::new(Vec::new()));
    let output = trace.clone();
    host.set_trace_sink(move |text| {
        output.borrow_mut().push(text.to_owned());
        Ok(())
    });
    let models = Models::register(&mut host, game.clone())?;
    let mut engine =
        crate::Startup::load(&game)?.engine(&game, crate::ScriptEncoding::Utf8, host)?;
    engine.frame(0)?;
    assert_eq!(
        *trace.borrow(),
        [
            "bad-level=0",
            "same=0:0",
            "top=1",
            "fallback=0",
            "suffix=1",
            "missing=-1",
            "reused=0"
        ],
    );
    let ids = models.live_ids()?;
    assert_eq!(ids.len(), 2);
    for (id, color) in ids.into_iter().zip([[0, 255, 0, 255], [255, 0, 0, 255]]) {
        let draw = models.loaded(id)?.context("missing fixture draw")?;
        assert!(draw.asset.textures.len() > 1);
        assert!(
            draw.asset
                .textures
                .iter()
                .skip(1)
                .all(|t| t.mips[0].rgba == color)
        );
    }
    assert!(
        engine
            .host
            .call("releasetexture", &[storm_script::Value::Int(0)])
            .is_err()
    );
    assert!(
        engine
            .host
            .call(
                "gettexture",
                &[storm_script::Value::String("../escape".into())]
            )
            .is_err()
    );
    engine.shutdown()?;
    Ok(())
}

#[test]
fn original_model_messages_load_static_hierarchies_into_the_shared_script_runtime() -> Result<()> {
    let files = fixture()?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game)?;
    let mut engine = Engine::from_runtime(Runtime::compile(PROGRAM)?, host);
    engine.register_service(models.clone())?;
    engine.frame(16)?;
    let ids = models.live_ids()?;
    assert_eq!(ids.len(), 2);
    let first = models.take_draws()?;
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].entity, ids[0]);
    assert_eq!(first[1].entity, ids[1]);
    let mut matrix = IDENTITY;
    matrix[12..15].copy_from_slice(&[1.0, 2.0, 3.0]);
    assert_eq!(first[0].matrix.map(f32::to_bits), matrix.map(f32::to_bits));
    for draw in &first {
        assert_eq!(draw.revision, 1);
        assert_eq!(draw.asset.geometry_files, 2);
        assert_eq!(draw.asset.sails, 0);
        assert!(draw.asset.textures.len() > 1);
        assert!(
            draw.asset
                .meshes
                .iter()
                .all(|m| m.sail.is_none() && m.rigging.is_none() && m.dynamic.is_none())
        );
        assert!(
            draw.asset
                .textures
                .iter()
                .skip(1)
                .all(|t| t.mips[0].rgba == [255, 0, 0, 255])
        );
    }
    engine.frame(16)?;
    let second = models.take_draws()?;
    assert_eq!(second[0].revision, 1);
    assert_eq!(second[1].revision, 2);
    assert!(Rc::ptr_eq(&first[0].asset, &second[0].asset));
    assert!(!Rc::ptr_eq(&first[1].asset, &second[1].asset));
    assert!(
        second[1]
            .asset
            .textures
            .iter()
            .skip(1)
            .all(|t| t.mips[0].rgba == [0, 255, 0, 255])
    );
    assert!(
        first[1]
            .asset
            .textures
            .iter()
            .skip(1)
            .all(|t| t.mips[0].rgba == [255, 0, 0, 255])
    );
    engine.frame(16)?;
    let third = models.take_draws()?;
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].entity, ids[1]);
    assert!(models.loaded(ids[0])?.is_none());
    assert_eq!(models.live_ids()?, [ids[1]]);
    engine.frame(0)?;
    assert!(models.take_draws()?.is_empty());
    assert!(models.loaded(ids[1])?.is_some());
    engine.shutdown()?;
    assert!(models.live_ids()?.is_empty());
    // The old immutable snapshot remains valid for an already submitted frame.
    assert_eq!(first[0].asset.geometry_files, 2);
    Ok(())
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn location_techniques_change_the_pixels_of_an_existing_model() -> Result<()> {
    let files = fixture()?;
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"#libriary "scriptLocationLibrary"
        native void ExecuteTechnique(string name);
        object model;
        #event_handler("residence","Residence");
        void Main() {
            ExecuteTechnique("amb_jail");
            CreateEntity(&model,"MODELR");
            SendMessage(&model,"ls",20500,"root");
            LayerAddObject(1,&model,0);
        }
        void Residence() { ExecuteTechnique("amb_residence"); }"#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine =
        crate::Startup::load(&game)?.engine(&game, crate::ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    let instance = storm_render::create_instance(wgpu::Backends::PRIMARY)?;
    let gpu = pollster::block_on(storm_render::Gpu::request(&instance, None))?;
    let mut scenes = ModelScenes::default();
    engine.frame(0)?;
    let jail = models.take_draws()?;
    assert_eq!(jail[0].ambient_light, Some([0x20, 0x20, 0x1c]));
    let live = models.live_ids()?;
    scenes.prepare(&gpu, &jail, &live)?;
    let dark = scenes.offscreen(&gpu, 320, 240, 0.0)?;
    engine.emit_event("residence", &mut Message::new("", vec![])?)?;
    engine.frame(0)?;
    let residence = models.take_draws()?;
    assert_eq!(residence[0].ambient_light, Some([0x85, 0x85, 0x80]));
    assert!(Rc::ptr_eq(&jail[0].asset, &residence[0].asset));
    scenes.prepare(&gpu, &residence, &live)?;
    let light = scenes.offscreen(&gpu, 320, 240, 0.0)?;
    let red = |frame: &storm_render::Frame| -> u32 {
        frame
            .rgba
            .chunks_exact(4)
            .map(|pixel| u32::from(pixel[0]))
            .sum()
    };
    assert!(red(&dark) > 0 && red(&light) > red(&dark));
    assert_eq!(scenes.cached(), 1);
    scenes.prepare(&gpu, &jail, &live)?;
    assert_eq!(dark.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    engine.shutdown()?;
    Ok(())
}

#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn script_model_reloads_change_real_gpu_pixels_and_owner_removal_releases_cached_scenes()
-> Result<()> {
    let files = fixture()?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, GameDirectory::open(&files.0)?)?;
    let mut engine = Engine::from_runtime(Runtime::compile(PROGRAM)?, host);
    engine.register_service(models.clone())?;
    let instance = storm_render::create_instance(wgpu::Backends::PRIMARY)?;
    let gpu = pollster::block_on(storm_render::Gpu::request(&instance, None))?;
    let mut scenes = ModelScenes::default();
    engine.frame(16)?;
    let draws = models.take_draws()?;
    let red = draws[1].clone();
    scenes.prepare(&gpu, std::slice::from_ref(&red), &models.live_ids()?)?;
    let before = scenes.offscreen(&gpu, 320, 240, 0.0)?;
    assert!(
        before
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] > p[1] && p[0] > p[2])
            .count()
            > 20
    );
    let mut moved = red.clone();
    moved.matrix[12] += red.asset.radius;
    scenes.prepare(&gpu, &[moved], &models.live_ids()?)?;
    assert_ne!(before.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    scenes.prepare(&gpu, std::slice::from_ref(&red), &models.live_ids()?)?;
    assert_eq!(before.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    engine.frame(16)?;
    let green = models.take_draws()?[1].clone();
    scenes.prepare(&gpu, std::slice::from_ref(&green), &models.live_ids()?)?;
    let after = scenes.offscreen(&gpu, 320, 240, 0.0)?;
    assert_ne!(before.rgba, after.rgba);
    assert_eq!(scenes.cached(), 1);
    assert!(
        after
            .rgba
            .chunks_exact(4)
            .filter(|p| p[1] > p[0] && p[1] > p[2])
            .count()
            > 20
    );
    let mut bad = green.clone();
    bad.matrix[0] = f32::NAN;
    assert!(scenes.prepare(&gpu, &[bad], &models.live_ids()?).is_err());
    assert_eq!(after.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    scenes.prepare(&gpu, std::slice::from_ref(&red), &models.live_ids()?)?;
    assert_eq!(before.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    engine.shutdown()?;
    // A new manager may reuse the same ID/revision. Cache identity must also
    // follow the actual asset, even if a caller retains the preceding snapshot.
    let mut next_host = NativeHost::new()?;
    let next_models = Models::register(&mut next_host, GameDirectory::open(&files.0)?)?;
    let next_program = PROGRAM.replace(
        "SendMessage(b,\"ls\",20500,\"root\");",
        "SendMessage(a,\"ls\",20509,\"alternate\\\");SendMessage(b,\"ls\",20500,\"root\");",
    );
    let mut next = Engine::from_runtime(Runtime::compile(&next_program)?, next_host);
    next.register_service(next_models.clone())?;
    next.frame(16)?;
    let fresh = next_models.take_draws()?[1].clone();
    assert_eq!(fresh.entity, red.entity);
    assert_eq!(fresh.revision, red.revision);
    assert!(!Rc::ptr_eq(&fresh.asset, &red.asset));
    scenes.prepare(&gpu, &[fresh], &next_models.live_ids()?)?;
    assert_eq!(after.rgba, scenes.offscreen(&gpu, 320, 240, 0.0)?.rgba);
    next.shutdown()?;
    scenes.prune(&next_models.live_ids()?);
    assert_eq!(scenes.cached(), 0);
    scenes.prepare(&gpu, &[], &[])?;
    let empty = scenes.offscreen(&gpu, 320, 240, 0.0)?;
    assert!(empty.rgba.chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
    if let Some(folder) = std::env::var_os("STORM_MODEL_CAPTURE_DIR") {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder)?;
        before.save_png(&folder.join("runtime-model-before.png"))?;
        after.save_png(&folder.join("runtime-model-after.png"))?;
        empty.save_png(&folder.join("runtime-model-empty.png"))?;
    }
    Ok(())
}

#[test]
fn static_model_loading_rejects_preview_texture_fallbacks_and_checked_message_failures_preserve_assets()
-> Result<()> {
    let files = fixture()?;
    let game = GameDirectory::open(&files.0)?;
    let path = game.resolve("RESOURCE/MODELS/root.gm")?;
    assert!(
        ModelAsset::load(
            &path,
            &storm_render::model::LoadOptions {
                game_dir: Some(files.0.clone()),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(ModelAsset::load_geometry(&path, &files.0, "missing/").is_err());
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game)?;
    let mut attributes = AttributeStore::default();
    let id = Host::create_entity(&mut host, "MODELR", &mut attributes, None)?;
    let send = |host: &mut NativeHost, code: i32, text: &str| {
        Host::send_message(
            host,
            id,
            &mut Message::new("ls", vec![Field::Long(code), Field::String(text.into())]).unwrap(),
            &mut AttributeStore::default(),
        )
    };
    assert_eq!(send(&mut host, LOAD_GEOMETRY, "root")?, 1);
    let old = models.loaded(id)?.unwrap();
    assert!(send(&mut host, LOAD_GEOMETRY, "../outside").is_err());
    assert!(send(&mut host, 20501, "animation").is_err());
    send(&mut host, SET_TEXTURE_PATH, "missing/")?;
    assert!(send(&mut host, LOAD_GEOMETRY, "root").is_err());
    let current = models.loaded(id)?.unwrap();
    assert_eq!(current.revision, 1);
    assert!(Rc::ptr_eq(&old.asset, &current.asset));
    Host::delete_entity(&mut host, id)?;
    host.entities.new_lifecycle()?;
    assert!(models.live_ids()?.is_empty());
    Ok(())
}

#[test]
fn native_stage_errors_reach_the_engine_owner_without_running_later_stages() -> Result<()> {
    struct Failing;
    impl Entity for Failing {
        fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
            if stage == Stage::Execute {
                bail!("native stage error");
            }
            panic!("Realize must not follow failed Execute");
        }
    }
    let mut host = NativeHost::new()?;
    host.entities
        .register_class("failing_stage", || Ok(Box::new(Failing)))?;
    let source = "object a;void Main(){CreateEntity(&a,\"failing_stage\");LayerAddObject(0,a,0);LayerAddObject(1,a,0);}";
    let mut engine = Engine::from_runtime(Runtime::compile(source)?, host);
    assert!(format!("{:#}", engine.frame(16).unwrap_err()).contains("native stage error"));
    assert_eq!(engine.phase(), Phase::Faulted);
    assert!(engine.frame(16).is_err());
    engine.shutdown()?;
    assert!(engine.host.entities.all_ids().is_empty());
    Ok(())
}

#[test]
fn configured_back_scene_places_models_and_dispatches_the_selected_command() -> Result<()> {
    use crate::{ControlInput, ScriptEncoding, Startup};

    let files = fixture()?;
    let root = files.0.join("RESOURCE");
    let texture_prefix = root.join("Textures/MainMenu");
    let body = std::fs::read(root.join("MODELS/root_yard.gm"))?;
    for texture in storm_formats::geometry::Geometry::read(&body)?.textures {
        let name = if texture.ends_with(".tx") {
            texture
        } else {
            format!("{texture}.tx")
        };
        let name = name.replace('\\', "/");
        let destination = texture_prefix.join(&name);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::copy(root.join("Textures").join(&name), destination)?;
    }
    std::fs::write(
        root.join("MODELS/root_locators.gm"),
        with_labels(
            &body,
            &[
                ("locators", "CAMERA", [0.0, 1.0, -5.0]),
                ("locators", "first", [-1.0, 0.0, 0.0]),
                ("locators", "second", [1.0, 0.0, 0.0]),
                ("locators", "ship", [5.0, 0.0, -2.0]),
            ],
        ),
    )?;
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"
        object scene, menu, ship;
        string command;
        int cameraCalls;
        #event_handler("CameraPosAng","Camera");
        #event_handler("backgroundcommand","Command");
        #event_handler("state","State");
        #event_handler("selected","Selected");
        #event_handler("erase","Erase");

        void Main() {
            CreateEntity(&scene,"InterfaceBackScene");
            LayerAddObject(0,&scene,-1);
            LayerAddObject(1,&scene,1000);
            SendMessage(&scene,"ls",0,"root");
            SendMessage(&scene,"ls",1,"camera");
            SendMessage(&scene,"lsa",2,"Ship",&ship);
            menu.first.locname = "first";
            menu.first.sel = "root_yard";
            menu.first.norm = "root_yard";
            menu.first.event = "New";
            menu.second.locname = "second";
            menu.second.sel = "root_yard";
            menu.second.norm = "root_yard";
            menu.second.event = "Exit";
            SendMessage(&scene,"lla",3,0,&menu);
            MapControl(CreateControl("IDown"),40);
            MapControl(CreateControl("IAction"),13);
        }
        void Camera() { cameraCalls++; }
        void Command() { command = GetEventData(); }
        string State() { return command + ":" + cameraCalls + ":" + sti(ship.Ship.Pos.x); }
        int Selected() { return sti(SendMessage(&scene,"l",5)); }
        void Erase() { DeleteClass(&scene); }
    "#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::new()?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    engine.frame(0)?;
    let camera = engine.host.authored_camera().context("menu camera")?;
    assert_eq!(camera.native_pose(), Some(([0.0, 1.0, -5.1], [0.0; 3])));
    let draws = models.draws()?;
    assert_eq!(draws.len(), 3);
    assert_eq!(draws[1].part, DrawPart::MenuItem(0, true));
    assert_eq!(draws[2].part, DrawPart::MenuItem(1, false));
    assert_eq!(&draws[2].matrix[12..15], &[1.0, 0.0, 0.0]);

    engine.control_input(ControlInput::Button {
        code: 40,
        pressed: true,
    });
    engine.frame(1)?;
    engine.frame(1)?;
    assert_eq!(models.draws()?[2].part, DrawPart::MenuItem(1, true));
    let mut event = Message::new("", vec![])?;
    assert_eq!(
        engine.emit_event("selected", &mut event)?,
        Some(storm_script::Value::Int(1))
    );
    engine.control_input(ControlInput::Button {
        code: 13,
        pressed: true,
    });
    engine.frame(1)?;
    engine.frame(1)?;
    assert_eq!(
        engine.emit_event("state", &mut event)?,
        Some(storm_script::Value::String(":10:5".into())),
    );
    engine.frame(1)?;
    assert_eq!(
        engine.emit_event("state", &mut event)?,
        Some(storm_script::Value::String("Exit:12:5".into())),
    );
    engine.emit_event("erase", &mut event)?;
    engine.frame(1)?;
    assert!(engine.host.authored_camera().is_none());
    assert!(models.draws()?.is_empty());
    Ok(())
}

#[test]
fn configured_ship_mounts_after_nested_script_callbacks_and_executes_on_sea() -> Result<()> {
    use crate::{ScriptEncoding, Startup};
    use storm_core::LayerType;
    use storm_sim::sea::{CELLS, FRAMES};

    let files = fixture()?;
    let models_dir = files.0.join("RESOURCE/MODELS/Ships/TestBoat");
    std::fs::create_dir_all(&models_dir)?;
    std::fs::copy(
        files.0.join("RESOURCE/MODELS/root_yard.gm"),
        models_dir.join("TestBoat.gm"),
    )?;
    let waves = files.0.join("RESOURCE/Sea");
    std::fs::create_dir_all(&waves)?;
    let mut tga = vec![0; 18 + CELLS * 4];
    tga[2] = 2;
    tga[12] = 128;
    tga[14] = 128;
    tga[16] = 32;
    for pixel in tga[18..].chunks_exact_mut(4) {
        pixel.copy_from_slice(&[160, 175, 250, 255]);
    }
    for index in 0..FRAMES {
        std::fs::write(waves.join(format!("sea{index:04}.tga")), &tga)?;
    }
    std::fs::write(files.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        files.0.join("main.c"),
        r#"
        object water, ship, design;
        string callbacks;
        float requested;
        #event_handler("Ship_StartLoad","StartShip");
        #event_handler("SeaGetLayers","Layers");
        #event_handler("ShpCreate","CreatedShip");
        #event_handler("Ship_EndLoad","FinishShip");
        #event_handler("status","Status");
        #event_handler("position","Position");
        #event_handler("erase","Erase");

        void Main() {
            CreateEntity(&water,"Sea");
            LayerAddObject(1,&water,65530);
            water.Sea2.Amp1=1.0;
            water.Sea2.Amp2=0.0;
            water.MaxSeaHeight=1.0;
            design.Name="TestBoat";
            design.WaterLine=0.4;
            design.InertiaAccelerationZ=1.0;
            design.InertiaBrakingZ=0.2;
            ship.index=17;
            ship.Ship.Pos.x=5.0;
            ship.Ship.Pos.z=12.0;
            ship.Ship.Ang.y=0.0;
            ship.Ship.Immersion=0.1;
            ship.Ship.Speed.z=2.0;
            CreateEntity(&ship,"SHIP");
            SendMessage(&ship,"laa",50300,&ship,&design);
            SendMessage(&ship,"le",50325,&requested);
            SendMessage(&ship,"llffffff",50308,0,0.0,0.0,2.0,0.0,0.0,0.0);
        }
        void StartShip() { aref ch=GetEventData(); callbacks="start"+sti(ch.index); }
        void Layers() {
            aref e=GetEventData();
            callbacks=callbacks+":layers";
            SendMessage(e,"lll",51015,2,3);
        }
        void CreatedShip() { int i=GetEventData(); callbacks=callbacks+":create"+i; }
        void FinishShip() { callbacks=callbacks+":end"; }
        string Status() { return callbacks+":"+sti(requested); }
        float Position() { return stf(ship.Ship.Pos.z); }
        void Erase() { DeleteClass(&ship); }
        "#,
    )?;
    let game = GameDirectory::open(&files.0)?;
    let mut host = NativeHost::with_random_seed(7)?;
    let models = Models::register(&mut host, game.clone())?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
    engine.register_service(models.clone())?;
    engine.frame(0)?;
    let ship = engine.host.entities.first_id("SHIP").unwrap();
    assert_eq!(engine.host.entities.layer_ids(LayerType::Execute), [ship]);
    let [model] = models.live_ids()?.try_into().unwrap();
    let mut event = Message::new("", vec![])?;
    assert_eq!(
        engine.emit_event("status", &mut event)?,
        Some(storm_script::Value::String(
            "start17:layers:create17:end:1".into()
        )),
    );
    let first = models.draws()?;
    assert_eq!(first.len(), 1);
    assert_eq!([first[0].matrix[12], first[0].matrix[14]], [5.0, 12.0]);
    assert!((first[0].matrix[13] - (160.0 / 255.0 - 0.5)).abs() < 1e-6);
    engine.frame(100)?;
    let Some(storm_script::Value::Float(z)) = engine.emit_event("position", &mut event)? else {
        panic!("missing world ship position");
    };
    assert!(z > 12.0);
    assert_eq!(models.draws()?[0].matrix[14], 12.0);
    engine.frame(100)?;
    assert!(models.draws()?[0].matrix[14] > 12.0);
    engine.emit_event("erase", &mut event)?;
    engine.frame(0)?;
    assert!(!engine.host.entities.is_valid(model));
    assert!(models.live_ids()?.is_empty() && models.draws()?.is_empty());
    Ok(())
}
