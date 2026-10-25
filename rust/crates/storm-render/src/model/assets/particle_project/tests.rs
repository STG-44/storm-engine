use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "storm-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("RESOURCE/Particles")).unwrap();
        fs::create_dir_all(root.join("RESOURCE/Textures/ships/override")).unwrap();
        let f = Self { root };
        let bytes =
            include_bytes!("../../../../../storm-formats/reference/particle_project/billboard.xps");
        for name in ["Ship_Fire.xps", "Ship_Smoke.xps", "other.xps"] {
            fs::write(f.particles().join(name), bytes).unwrap();
        }
        f.texture("particles_list.tga.tx", [0, 0, 255, 255]);
        f.texture("project.tga.tx", [255, 0, 0, 255]);
        f.texture("projectnm.tx", [128, 128, 255, 255]);
        f.texture("ships/override/project.tga.tx", [0, 255, 0, 255]);
        fs::write(f.descriptor(),b"[Textures]\nMainTexture=RESOURCE\\Textures\\project.tga\n[Manager]\nSystem_0000=Ship_Fire\nSystem_0002=other\n").unwrap();
        f
    }
    fn particles(&self) -> PathBuf {
        self.root.join("RESOURCE/Particles")
    }
    fn descriptor(&self) -> PathBuf {
        self.root.join("selected.prj")
    }
    fn texture(&self, name: &str, color: [u8; 4]) {
        let mut bytes: Vec<_> = [0u32, 8, 8, 1, 21, 256]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        for _ in 0..64 {
            bytes.extend([color[2], color[1], color[0], color[3]]);
        }
        fs::write(self.root.join("RESOURCE/Textures").join(name), bytes).unwrap();
    }
    fn builder(&self) -> Builder {
        let mut asset = ModelAsset::ship_demo().unwrap();
        asset.meshes.clear();
        asset.ship_state.fire_started = Some(0.0);
        let root = self.root.join("RESOURCE/Textures");
        let mut paths = BTreeMap::new();
        for name in [
            "particles_list.tga.tx",
            "project.tga.tx",
            "projectnm.tx",
            "ships/override/project.tga.tx",
        ] {
            paths.insert(name.into(), root.join(name));
        }
        Builder {
            asset,
            files: Some(TextureFiles {
                root,
                paths,
                prefixes: vec!["ships/override".into(), String::new()],
            }),
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            single: false,
            rigging: rigging::Collector::default(),
            render_bytes: 0,
        }
    }
}
#[test]
fn selected_project_uses_global_atlas_and_honors_gaps_membership_and_missing_files() {
    let fixture = Fixture::new();
    let mut builder = fixture.builder();
    builder
        .add_xps_fire_effects(&[], Some(&fixture.descriptor()))
        .unwrap();
    assert_eq!(
        builder.asset.meshes.len(),
        1,
        "a gap must not select a later entry or an undeclared default"
    );
    let p = builder.asset.ship.particle_project.as_ref().unwrap();
    assert_eq!(p.available, ["ship_fire.xps"]);
    assert!(p.normal_loaded);
    assert_eq!(p.normal_key, "projectnm.tx");
    let color = &builder.asset.textures[builder.asset.meshes[0].texture].mips[0].rgba;
    assert_eq!(
        &color[..4],
        [255, 0, 0, 255],
        "ship texture overrides must not replace the project atlas"
    );
    assert!(
        fixture
            .builder()
            .add_xps_fire_effects(&["other".into()], Some(&fixture.descriptor()))
            .is_err()
    );
    fs::write(fixture.descriptor(),b"[Textures]\nMainTexture=project.tga\n[Manager]\nSystem_0000=Ship_Fire\nSystem_0001=missing\nSystem_0002=Ship_Smoke").unwrap();
    let mut builder = fixture.builder();
    builder
        .add_xps_fire_effects(&[], Some(&fixture.descriptor()))
        .unwrap();
    assert_eq!(builder.asset.meshes.len(), 2);
    assert_eq!(
        builder.asset.ship.particle_project.unwrap().missing,
        ["missing.xps"]
    );
    let mut empty = fixture.builder();
    empty.asset.ship.fires.clear();
    empty
        .add_xps_fire_effects(&[], Some(Path::new("default")))
        .unwrap();
    assert!(empty.asset.meshes.is_empty());
    assert!(empty.asset.ship.particle_project.is_some());
}
#[test]
fn default_scan_and_named_projects_reject_missing_atlases_ambiguous_names_and_escapes() {
    let fixture = Fixture::new();
    let mut builder = fixture.builder();
    builder
        .add_xps_fire_effects(&[], Some(Path::new("default")))
        .unwrap();
    assert_eq!(
        builder.asset.ship.particle_project.unwrap().available.len(),
        3
    );
    assert!(
        fixture
            .builder()
            .add_xps_fire_effects(&[], Some(Path::new("../selected")))
            .is_err()
    );
    fs::write(
        fixture.descriptor(),
        b"[Textures]\nMainTexture=missing.txx\n",
    )
    .unwrap();
    assert!(
        fixture
            .builder()
            .add_xps_fire_effects(&[], Some(&fixture.descriptor()))
            .is_err()
    );
    fs::write(fixture.particles().join("sHiP_Fire.xps"), b"collision").unwrap();
    let error = fixture
        .builder()
        .add_xps_fire_effects(&[], Some(Path::new("default")))
        .unwrap_err()
        .to_string();
    let aliases = fs::read_dir(fixture.particles())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Ship_Fire.xps")
        })
        .count();
    assert!(if aliases > 1 {
        error.contains("case-colliding")
    } else {
        error.contains("XPS")
    });
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn project_atlas_reaches_the_gpu_and_can_restore_the_default_atlas() {
    use crate::model::{ModelScene, OrbitCamera};
    use crate::{Gpu, create_instance, render_offscreen_with};
    let fixture = Fixture::new();
    let mut default = fixture.builder();
    default
        .add_xps_fire_effects(&[], Some(Path::new("default")))
        .unwrap();
    let mut selected = fixture.builder();
    selected
        .add_xps_fire_effects(&[], Some(&fixture.descriptor()))
        .unwrap();
    let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    let mut camera = OrbitCamera::new(&selected.asset);
    camera.target = selected.asset.ship.fires[0].position;
    camera.distance = 8.0;
    let capture = |asset: &ModelAsset| {
        let mut scene = ModelScene::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, asset).unwrap();
        render_offscreen_with(&gpu, 321, 181, 1.0, |encoder, target| {
            scene.draw(
                &gpu,
                encoder,
                target,
                321,
                181,
                1.0,
                &camera,
                super::super::SailWind::default(),
            )
        })
        .unwrap()
    };
    let blue = capture(&default.asset);
    let red = capture(&selected.asset);
    let red_pixels = |f: &crate::Frame| {
        f.rgba
            .chunks_exact(4)
            .filter(|p| {
                u16::from(p[0]) > u16::from(p[1]) * 2 && u16::from(p[0]) > u16::from(p[2]) * 2
            })
            .count()
    };
    assert!(red_pixels(&red) > red_pixels(&blue) + 20);
    assert_eq!(blue.rgba, capture(&default.asset).rgba);
    assert_eq!(red.rgba, capture(&selected.asset).rgba);
}
