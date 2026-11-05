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
    let mut header = source[..72].to_vec();
    let names_end = 72 + word(source, 2);
    let table_end = names_end + word(source, 3) * 4;
    let labels_start =
        table_end + word(source, 4) * 4 + word(source, 5) * 56 + word(source, 6) * 80;
    let labels_end = labels_start + word(source, 7) * 108;
    let mut names = source[72..names_end].to_vec();
    let mut offsets = Vec::new();
    let mut labels = Vec::new();
    for (group, name, position) in [
        ("geometry", "yard", [2.0, 0.0, 0.0]),
        ("sailt1", "sail1", [0.0, 0.0, 0.0]),
    ] {
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
    set(&mut header, 3, word(source, 3) + 4);
    set(&mut header, 7, word(source, 7) + 2);
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
