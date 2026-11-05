use super::*;
#[test]
fn project_ini_and_texture_requests_match_complete_native_reference_snapshots() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../reference/particle_project/cases.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let p = Project::read(&bytes).unwrap();
        let expected = &case["expected"];
        assert_eq!(json!(p.texture.0), expected["texture"]);
        assert_eq!(
            json!(p.systems.iter().map(|s| &s.0).collect::<Vec<_>>()),
            expected["systems"]
        );
        assert_eq!(expected["closed"], 1);
        assert_eq!(expected["geom"], 1);
        assert_eq!(expected["ok"], true);
        let requests = expected["requests"].as_array().unwrap();
        let raw_normal = requests[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let (color, normal) = texture_keys(&p.texture.display()).unwrap();
        assert_eq!(
            normal,
            format!("{}.tx", Text(raw_normal).display()).to_ascii_lowercase()
        );
        let mut expected_color = p.texture.display().replace('\\', "/").to_ascii_lowercase();
        if let Some(rest) = expected_color.strip_prefix("resource/textures/") {
            expected_color = rest.to_owned();
        }
        if !expected_color.ends_with(".tx") {
            expected_color += ".tx";
        }
        assert_eq!(color, expected_color);
    }
    assert_eq!(fixture["missing"]["ok"], false);
    assert_eq!(fixture["missing"]["geom"], 0);
    assert_eq!(
        fixture["default"]["systems"],
        json!([b"Ship_Fire.xps", b"Ship_Smoke.xps"])
    );
    assert_eq!(fixture["default"]["texture"], json!(b"particles_list.tga"));
}
#[test]
fn numbered_systems_use_native_order_and_stop_at_first_missing_or_empty_key() {
    let p=Project::read(b"[Textures]\r\nMainTexture=atlas.tga\r\n[Manager]\r\nSystem_0002=c\r\nSystem_0000=a\r\nSystem_0001=b\r\nSystem_0004=ignored\r\n").unwrap();
    assert_eq!(
        p.systems,
        vec![
            Text(b"a".to_vec()),
            Text(b"b".to_vec()),
            Text(b"c".to_vec())
        ]
    );
    assert_eq!(
        Project::read(b"[Manager]\nSystem_0000=a\nSystem_0001= \t\nSystem_0002=c")
            .unwrap()
            .systems
            .len(),
        1
    );
    assert!(
        Project::read(b"[Manager]\nSystem_0001=b")
            .unwrap()
            .systems
            .is_empty()
    );
}
#[test]
fn first_section_and_first_key_win_and_names_keep_section_whitespace() {
    let p=Project::read(b"[TEXTURES]\nMainTexture= first.tga \nmaintexture=second.tga\n[Manager]\nSystem_0000=a\n[manager]\nSystem_0001=b").unwrap();
    assert_eq!(p.texture, Text(b"first.tga".to_vec()));
    assert_eq!(p.systems.len(), 1);
    assert_eq!(
        Project::read(b"[Textures ]\nMainTexture=ignored")
            .unwrap()
            .texture,
        Text(b"none".to_vec())
    );
    assert_eq!(
        Project::read(b"[Textures]\nMainTexture=\nMainTexture=later")
            .unwrap()
            .texture,
        Text(b"none".to_vec())
    );
}
#[test]
fn native_ini_keeps_inline_semicolons_quotes_and_legacy_value_bytes() {
    let p=Project::read(b"; comment\n[Textures]\nMainTexture=\"atlas\" ; literal\n[Manager]\nSystem_0000=\xe4\xfb\xec\0System_0001=smoke;literal").unwrap();
    assert_eq!(p.texture.0, b"\"atlas\" ; literal");
    assert_eq!(p.systems[0].display(), "дым");
    assert_eq!(p.systems[1].0, b"smoke;literal");
    assert!(
        Project::read(b"[Manager]\nSystem_0000\nSystem_0001=b")
            .unwrap()
            .systems
            .is_empty()
    );
}
#[test]
fn checked_project_input_limits_and_native_sequence_limit_are_enforced() {
    assert!(Project::read(&vec![b'x'; 1_048_577]).is_err());
    assert!(Project::read(&vec![b'\n'; 32768]).is_err());
    let text = format!("[Textures]\nMainTexture={}", "a".repeat(8192));
    assert!(Project::read(text.as_bytes()).is_err());
    let text = format!("[Textures]\nMainTexture={}", "a".repeat(8191));
    assert!(Project::read(text.as_bytes()).is_ok());
    let mut text = "[Manager]\n".to_owned();
    for i in 0..10000 {
        text += &format!("System_{i:04}=s\n");
    }
    assert_eq!(Project::read(text.as_bytes()).unwrap().systems.len(), 9999);
}
#[test]
fn project_and_global_atlas_names_preserve_native_suffix_and_stem_rules() {
    assert_eq!(project_key("AOP.PrJ").unwrap(), "aop.prj");
    assert_eq!(project_key("old.ini").unwrap(), "old.ini.prj");
    for (name, color, normal) in [
        (
            "particles_list.tga",
            "particles_list.tga.tx",
            "particles_listnm.tx",
        ),
        (
            "RESOURCE\\Textures\\effects\\Atlas.TGA",
            "effects/atlas.tga.tx",
            "atlasnm.tx",
        ),
        (
            "effects/atlas.tga.TX",
            "effects/atlas.tga.tx",
            "atlas.tganm.tx",
        ),
        ("Particles_3.txx", "particles_3.txx.tx", "particles_3nm.tx"),
        (".tx", ".tx", ".txnm.tx"),
    ] {
        assert_eq!(texture_keys(name).unwrap(), (color.into(), normal.into()));
    }
    for name in ["../atlas", "/atlas", "C:\\atlas", "x\\..\\atlas", "дым.tga"] {
        assert!(texture_keys(name).is_err());
        assert!(project_key(name).is_err());
    }
}
