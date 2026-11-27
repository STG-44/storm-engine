use super::*;
use std::sync::Arc;

#[test]
fn configured_weather_applies_settings_and_emits_native_midnight_updates() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object skySettings;
        #event_handler("WeatherTimeUpdate","HourChanged");
        #event_handler("lock_time","LockTime");
        #event_handler("end_weather","EndWeather");
        void Main() {
            CreateEntity(&skySettings,"Weather");
            LayerAddObject(0,&skySettings,10);
            skySettings.Fog.Enable = true;
            skySettings.Fog.Start = 12.5;
            skySettings.Fog.Density = 0.0625;
            skySettings.Fog.Color = argb(255,40,50,60);
            skySettings.Wind.Angle = 1.25;
            skySettings.Wind.Speed = 9.0;
            skySettings.Sun.Color = argb(255,200,180,160);
            skySettings.Sun.Ambient = argb(255,33,55,77);
            skySettings.Time.Time = 23.75;
            skySettings.Time.Speed = 1.0;
            skySettings.Time.UpdateFrequence = 4.0;
            skySettings.isDone = "";
        }
        void HourChanged() {
            float timestamp = GetEventData();
            Trace("hour="+makeint(timestamp*100.0));
            skySettings.Wind.Speed = timestamp;
        }
        void LockTime() {
            skySettings.Time.Speed = 0.0;
            skySettings.Sun.HeightAngle = 0.0;
            skySettings.Sun.AzimuthAngle = 0.0;
            skySettings.isDone = "";
        }
        void EndWeather() { DeleteClass(&skySettings); }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    let night = engine.host.weather_state().unwrap();
    assert!(night.moon_present && !night.sun_present);
    assert!(night.light_position[1] > 0.0);
    assert_eq!((night.wind_angle, night.wind_speed), (1.25, 9.0));
    assert!(night.fog_enabled);
    assert_eq!(
        (night.fog_start, night.fog_density, night.fog_color),
        (12.5, 0.0625, 0xff28323c)
    );
    assert_eq!(night.sun_color, 0xffc8b4a0);
    assert_eq!(engine.host.ambient_light.get(), Some([33, 55, 77]));
    engine.frame(250)?;
    assert_eq!(trace.take(), ["script:hour=2400"]);
    engine.frame(250)?;
    assert_eq!(trace.take(), ["script:hour=25"]);
    let after = engine.host.weather_state().unwrap();
    assert_eq!((after.hour, after.wind_speed), (0.25, 0.25));
    assert_ne!(night.light_position, after.light_position);
    let mut event = Message::new("", vec![])?;
    engine.emit_event("lock_time", &mut event)?;
    engine.frame(250)?;
    let locked = engine.host.weather_state().unwrap();
    assert_eq!(locked.hour, 0.25);
    assert_eq!(locked.light_position, [5000.0, 0.0, 0.0]);
    assert!(trace.take().is_empty());
    engine.emit_event("end_weather", &mut event)?;
    engine.frame(250)?;
    assert!(engine.host.weather_state().is_none());
    Ok(())
}

#[test]
fn configured_sky_swaps_weather_faces_samples_fog_and_drives_unscheduled_astronomy() -> Result<()> {
    let temp = Temporary::new()?;
    let assets = temp.0.join("RESOURCE/Textures");
    for (directory, red) in [
        ("weather/day", 70),
        ("weather/night", 110),
        ("weather/astronomy", 255),
    ] {
        std::fs::create_dir_all(assets.join(directory))?;
        let mut tx = [0u32, 1, 1, 1, 21, 4].map(u32::to_le_bytes).concat();
        tx.extend([10, 20, red, 255]);
        let names: &[_] = if red == 255 {
            &["stars"]
        } else {
            &["sky_fr", "sky_lf", "sky_bk", "sky_rt", "sky_up"]
        };
        for name in names {
            std::fs::write(assets.join(format!("{directory}/{name}.tga.tx")), &tx)?;
        }
    }
    let mut catalog = 1u32.to_le_bytes().to_vec();
    for component in [1.0_f32, 0.1, 1.0] {
        catalog.extend(component.to_le_bytes());
    }
    catalog.extend(*b"G0");
    std::fs::write(temp.0.join("RESOURCE/hic.dat"), catalog)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object weather, sky, astronomy;
        int fogCalls, tint;
        #event_handler("WhrCalcFogColor", "FogColor");
        #event_handler("night", "Night");
        #event_handler("midnight", "Midnight");
        #event_handler("remove_sky", "RemoveSky");
        #event_handler("fog_count", "FogCount");
        void Main() {
            CreateEntity(&weather,"Weather");
            weather.Time.Time=6.0;
            CreateEntity(&astronomy,"Astronomy");
            astronomy.Stars.Catalog="hic.dat";
            astronomy.Stars.Texture="weather\\astronomy\\stars.tga";
            astronomy.Stars.Enable=true;
            astronomy.Stars.Spectr.G=argb(0,255,255,255);
            astronomy.isDone=true;
            CreateEntity(&sky,"Sky");
            LayerAddObject(1,&sky,3);
            sky.Dir.d0="weather\\day\\";
            sky.Dir.d1="weather\\night\\";
            sky.Dir=6.0;
            sky.Size=2048.0;
            sky.Angle=0.5;
            sky.RotateSpeed=0.1;
            tint=44;
            sky.isDone="";
        }
        int FogColor() {
            float x=GetEventData();
            float y=GetEventData();
            float z=GetEventData();
            fogCalls++;
            return argb(255-makeint(y/8.0),tint,20,40);
        }
        void Night() { weather.Time.Time=18.0; tint=88; sky.TimeUpdate=18.0; }
        void Midnight() { weather.Time.Time=24.0; }
        void RemoveSky() { DeleteClass(&sky); }
        int FogCount() { return fogCalls; }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.frame(0)?;
    let first = engine.host.sky_draw().unwrap();
    assert!(first.has_textures() && !first.celestial);
    assert_eq!((first.blend, first.angle), (0.5, 0.5));
    assert_eq!(first.current.0[0].as_ref().unwrap().mips[0].rgba[0], 70);
    assert_eq!(first.next.0[4].as_ref().unwrap().mips[0].rgba[0], 110);
    let colors = first.fog.as_ref().unwrap();
    assert_eq!(colors.len(), 513);
    assert_eq!(
        (colors[0], colors[512]),
        ([44, 20, 40, 255], [44, 20, 40, 5])
    );
    assert!(engine.host.astronomy_draws().is_empty());
    let mut event = Message::new("", Vec::new())?;
    assert!(matches!(
        engine.emit_event("fog_count", &mut event)?,
        Some(storm_script::Value::Int(513))
    ));
    engine.frame(100)?;
    let daylight = engine.host.sky_draw().unwrap();
    assert!(daylight.celestial);
    assert!((daylight.angle - 0.51).abs() < 1e-6);
    assert!(std::rc::Rc::ptr_eq(colors, daylight.fog.as_ref().unwrap()));
    assert_eq!(engine.host.astronomy_draws().len(), 1);
    engine.emit_event("night", &mut event)?;
    engine.frame(100)?;
    let night = engine.host.sky_draw().unwrap();
    assert!(std::rc::Rc::ptr_eq(&first.next, &night.current));
    assert!(std::rc::Rc::ptr_eq(&first.current, &night.next));
    assert_eq!(night.fog.unwrap()[0][0], 88);
    assert!(matches!(
        engine.emit_event("fog_count", &mut event)?,
        Some(storm_script::Value::Int(1026))
    ));
    engine.emit_event("midnight", &mut event)?;
    engine.frame(16)?;
    let midnight = engine.host.sky_draw().unwrap();
    assert_eq!(midnight.blend, 0.0);
    assert!(std::rc::Rc::ptr_eq(&first.current, &midnight.current));
    engine.emit_event("remove_sky", &mut event)?;
    engine.frame(16)?;
    assert!(engine.host.sky_draw().is_none());
    assert!(engine.host.astronomy_draws().is_empty());
    Ok(())
}

#[test]
fn configured_astronomy_stages_the_catalog_and_planets_with_weather_fades() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("RESOURCE/Textures/Weather/Astronomy");
    std::fs::create_dir_all(textures.join("Planets"))?;
    let mut texture = [0u32, 1, 1, 1, 21, 4].map(u32::to_le_bytes).concat();
    texture.extend([255; 4]);
    std::fs::write(textures.join("Stars.tga.tx"), &texture)?;
    std::fs::write(textures.join("Planets/Jupiter.tga.tx"), &texture)?;
    let mut catalog = 2u32.to_le_bytes().to_vec();
    for [ra, dec, magnitude] in [[std::f32::consts::FRAC_PI_2, 0.0, 1.0], [-1.0, 0.0, 2.0]] {
        for field in [ra, dec, magnitude] {
            catalog.extend(field.to_le_bytes());
        }
        catalog.extend(*b"g2");
    }
    std::fs::write(temp.0.join("RESOURCE/HIC.dat"), catalog)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object night, astronomy;
        #event_handler("fade", "StartFade");
        #event_handler("morning", "Morning");
        #event_handler("night", "Night");
        #event_handler("delete", "RemoveAstronomy");
        void Main() {
            CreateEntity(&night, "WEATHER");
            night.Time.Time=20.0;
            CreateEntity(&astronomy, "Astronomy");
            LayerAddObject(1, &astronomy, 5);
            astronomy.TimeScale=1.0;
            astronomy.Stars.Enable=true;
            astronomy.Stars.Catalog="hic.dat";
            astronomy.Stars.Texture="weather\\astronomy\\stars.tga";
            astronomy.Stars.Spectr.G=argb(0,255,220,100);
            astronomy.Stars.TelescopeMagnitude=10.0;
            astronomy.Planets.Planet.Jupiter.Distance=5.20441;
            astronomy.Planets.Planet.Jupiter.Diameter=143760.0;
            astronomy.Planets.Planet.Jupiter.Scale=4.0;
            astronomy.isDone=true;
        }
        void StartFade() {
            astronomy.Stars.FadeStartTime=20.0;
            astronomy.Stars.FadeTime=-2.0;
            astronomy.Planets.FadeStartTime=20.0;
            astronomy.Planets.FadeTime=-2.0;
            astronomy.TimeUpdate=1;
            night.Time.Time=21.0;
        }
        void Morning() { night.Time.Time=22.0; }
        void Night() {
            astronomy.Stars.Radius=3000.0;
            astronomy.Stars.FadeStartTime=-1.0;
            astronomy.Planets.FadeStartTime=-1.0;
            astronomy.TimeUpdate=1;
        }
        void RemoveAstronomy() { DeleteClass(&astronomy); }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(
        &game,
        ScriptEncoding::Utf8,
        NativeHost::with_random_seed(31)?,
    )?;
    engine.frame(0)?;
    let first = engine.host.astronomy_draws();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].magnitudes, Some([10.0, 8.5]));
    assert_eq!(first[0].sprites.len(), 1);
    assert_eq!(first[1].magnitudes, None);
    assert_eq!(first[1].sprites.len(), 1);
    engine.frame(50)?;
    let stable = engine.host.astronomy_draws();
    for (before, after) in first.iter().zip(&stable) {
        assert!(std::rc::Rc::ptr_eq(&before.sprites, &after.sprites));
    }

    let mut event = Message::new("", Vec::new())?;
    engine.emit_event("fade", &mut event)?;
    engine.frame(16)?;
    let fading = engine.host.astronomy_draws();
    assert_eq!(fading.len(), 2);
    for (before, after) in first.iter().zip(&fading) {
        assert_ne!(before.sprites, after.sprites);
    }
    engine.emit_event("morning", &mut event)?;
    engine.frame(16)?;
    assert!(engine.host.astronomy_draws().is_empty());
    engine.emit_event("night", &mut event)?;
    engine.frame(16)?;
    let next = engine.host.astronomy_draws();
    assert_eq!(next.len(), 2);
    assert_ne!(first[0].sprites, next[0].sprites);
    assert_eq!(first[1].sprites, next[1].sprites);
    engine.emit_event("delete", &mut event)?;
    engine.frame(16)?;
    assert!(engine.host.astronomy_draws().is_empty());
    Ok(())
}

#[test]
fn configured_sun_glow_loads_original_texture_requests_and_switches_to_the_moon() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("RESOURCE/Textures/Weather");
    std::fs::create_dir_all(&textures)?;
    for (name, red) in [
        ("Sun", 40),
        ("Moon", 50),
        ("Halo", 60),
        ("Flares", 70),
        ("Flash", 80),
    ] {
        let mut texture = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        texture.extend([1, 2, red, 255]);
        std::fs::write(textures.join(format!("{name}.tga.tx")), texture)?;
    }
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object weather, sun;
        #event_handler("night", "Night");
        #event_handler("clear", "Clear");
        void Main() {
            CreateEntity(&weather, "Weather");
            weather.Sun.HeightAngle=0.2;
            weather.Sun.AzimuthAngle=0.1;
            weather.isDone="";
            CreateEntity(&sun, "SunGlow");
            LayerAddObject(0, &sun, -2);
            LayerAddObject(1, &sun, -2);
            sun.Glow.Dist=3500.0;
            sun.Glow.SunSize=400;
            sun.Glow.MoonSize=110;
            sun.Glow.Size=1250;
            sun.Glow.Color=argb(255,120,200,250);
            sun.Glow.DecayTime=8.0;
            sun.Glow.SunTexture="weather\\sun.tga";
            sun.Glow.MoonTexture="weather\\moon.tga.tx";
            sun.Glow.GlowTexture="weather\\halo.tga";
            sun.Flare.Dist=3500.0;
            sun.Flare.Scale=2.0;
            sun.Flare.TexSizeX=2;
            sun.Flare.TexSizeY=2;
            sun.Flare.Texture="weather\\flares.tga";
            sun.Flares.f0="0.8,250,2,0x503020";
            sun.Overflow.Size=3700.0;
            sun.Overflow.Start=0.7;
            sun.Overflow.Color=argb(0,100,100,100);
            sun.Overflow.Texture="weather\\flash.tga";
            sun.isDone=true;
        }
        void Night() {
            sun.Moon=true;
            weather.Sun.AzimuthAngle=0.5;
            weather.isDone="";
            sun.isDone=true;
        }
        void Clear() { sun.Clear=""; DeleteAttribute(&sun,""); }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.frame(0)?;
    let first = engine.host.sun_draws().remove(0);
    assert_eq!(first.distance, 3500.0);
    assert_eq!(first.halo_alpha, 0.0);
    assert!(first.flares.is_none());
    let first_disc = first.disc.unwrap();
    assert_eq!(first_disc.texture.mips[0].rgba, [40, 2, 1, 255]);
    assert_eq!(first_disc.color, [120, 200, 250]);
    assert_eq!(first_disc.size, 400.0);
    assert_eq!(first.halo.unwrap().texture.mips[0].rgba[0], 60);
    assert_eq!(first.overflow.unwrap().sprite.texture.mips[0].rgba[0], 80);
    engine.frame(50)?;
    let day = engine.host.sun_draws().remove(0);
    assert!((day.halo_alpha - 0.4).abs() < 1e-6);
    assert!(std::rc::Rc::ptr_eq(
        &first_disc.texture,
        &day.disc.unwrap().texture
    ));
    let flares = day.flares.unwrap();
    assert_eq!(flares.grid, [2, 2]);
    assert_eq!(flares.atlas.mips[0].rgba[0], 70);
    assert_eq!(flares.entries[0].tile, 2);
    assert_eq!(flares.entries[0].color, [80, 48, 32]);
    assert!((flares.alpha - 0.16).abs() < 1e-6);
    let mut event = Message::new("", Vec::new())?;
    engine.emit_event("night", &mut event)?;
    engine.frame(16)?;
    let night = engine.host.sun_draws().remove(0);
    assert_ne!(night.direction, first.direction);
    assert_eq!(
        night.direction,
        engine.host.weather_state().unwrap().light_position
    );
    assert!(night.halo.is_none());
    let moon = night.disc.unwrap();
    assert_eq!(moon.size, 110.0);
    assert_eq!(moon.texture.mips[0].rgba[0], 50);
    engine.emit_event("clear", &mut event)?;
    engine.frame(16)?;
    assert!(engine.host.sun_draws().is_empty());
    Ok(())
}

#[test]
fn configured_rain_generates_and_animates_streaks_only_while_visible() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object atmosphere, rain;
        #event_handler("hide", "HideRain");
        #event_handler("show", "ShowRain");
        #event_handler("storm", "StartStorm");
        #event_handler("clear", "ClearRain");
        void Main() {
            CreateEntity(&atmosphere, "Weather");
            atmosphere.Wind.Angle = 0.0;
            atmosphere.Wind.Speed = 18.0;
            CreateEntity(&rain, "rain");
            LayerAddObject(1, &rain, 10);
            rain.NumDrops = 2;
            rain.Color = argb(0,50,70,90);
            rain.DropLength = 1.0;
            rain.Height = 3.0;
            rain.Radius = 5.0;
            rain.Speed = 6.0;
            rain.TimeBlend = 100;
            rain.MaxBlend = 50;
            rain.isDone = "";
        }
        void HideRain() { SendMessage(&rain,"ll",1,0); }
        void ShowRain() { SendMessage(&rain,"ll",1,1); }
        void StartStorm() { rain.NumDrops = 50000; rain.isDone = ""; }
        void ClearRain() { rain.Clear = ""; rain.isDone = ""; }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.frame(0)?;
    let initial = engine.host.rain_draws().remove(0);
    assert_eq!(initial.starts.len(), 2);
    assert_eq!(initial.length, 1.0);
    assert_eq!(initial.rgb, [50, 70, 90]);
    assert_eq!(
        initial.blocks.map(|block| block.origin[1]),
        [0.0, 3.0, 1.5, 4.5]
    );
    assert!(
        initial
            .blocks
            .iter()
            .all(|block| block.alpha == 50 && block.tilt == 1.0)
    );
    assert!((initial.blocks[0].origin[2] + 32.4).abs() < 1e-5);
    for &[x, y, z] in initial.starts.iter() {
        assert!((1.5..=6.5).contains(&x.hypot(z)));
        assert!((-3.0..=3.0).contains(&y));
    }

    engine.frame(60)?;
    let mut event = Message::new("", vec![])?;
    engine.emit_event("hide", &mut event)?;
    engine.frame(250)?;
    assert!(engine.host.rain_draws().is_empty());
    engine.emit_event("show", &mut event)?;
    engine.frame(0)?;
    let resumed = engine.host.rain_draws().remove(0);
    assert!(Arc::ptr_eq(&initial.starts, &resumed.starts));
    assert!((resumed.blocks[0].origin[1] + 1.2).abs() < 1e-5);
    assert!((resumed.blocks[0].origin[2] + 31.2).abs() < 1e-5);

    engine.frame(120)?;
    engine.frame(0)?;
    let recycled = engine.host.rain_draws().remove(0);
    assert_eq!(recycled.blocks[0].alpha, 0);
    assert!((recycled.blocks[0].origin[1] - 1.4).abs() < 1e-5);
    assert!((recycled.blocks[0].origin[2] + 32.4).abs() < 1e-5);
    engine.emit_event("storm", &mut event)?;
    engine.frame(0)?;
    let storm = engine.host.rain_draws().remove(0);
    assert_eq!(storm.starts.len(), 50_000);
    assert!(!Arc::ptr_eq(&storm.starts, &resumed.starts));
    engine.emit_event("clear", &mut event)?;
    engine.frame(0)?;
    assert!(engine.host.rain_draws().is_empty());
    Ok(())
}

#[test]
fn configured_ship_lights_load_script_catalog_and_follow_native_masts_and_yards() -> Result<()> {
    use crate::ship_lights::ShipLightPose;
    use std::collections::BTreeMap;
    use storm_formats::geometry::Label;
    use storm_sim::sea::camera::Matrix;

    fn translated(x: f32, y: f32, z: f32) -> Matrix {
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
        ]
    }

    let temp = Temporary::new()?;
    let texture_dir = temp.0.join("Resource/Textures/ShipsFlares");
    std::fs::create_dir_all(&texture_dir)?;
    let mut texture = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    texture.extend([61, 83, 210, 255]);
    std::fs::write(texture_dir.join("corona.tga.tx"), &texture)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object lights, ship;
        #event_handler("remove_ship","RemoveShip");
        void Main() {
            lights.LightTypes.default.light.g = 0.9;
            lights.LightTypes.default.light.range = 15.0;
            lights.LightTypes.default.light.Attenuation1 = 0.3;
            lights.LightTypes.flare.corona.Size = 0.5;
            lights.CoronaTexture = "ShipsFlares\corona.tga";
            lights.MaxD3DLights = 2;
            CreateEntity(&lights,"ShipLights");
            LayerAddObject(0,&lights,0);
            SendMessage(&lights,"l",50201);
            CreateEntity(&ship,"test_body");
        }
        void RemoveShip() { DeleteClass(&ship); }
    "#,
    )?;
    struct Body;
    impl Entity for Body {}
    let game = GameDirectory::open(&temp.0)?;
    let mut host = NativeHost::with_random_seed(17)?;
    host.entities
        .register_class("test_body", || Ok(Box::new(Body)))?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
    engine.frame(0)?;
    let ship = engine.host.entities.first_id("test_body").unwrap();
    assert!(engine.host.ship_corona_texture(false).is_none());
    let nodes = BTreeMap::from([
        ("mast2".to_owned(), translated(10.0, 0.0, 0.0)),
        ("rey_b2".to_owned(), translated(20.0, 0.0, 0.0)),
        ("rey_b301".to_owned(), translated(60.0, 0.0, 0.0)),
    ]);
    let root = translated(100.0, 0.0, 0.0);
    let pose = ShipLightPose {
        root: &root,
        nodes: &nodes,
    };
    let labels = [
        ("lights", "L1", 1.0),
        ("lights", "LM2", 3.0),
        ("flares", "FM2b", 1.0),
        ("flares", "FR301b30101", 1.0),
        ("flares", "FR405", 1.0), // Missing mast405.
        ("reload", "L0", 1.0),
    ]
    .map(|(group, name, x)| Label {
        group: group.into(),
        name: name.into(),
        matrix: translated(x, 2.0, 3.0),
    });
    assert_eq!(engine.host.attach_ship_lights(ship, &labels, &pose)?, 4);
    assert_eq!(
        engine.host.ship_corona_texture(false).unwrap().mips[0].rgba,
        [210, 83, 61, 255]
    );
    assert!(engine.host.ship_coronas().is_empty());
    engine.host.reset_ship_lights(true);
    let before = engine.host.ship_coronas();
    let points = engine.host.ship_point_lights(ship, [0.0; 3]);
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].position, [3.0, 2.0, 3.0]);
    assert_eq!(points[1].position, [101.0, 2.0, 3.0]);
    assert_eq!(points[0].range, 15.0);
    assert_eq!(points[0].attenuation, [1.0, 0.3, 0.0]);

    let updated = BTreeMap::from([
        ("mast2".to_owned(), translated(15.0, 0.0, 0.0)),
        ("rey_b2".to_owned(), translated(18.0, 0.0, 0.0)),
        ("rey_b301".to_owned(), translated(66.0, 0.0, 0.0)),
    ]);
    engine.host.update_ship_light_pose(
        ship,
        &ShipLightPose {
            root: &root,
            nodes: &updated,
        },
    )?;
    engine.frame(125)?;
    let after = engine.host.ship_coronas();
    assert_eq!(after[1].position, [8.0, 2.0, 3.0]);
    assert_eq!(after[2].position, [-1.0, 2.0, 3.0]);
    assert_eq!(after[3].position, [7.0, 2.0, 3.0]);
    assert_ne!(before[2].size, after[2].size);
    engine.host.reset_ship_lights(false);
    assert!(engine.host.ship_coronas().is_empty());
    engine.host.reset_ship_lights(true);
    engine.emit_event("remove_ship", &mut Message::new("", vec![])?)?;
    engine.frame(16)?;
    assert!(engine.host.ship_coronas().is_empty());
    assert!(engine.host.ship_point_lights(ship, [0.0; 3]).is_empty());
    Ok(())
}

#[test]
fn configured_sea_reads_all_frames_and_obeys_script_weather_and_realize_layers() -> Result<()> {
    use storm_sim::sea::{CELLS, FRAMES};

    let temp = Temporary::new()?;
    let waves = temp.0.join("RESOURCE").join("Sea");
    std::fs::create_dir_all(&waves)?;
    let mut tga = vec![0; 18 + CELLS * 4];
    tga[2] = 2;
    tga[12] = 128;
    tga[14] = 128;
    tga[16] = 32;
    for index in 0..FRAMES {
        for pixel in tga[18..].chunks_exact_mut(4) {
            pixel.copy_from_slice(&[(index * 3) as u8, 175, 250, 255]);
        }
        std::fs::write(waves.join(format!("sea{index:04}.tga")), &tga)?;
    }
    let coast = temp.0.join("RESOURCE/Foam/Locations");
    let textures = temp.0.join("RESOURCE/Textures/Weather/CoastFoam");
    std::fs::create_dir_all(&coast)?;
    std::fs::create_dir_all(&textures)?;
    std::fs::write(
        coast.join("Harbor.ini"),
        "NumFoams=1\nFoamDivides=4\n[Foam_0]\nType=1\nkey_0=0,0,0,3\nkey_1=6,0,6,3\n",
    )?;
    let mut tex = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    tex.extend([255, 255, 255, 128]);
    std::fs::write(textures.join("Foam.tga.tx"), tex)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object water, coast, missing;
        #event_handler("freeze","FreezeSea");
        #event_handler("refresh","RefreshSea");
        #event_handler("stop","StopSea");
        #event_handler("resume","ResumeSea");
        #event_handler("erase","EraseSea");
        void Main() {
            CreateEntity(&water,"sea");
            LayerAddObject(1,&water,65530);
            water.Sea2.Amp1=6.0;
            water.Sea2.Amp2=2.0;
            water.MaxSeaHeight=4.0;
            water.Sea2.AnimSpeed1=4.0;
            water.Sea2.AnimSpeed2=2.0;
            coast.id="harbor";
            coast.Edit=false;
            CreateEntity(&coast,"CoastFoam");
            LayerAddObject(1,&coast,65531);
            missing.id="empty";
            CreateEntity(&missing,"CoastFoam");
            LayerAddObject(1,&missing,65531);
        }
        void FreezeSea() { LayerFreeze(1,true); }
        void RefreshSea() { water.isDone.refresh = ""; }
        void StopSea() { LayerFreeze(1,false); water.Stop=true; }
        void ResumeSea() { water.Stop=false; }
        void EraseSea() { DeleteClass(&water); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.frame(0)?;
    let id = engine
        .host
        .entities
        .first_id("sea")
        .expect("registered SEA");
    let start = engine.host.sea_draw().expect("first SEA draw");
    assert_eq!(engine.host.entities.ids("CoastFoam").len(), 2);
    let draws = engine.host.coast_foam_draws();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].strip.layout().origin, [0.0, 3.0]);
    assert_eq!(draws[0].height, 0.2);
    assert_eq!(
        draws[0].texture.mips[0].rgba.as_slice(),
        &[255, 255, 255, 128]
    );
    assert_eq!(start.parameters.amplitude, [3.0, 1.0]);
    assert_eq!(start.animation.frame, [0.0, 10.0]);
    assert!((start.sample(12.0, -53.0)?.height - 30.0 / 255.0).abs() < 1e-7);
    engine.frame(250)?;
    let surface = engine.host.sea_draw().expect("advancing SEA draw");
    assert_eq!(surface.animation.frame, [1.0, 10.5]);
    assert!((surface.sample(12.0, -53.0)?.height - 40.5 / 255.0).abs() < 1e-7);

    let mut event = Message::new("", vec![])?;
    for name in ["freeze", "stop"] {
        engine.emit_event(name, &mut event)?;
        engine.frame(250)?;
        assert!(engine.host.sea_draw().is_none());
        assert!(engine.host.coast_foam_draws().is_empty());
        assert_eq!(
            engine.host.sea_surface(id).unwrap().animation.frame,
            [1.0, 10.5]
        );
        engine.emit_event("refresh", &mut event)?;
        assert_eq!(
            engine.host.sea_draw().map(|sea| sea.animation.frame),
            (name == "freeze").then_some([1.0, 10.5])
        );
    }
    engine.emit_event("resume", &mut event)?;
    engine.frame(250)?;
    assert_eq!(engine.host.sea_draw().unwrap().animation.frame, [2.0, 11.0]);
    assert_eq!(engine.host.coast_foam_draws().len(), 1);
    engine.emit_event("erase", &mut event)?;
    engine.frame(250)?;
    assert!(engine.host.sea_surface(id).is_none());
    assert!(engine.host.sea_draw().is_none());
    assert!(engine.host.coast_foam_draws().is_empty());
    Ok(())
}

#[test]
fn scrshoter_retries_and_signals_after_publication_in_realize_order() -> Result<()> {
    use storm_formats::save_thumbnail::{RGBA_BYTES, SIDE};

    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    let border = temp.0.join("RESOURCE/Textures/interfaces");
    std::fs::create_dir_all(&border)?;
    let mut texture = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    texture.extend([0, 0, 200, 128]);
    std::fs::write(border.join("EmptyBorder.tga.tx"), texture)?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
            object camera;
            #event_handler("makescrshot","PictureReady");
            #event_handler("Control Activation","KeyDown");
            void Main() {
                CreateEntity(&camera,"SCRSHOTER");
                LayerAddObject(1,&camera,-1);
                Trace("initial="+sti(SendMessage(&camera,"l",45042)));
                int key = CreateControl("MenuKey");
                MapControl(key,65);
            }
            void PictureReady() {
                string info = "previous";
                int handle = sti(SendMessage(&camera,"l",45042));
                int preview = sti(SendMessage(&camera,"lsse",45043,"","NewSave",&info));
                Trace("capture="+handle+":"+preview+":"+info);
            }
            void KeyDown() { Trace("key"); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    assert_eq!(
        trace.take(),
        [
            "script:initial=-1",
            "script:ERROR!!! screen shot create error",
            "script:capture=-1:-1:"
        ]
    );
    engine.control_input(ControlInput::Button {
        code: 65,
        pressed: true,
    });
    engine.frame_with_screenshot(16, &mut |_| {
        Ok(Some(storm_render::Frame {
            width: SIDE,
            height: SIDE,
            rgba: [20, 40, 60, 5].repeat(RGBA_BYTES / 4),
        }))
    })?;
    assert_eq!(trace.take(), ["script:capture=0:0:", "script:key"]);
    let image = engine.host.screenshot_image(0).expect("captured texture");
    assert!(
        image
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [110, 20, 30, 191])
    );
    engine.frame_with_screenshot(16, &mut |_| panic!("this entity already has a screenshot"))?;
    assert!(trace.take().is_empty());
    engine.shutdown()?;
    assert!(engine.host.screenshot_image(0).is_none());
    Ok(())
}

#[test]
fn interface_loads_legacy_options_and_reports_an_empty_file_synchronously() -> Result<()> {
    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI/INTERFACES");
    std::fs::create_dir_all(&ini)?;
    std::fs::write(ini.join("interfaces.ini"), "[PC_SCREEN]\n")?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
            #libriary "script_interface_functions"
            native void XI_SetColorCorrection(float contrast, float gamma, float brightness);
            native void XI_ControlMakeInvert(string name, int flag);
            native void XI_SetMouseSensitivity(float x, float y);
            object xi, options;
            int broken;
            #event_handler("evntOptionsBreak","OnEmpty");
            void OnEmpty() { broken++; }
            void Main() {
                CreateEntity(&xi,"xinterface");
                options.volume.sound=0.5;
                options.volume.music=0.5;
                SendMessage(&xi,"lsa",45053,"missing",&options);
                Trace("missing="+broken+":"+options.volume.sound);
                SendMessage(&xi,"lsa",45053,"OPTIONS",&options);
                Trace("loaded="+broken+":"+options.volume.sound+":"+options.volume.music);
                Trace(options.StartGameParam.PlayerProfile+":"+options.note);
                SendMessage(&xi,"lsa",45053,"empty",&options);
                Trace("empty="+broken+":"+options.volume.sound);
                options.StartGameParam.MOD_SKILL_ENEMY_RATE=8;
                SendMessage(&xi,"lsa",45052,"OPTIONS",&options);
                object restored;
                SendMessage(&xi,"lsa",45053,"options",&restored);
                Trace("saved="+restored.StartGameParam.MOD_SKILL_ENEMY_RATE+":"+restored.StartGameParam.PlayerProfile);
                XI_SetColorCorrection(0.5,2.0,32.0);
                int mouseX = CreateControl("CameraX");
                int mouseY = CreateControl("CameraY");
                MapControl(mouseX,256);
                MapControl(mouseY,257);
                XI_ControlMakeInvert("camerax",0);
                XI_ControlMakeInvert("cameray",1);
                XI_SetMouseSensitivity(0.5,2.0);
            }
        "#,
    )?;
    std::fs::write(
        temp.0.join("options"),
        "  volume.sound=0.75\r\n\tStartGameParam.PlayerProfile=Капитан\r\nnote= a=b ",
    )?;
    std::fs::write(temp.0.join("empty"), [])?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.control_input(ControlInput::MouseMotion { x: 10.0, y: -6.0 });
    engine.frame(0)?;
    assert_eq!(
        trace.take(),
        [
            "script:missing=0:0.5000000",
            "script:loaded=0:0.75:0.5000000",
            "script:Капитан: a=b ",
            "script:empty=1:0.75",
            "script:saved=8:Капитан",
        ]
    );
    assert_eq!(
        std::fs::read_to_string(temp.0.join("options"))?.to_ascii_lowercase(),
        "volume.sound=0.75\nvolume.music=0.5000000\nstartgameparam.playerprofile=Капитан\nstartgameparam.mod_skill_enemy_rate=8\nnote= a=b "
    );
    assert_eq!(
        engine.host.color_correction(),
        Some(storm_render::color::ColorCorrection {
            contrast: 0.5,
            gamma: 2.0,
            brightness: 32.0,
        })
    );
    assert_eq!(engine.host.mouse_axis("CameraX"), Some(-5.0));
    assert_eq!(engine.host.mouse_axis("CameraY"), Some(-12.0));
    Ok(())
}

#[test]
fn configured_post_logo_callback_can_rebuild_controls_and_clean_up_scene_entities() -> Result<()> {
    const PROGRAM: &str = r#"
        object first, second, debug;
        int frames;
        #event_handler("frame","Main_Start");
        void Main() {}
        void Main_Start() {
            frames++;
            if(frames != 2) return;
            int total=0;
            for(int i=0; i<30000; i++) { total=total+1; }
            CreateEntity(&first,"particles");
            CreateEntity(&second,"PARTICLES");
            CreateEntity(&debug,"TMPTELEPORT");
            first.name="first";
            second.name="second";
            aref found;
            Trace("first="+GetEntity(found,"PARTICLES")+":"+found.name);
            Trace("missing-profile="+sti(SendMessage(&first,"lsffffffl",10000,"missing",
                0.0,0.0,0.0,0.0,0.0,0.0,0)));
            SendMessage(&first,"ll",10010,1);
            SendMessage(&second,"l",10006);
            DeleteEntitiesByType("PaRtIcLeS");
            Trace("lookup="+GetEntity(&found,"particles")+":"+IsEntity(found));
            Trace("done="+total+":"+IsEntity(first)+":"+IsEntity(second)+":"+IsEntity(debug));
        }
    "#;
    let mut bounded = Engine::from_runtime(Runtime::compile(PROGRAM)?, NativeHost::new()?);
    bounded.frame(0)?;
    let error = bounded.frame(16).unwrap_err();
    assert!(format!("{error:#}").contains("script instruction budget exhausted"));

    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("main.c"), PROGRAM)?;
    std::fs::write(
        temp.0.join("engine.ini"),
        "run=main.c\n[controls]\nondebugkeys=0\n",
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        [
            "script:first=1:first",
            "script:missing-profile=0",
            "script:lookup=0:0",
            "script:done=30000:0:0:1",
        ]
    );
    assert!(engine.host.entities.ids("particles").is_empty());
    engine.shutdown()?;
    Ok(())
}

#[test]
fn fader_tips_use_the_game_border_and_the_retained_progress_frame_setting() -> Result<()> {
    for (setting, has_texture, enabled) in
        [("", true, true), ("0", true, false), ("4", false, true)]
    {
        let temp = Temporary::new()?;
        let textures = temp.0.join("Resource/Textures");
        std::fs::create_dir_all(textures.join("Interfaces"))?;
        for (name, rgba) in [
            ("Panel.tga.tx", [180, 0, 0, 255]),
            ("Backdrop.tga.tx", [0, 60, 0, 255]),
            ("Interfaces/Int_Border.tga.tx", [0, 0, 220, 128]),
        ] {
            if !has_texture && name.starts_with("Interfaces/") {
                continue;
            }
            let mut bytes = [0u32, 1, 1, 1, 21, 4]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            let [r, g, b, a] = rgba;
            bytes.extend([b, g, r, a]);
            std::fs::write(textures.join(name), bytes)?;
        }
        let mut ini = String::from("run=main.c\n");
        if !setting.is_empty() {
            ini.push_str(&format!("ProgressFrame={setting}\n"));
        }
        std::fs::write(temp.0.join("engine.ini"), ini)?;
        std::fs::write(
            temp.0.join("main.c"),
            r#"
            #libriary "scriptLocationLibrary"
            native void SetReloadNextTipsImage();
            native void ReloadProgressStart();
            native void ReloadProgressEnd();
            object fade;
            #event_handler("start_progress","StartProgress");
            #event_handler("end_progress","EndProgress");
            #event_handler("backdrop","Backdrop");
            void Main() {
                SetReloadNextTipsImage();
                Trace("before="+Rand(32767));
                CreateEntity(&fade,"Fader");
                SetReloadNextTipsImage();
                Trace("after="+Rand(32767));
                SendMessage(&fade,"ls",33003,"panel.tga");
                Backdrop();
                SendMessage(&fade,"lfl",33000,1.0,0);
            }
            void StartProgress() { ReloadProgressStart(); }
            void EndProgress() { ReloadProgressEnd(); }
            void Backdrop() { SendMessage(&fade,"ls",33004,"backdrop.tga"); }
            "#,
        )?;
        let game = GameDirectory::open(&temp.0)?;
        let trace = Trace::default();
        let mut host = NativeHost::with_random_seed(1)?;
        let sink = trace.clone();
        host.set_trace_sink(move |line| {
            record(&sink, line);
            Ok(())
        });
        let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
        engine.frame(500)?;
        engine.frame(0)?;
        assert_eq!(
            trace.take(),
            [
                "before=41",
                if enabled { "after=6334" } else { "after=18467" },
            ]
        );
        let fade = engine.host.fade_draws().unwrap()[0]
            .pictures
            .clone()
            .unwrap();
        assert_eq!(fade.border.is_some(), enabled && has_texture);
        if let Some(border) = &fade.border {
            assert_eq!(border.mips[0].rgba, [0, 0, 220, 128]);
        }

        let mut args = Message::new("", vec![])?;
        engine.emit_event("start_progress", &mut args)?;
        let session = engine.host.loading_progress().unwrap().pictures.unwrap();
        assert!(match (&fade.border, &session.border) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        });
        engine.emit_event("end_progress", &mut args)?;
        engine.emit_event("backdrop", &mut args)?;
        engine.frame(0)?;
        let faded = engine.host.fade_draws().unwrap()[0]
            .pictures
            .clone()
            .unwrap();
        assert!(faded.border.is_none());
        assert!(faded.foreground.is_some());
        engine.shutdown()?;
    }
    Ok(())
}

#[test]
fn infohandler_uses_the_default_border_unless_the_script_chooses_an_empty_name() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("Resource/Textures");
    std::fs::create_dir_all(textures.join("Interfaces"))?;
    let mut bytes = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    bytes.extend([0, 200, 20, 128]);
    std::fs::write(textures.join("Panel.tga.tx"), &bytes)?;
    std::fs::write(textures.join("Interfaces/Int_Border.tga.tx"), &bytes)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\nProgressFrame=0\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "scriptLocationLibrary"
        native void ReloadProgressStart();
        native void ReloadProgressEnd();
        #event_handler("start_progress","StartProgress");
        #event_handler("end_progress","EndProgress");
        #event_handler("no_border","NoBorder");
        object first, second;
        void Main() {
            first.picfilename="panel.tga";
            CreateEntity(&first,"InfoHandler");
            SendMessage(&first,"l",1);
        }
        void NoBorder() {
            second.picfilename="panel.tga";
            second.picborderfilename="";
            CreateEntity(&second,"InfoHandler");
            SendMessage(&second,"l",1);
        }
        void StartProgress() { ReloadProgressStart(); }
        void EndProgress() { ReloadProgressEnd(); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let mut args = Message::new("", vec![])?;
    engine.frame(0)?;
    let info = engine.host.loading_draw().unwrap();
    assert_eq!(
        info.border.as_ref().unwrap().mips[0].rgba,
        [20, 200, 0, 128]
    );
    engine.emit_event("start_progress", &mut args)?;
    let progress = engine.host.loading_progress().unwrap().pictures.unwrap();
    assert!(Arc::ptr_eq(
        info.border.as_ref().unwrap(),
        progress.border.as_ref().unwrap()
    ));
    engine.emit_event("end_progress", &mut args)?;
    engine.emit_event("no_border", &mut args)?;
    assert!(engine.host.loading_draw().unwrap().border.is_none());
    engine.emit_event("start_progress", &mut args)?;
    assert!(
        engine
            .host
            .loading_progress()
            .unwrap()
            .pictures
            .unwrap()
            .border
            .is_none()
    );
    Ok(())
}

#[test]
fn fader_startframe_uses_one_snapshot_per_transition_and_ignores_late_requests() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object fade;
        #event_handler("restart","Restart");
        #event_handler("snapshot","Snapshot");
        void Main() {
            CreateEntity(&fade,"Fader");
            Restart();
            Snapshot();
        }
        void Restart() { SendMessage(&fade,"lfl",33000,1.0,0); }
        void Snapshot() { SendMessage(&fade,"l",33002); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let token = |engine: &Engine| {
        engine
            .host
            .fade_draws()
            .unwrap()
            .first()
            .and_then(|fade| fade.frozen_frame.clone())
    };
    engine.frame(500)?;
    let first = token(&engine).unwrap();
    assert_eq!(engine.host.fade_opacity(), 0.0);
    for (delta, opacity) in [(500, 0.5), (0, 1.0)] {
        engine.frame(delta)?;
        assert!(Arc::ptr_eq(&token(&engine).unwrap(), &first));
        assert_eq!(engine.host.fade_opacity(), opacity);
    }
    engine.frame(0)?;
    assert!(token(&engine).is_none());

    let mut args = Message::new("", vec![])?;
    engine.emit_event("restart", &mut args)?;
    engine.frame(500)?;
    engine.emit_event("snapshot", &mut args)?;
    engine.frame(0)?;
    assert!(token(&engine).is_none());
    assert_eq!(engine.host.fade_opacity(), 0.5);

    engine.emit_event("restart", &mut args)?;
    engine.emit_event("snapshot", &mut args)?;
    engine.frame(0)?;
    assert!(!Arc::ptr_eq(&token(&engine).unwrap(), &first));
    engine.shutdown()?;
    Ok(())
}

#[test]
fn fader_picture_messages_update_the_fade_and_seed_the_next_loading_screen() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("Resource/Textures/Loading");
    std::fs::create_dir_all(&textures)?;
    for (name, red) in [("Backdrop", 60), ("Panel", 120), ("Alternate", 180)] {
        let mut bytes = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.extend([0, 0, red, 255]);
        std::fs::write(textures.join(format!("{name}.tga.tx")), bytes)?;
    }
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "scriptLocationLibrary"
        native void ReloadProgressStart();
        native void ReloadProgressEnd();
        object fade;
        #event_handler("start_progress","StartProgress");
        #event_handler("end_progress","EndProgress");
        #event_handler("replace_pictures","ReplacePictures");
        #event_handler("clear_foreground","ClearForeground");
        void Main() {
            CreateEntity(&fade,"Fader");
            SendMessage(&fade,"ls",33004,"loading\backdrop.tga");
            SendMessage(&fade,"ls",33003,"loading\panel.tga");
            SendMessage(&fade,"lfl",33000,1.0,0);
        }
        void StartProgress() { ReloadProgressStart(); }
        void EndProgress() { ReloadProgressEnd(); }
        void ReplacePictures() {
            SendMessage(&fade,"ls",33004,"loading\missing.tga");
            SendMessage(&fade,"ls",33003,"loading\alternate.tga");
        }
        void ClearForeground() { SendMessage(&fade,"ls",33003,""); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let colors = |pictures: &storm_render::loading::LoadingPictures| {
        [
            pictures.background.as_ref().map(|tex| tex.mips[0].rgba[0]),
            pictures.foreground.as_ref().map(|tex| tex.mips[0].rgba[0]),
        ]
    };
    let mut args = Message::new("", vec![])?;
    engine.frame(500)?;
    engine.frame(0)?;
    {
        let draws = engine.host.fade_draws().unwrap();
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].opacity, 0.5);
        assert_eq!(
            colors(draws[0].pictures.as_ref().unwrap()),
            [Some(60), Some(120)]
        );
    }
    engine.emit_event("start_progress", &mut args)?;
    let loading = engine.host.loading_progress().unwrap().pictures.unwrap();
    assert_eq!(colors(&loading), [Some(60), Some(120)]);
    engine.emit_event("replace_pictures", &mut args)?;
    engine.frame(0)?;
    let replace = engine.host.fade_draws().unwrap()[0]
        .pictures
        .clone()
        .unwrap();
    assert_eq!(colors(&replace), [None, Some(180)]);
    let active = engine.host.loading_progress().unwrap().pictures.unwrap();
    assert!(Rc::ptr_eq(&active, &loading));

    engine.emit_event("end_progress", &mut args)?;
    engine.emit_event("start_progress", &mut args)?;
    assert!(engine.host.loading_progress().unwrap().pictures.is_none());
    engine.emit_event("end_progress", &mut args)?;
    engine.emit_event("clear_foreground", &mut args)?;
    engine.frame(0)?;
    assert!(engine.host.fade_draws().unwrap()[0].pictures.is_none());
    engine.shutdown()?;
    Ok(())
}

#[test]
fn fader_draws_timed_transitions_posts_live_entity_references_and_auto_deletes() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object fade;
        #event_handler("FaderEvent_StartFade","OutStart");
        #event_handler("FaderEvent_EndFade","OutEnd");
        #event_handler("FaderEvent_StartFadeIn","InStart");
        #event_handler("FaderEvent_EndFadeIn","InEnd");
        #event_handler("reveal","Reveal");
        #event_handler("instant","Instant");
        void Main() {
            fade.kind="scene";
            CreateEntity(&fade,"Fader");
            SendMessage(&fade,"lfl",33000,1.0,0);
        }
        void Reveal() { SendMessage(&fade,"lfl",33001,1.0,1); }
        void Instant() {
            object unbound=CreateClass("Fader");
            SendMessage(&unbound,"lfl",33000,0.0,1);
        }
        void OutStart() {
            int direction=GetEventData(); aref actor=GetEventData();
            string kind="unbound";
            if (CheckAttribute(actor,"kind")) {
                kind=actor.kind;
                actor.notified=1;
            }
            Trace("out start:"+direction+":"+IsEntity(actor)+":"+kind);
        }
        void OutEnd() {
            int direction=GetEventData(); aref actor=GetEventData();
            Trace("out end:"+direction+":"+IsEntity(actor)+":"+CheckAttribute(fade,"notified"));
        }
        void InStart() {
            int direction=GetEventData(); aref actor=GetEventData();
            Trace("in start:"+direction+":"+actor.kind);
        }
        void InEnd() {
            int direction=GetEventData(); aref actor=GetEventData();
            Trace("in end:"+direction+":"+IsEntity(actor));
        }
        "#,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    for (delta, opacity, event) in [
        (500, 0.0, None),
        (500, 0.5, None),
        (0, 1.0, Some("script:out start:0:1:scene")),
        (0, 1.0, None),
        (0, 1.0, Some("script:out end:0:1:1")),
    ] {
        engine.frame(delta)?;
        assert_eq!(engine.host.fade_opacity(), opacity);
        assert_eq!(trace.take().as_slice(), event.as_slice());
    }
    let id = engine.host.entities.first_id("Fader").unwrap();
    engine.emit_event("reveal", &mut Message::new("", Vec::new())?)?;
    for (delta, opacity, event) in [
        (500, 1.0, None),
        (500, 0.5, None),
        (0, 0.0, Some("script:in start:1:scene")),
        (0, 0.0, None),
        (0, 0.0, Some("script:in end:1:1")),
    ] {
        engine.frame(delta)?;
        assert_eq!(engine.host.fade_opacity(), opacity);
        assert_eq!(trace.take().as_slice(), event.as_slice());
        assert!(engine.host.entities.is_valid(id));
    }
    engine.frame(0)?;
    assert!(!engine.host.entities.is_valid(id));
    assert!(trace.take().is_empty());

    engine.emit_event("instant", &mut Message::new("", Vec::new())?)?;
    engine.frame(0)?;
    assert_eq!(engine.host.fade_opacity(), 0.0);
    engine.frame(0)?;
    assert_eq!(engine.host.fade_opacity(), 1.0);
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:out start:0:1:unbound"]);
    engine.shutdown()?;
    assert_eq!(engine.host.fade_opacity(), 0.0);
    Ok(())
}

#[test]
fn reload_progress_commands_present_inside_the_script_and_retain_the_last_picture() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("Resource/Textures");
    std::fs::create_dir_all(&textures)?;
    let mut bytes = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    bytes.extend([30, 20, 10, 255]);
    std::fs::write(textures.join("Backdrop.tga.tx"), bytes)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "scriptLocationLibrary"
        native void ReloadProgressStart();
        native void ReloadProgressUpdate();
        native void ReloadProgressEnd();
        object loading;
        int frames;
        #event_handler("frame","Tick");
        void Main() {
            loading.picbackfilename="backdrop.tga";
            CreateEntity(&loading,"InfoHandler");
            LayerSetRealize(13);
            LayerAddObject(13,&loading,0);
        }
        void Tick() {
            frames++;
            if (frames==2) {
                LayerFreeze(13,true);
                ReloadProgressStart();
                ReloadProgressStart();
                Trace("loading");
            }
            if (frames==3) ReloadProgressUpdate();
            if (frames==4) ReloadProgressEnd();
            if (frames==5) {
                ReloadProgressStart();
                ReloadProgressEnd();
                Trace("loaded");
            }
        }
        "#,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    let sink = trace.clone();
    engine.host.set_progress_presenter(move |progress| {
        record(&sink, format!("present:{}", progress.pictures.is_some()));
        Ok(())
    });
    engine.frame(0)?;
    let picture = engine.host.loading_draw().unwrap();
    engine.frame(16)?;
    assert_eq!(trace.take(), ["present:true", "script:loading"]);
    assert!(engine.host.loading_draw().is_none());
    let progress = engine.host.loading_progress().unwrap();
    assert!(Rc::ptr_eq(progress.pictures.as_ref().unwrap(), &picture));
    engine.frame(16)?;
    assert!(engine.host.loading_progress().is_some());
    trace.take();
    engine.frame(16)?;
    assert!(engine.host.loading_progress().is_none());
    engine.frame(16)?;
    assert_eq!(trace.take(), ["present:false", "script:loaded"]);
    assert!(engine.host.loading_progress().is_none());
    engine.shutdown()?;
    Ok(())
}

#[test]
fn reload_progress_uses_the_script_foreground_until_the_session_ends() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("Resource/Textures");
    std::fs::create_dir_all(&textures)?;
    for (name, red) in [
        ("Panel", 60),
        ("Backdrop", 80),
        ("Frame", 100),
        ("Selected", 120),
    ] {
        let mut bytes = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.extend([0, 0, red, 255]);
        std::fs::write(textures.join(format!("{name}.tga.tx")), bytes)?;
    }
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "scriptLocationLibrary"
        native void SetReloadProgressBackImage(string picture);
        native void ReloadProgressStart();
        native void ReloadProgressUpdate();
        native void ReloadProgressEnd();
        object loading;
        #event_handler("preview","Preview");
        #event_handler("select_picture","SelectPicture");
        #event_handler("select_missing","SelectMissing");
        #event_handler("clear_picture","ClearPicture");
        #event_handler("start_progress","StartProgress");
        #event_handler("end_progress","EndProgress");
        void Main() { SelectPicture(); }
        void Preview() {
            loading.picfilename="panel.tga";
            loading.picbackfilename="backdrop.tga";
            loading.picborderfilename="frame.tga";
            CreateEntity(&loading,"InfoHandler");
            SendMessage(&loading,"l",1);
        }
        void SelectPicture() { SetReloadProgressBackImage("selected.tga"); }
        void SelectMissing() { SetReloadProgressBackImage("missing.tga"); }
        void ClearPicture() { SetReloadProgressBackImage(""); }
        void StartProgress() {
            ReloadProgressStart();
            ReloadProgressUpdate();
        }
        void EndProgress() { ReloadProgressEnd(); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let current_pictures =
        |engine: &Engine| engine.host.loading_progress().unwrap().pictures.unwrap();
    let mut args = Message::new("", vec![])?;
    engine.frame(0)?;
    engine.emit_event("start_progress", &mut args)?;
    let standalone = current_pictures(&engine);
    assert_eq!(
        standalone.foreground.as_ref().unwrap().mips[0].rgba,
        [120, 0, 0, 255]
    );
    assert!(standalone.background.is_none());
    engine.emit_event("end_progress", &mut args)?;

    engine.emit_event("preview", &mut args)?;
    let original = engine.host.loading_draw().unwrap();
    engine.emit_event("select_picture", &mut args)?;
    engine.emit_event("start_progress", &mut args)?;
    let overridden = current_pictures(&engine);
    assert_eq!(
        overridden.foreground.as_ref().unwrap().mips[0].rgba,
        [120, 0, 0, 255]
    );
    assert!(Arc::ptr_eq(
        original.background.as_ref().unwrap(),
        overridden.background.as_ref().unwrap(),
    ));
    assert!(Arc::ptr_eq(
        original.border.as_ref().unwrap(),
        overridden.border.as_ref().unwrap(),
    ));
    assert!(Rc::ptr_eq(&original, &engine.host.loading_draw().unwrap()));

    engine.emit_event("clear_picture", &mut args)?;
    engine.emit_event("start_progress", &mut args)?;
    assert!(Rc::ptr_eq(&overridden, &current_pictures(&engine)));
    engine.emit_event("end_progress", &mut args)?;
    engine.emit_event("start_progress", &mut args)?;
    assert!(Rc::ptr_eq(&original, &current_pictures(&engine)));
    engine.emit_event("end_progress", &mut args)?;

    for selection in ["select_missing", "clear_picture"] {
        engine.emit_event(selection, &mut args)?;
        engine.emit_event("start_progress", &mut args)?;
        let pictures = current_pictures(&engine);
        assert!(pictures.foreground.is_none());
        assert!(pictures.border.is_none());
        assert!(Arc::ptr_eq(
            original.background.as_ref().unwrap(),
            pictures.background.as_ref().unwrap(),
        ));
        engine.emit_event("end_progress", &mut args)?;
    }
    Ok(())
}

#[test]
fn infohandler_loads_configured_pictures_and_respects_realize_membership() -> Result<()> {
    let temp = Temporary::new()?;
    let textures = temp.0.join("Resource/Textures");
    std::fs::create_dir_all(textures.join("Loading"))?;
    for (file, rgba) in [
        ("Loading/Panel.tga.tx", [10, 20, 30, 255]),
        ("Loading/Backdrop.tga.tx", [40, 50, 60, 255]),
        ("Loading/Frame.tga.tx", [70, 80, 90, 64]),
    ] {
        let mut bytes = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let [r, g, b, a] = rgba;
        bytes.extend([b, g, r, a]);
        std::fs::write(textures.join(file), bytes)?;
    }
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object loading;
        #event_handler("hide_picture","Hide");
        #event_handler("show_picture","Show");
        #event_handler("remove_picture","Remove");
        void Main() {
            loading.picfilename="loading\panel.tga";
            loading.picbackfilename="loading\backdrop.tga";
            loading.picborderfilename="loading\frame.tga";
            loading.infoStr="An optional caption";
            CreateEntity(&loading,"InfoHandler");
            LayerSetRealize(13);
            LayerAddObject(13,&loading,-1);
        }
        void Hide() { LayerFreeze(13,true); }
        void Show() { SendMessage(&loading,"l",1); }
        void Remove() { DeleteClass(&loading); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.frame(0)?;
    let pictures = engine.host.loading_draw().unwrap();
    assert_eq!(
        pictures.foreground.as_ref().unwrap().mips[0].rgba,
        [10, 20, 30, 255]
    );
    assert_eq!(
        pictures.background.as_ref().unwrap().mips[0].rgba,
        [40, 50, 60, 255]
    );
    assert_eq!(
        pictures.border.as_ref().unwrap().mips[0].rgba,
        [70, 80, 90, 64]
    );
    assert!(pictures.text.is_none());
    engine.frame(16)?;
    assert!(Rc::ptr_eq(&pictures, &engine.host.loading_draw().unwrap()));

    let mut args = Message::new("", vec![])?;
    engine.emit_event("hide_picture", &mut args)?;
    engine.frame(16)?;
    assert!(engine.host.loading_draw().is_none());
    engine.emit_event("show_picture", &mut args)?;
    assert!(Rc::ptr_eq(&pictures, &engine.host.loading_draw().unwrap()));
    engine.emit_event("remove_picture", &mut args)?;
    engine.frame(16)?;
    assert!(engine.host.loading_draw().is_none());
    assert!(engine.host.entities.first_id("InfoHandler").is_none());
    Ok(())
}

#[test]
fn missing_videos_finish_through_execute_or_the_script_queue() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object video;
        int count;
        #event_handler("next_logo","StartVideo");
        #event_handler("unfreeze","UnfreezeVideo");
        #event_handler("resume_time","ResumeTime");
        void StartVideo() {
            count++;
            SetEventHandler("ievntEndVideo","EndVideo",0);
            CreateEntity(&video,"CAviPlayer");
            SendMessage(&video,"ll",49002,1);
            string resource = "logos\missing" + count + ".wmv";
            SendMessage(&video,"ls",49000,resource);
            Trace("start="+count);
            if (count==2) LayerFreeze(10,true);
        }
        void EndVideo() {
            Trace("finish="+count+";delta="+GetDeltaTime());
            DeleteClass(&video);
            DelEventHandler("ievntEndVideo","EndVideo");
            LayerFreeze(10,false);
            if (count<3) PostEvent("next_logo",1);
        }
        void UnfreezeVideo() { LayerFreeze(10,false); }
        void ResumeTime() { SetTimeScale(1); }
        void Main() {
            StartVideo();
            LayerFreeze(10,true);
            SetTimeScale(0);
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:start=1"]);
    let player = engine.host.entities.first_id("CAviPlayer").unwrap();
    assert!(
        engine
            .host
            .entities
            .layer_ids(storm_core::LayerType::Realize)
            .contains(&player)
    );

    let mut message = Message::new("", vec![])?;
    engine.emit_event("unfreeze", &mut message)?;
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:finish=1;delta=0"]);
    assert!(engine.host.entities.first_id("CAviPlayer").is_none());
    engine.frame(16)?;
    assert!(trace.take().is_empty());
    engine.emit_event("resume_time", &mut message)?;
    engine.frame(1)?;
    assert_eq!(trace.take(), ["script:start=2"]);
    engine.frame(1)?;
    assert_eq!(trace.take(), ["script:finish=2;delta=1"]);
    engine.frame(1)?;
    assert_eq!(trace.take(), ["script:start=3", "script:finish=3;delta=1"]);
    engine.frame(16)?;
    assert!(trace.take().is_empty());
    assert!(engine.host.entities.first_id("CAviPlayer").is_none());
    Ok(())
}

#[test]
fn available_videos_report_the_missing_decoder() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::create_dir_all(temp.0.join("Resource/Videos"))?;
    std::fs::write(temp.0.join("Resource/Videos/AkellaLogo.WMV"), b"fixture")?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object video;
        void Main() {
            CreateEntity(&video,"CAviPlayer");
            SendMessage(&video,"ls",49000,"akellalogo.wmv");
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let error = engine.frame(16).unwrap_err();
    assert!(
        format!("{error:#}")
            .contains("video decoding is unavailable for RESOURCE/VIDEOS/akellalogo.wmv")
    );
    Ok(())
}

#[test]
fn configured_renderer_records_glow_from_the_original_script_library() -> Result<()> {
    use storm_render::GlowParameters;

    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\nPostProcess=0\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"#libriary "DX9RENDER_SCRIPT_LIBRIARY"
        native int SetGlowParams(float fBlurBrushSize, int intensity, int blurPasses);
        #event_handler("new_glow","UpdateGlow");
        void Main() { Trace("glow="+SetGlowParams(1.0,50,2)); }
        void UpdateGlow() { Trace("changed="+SetGlowParams(3,350,0.5)); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    assert_eq!(engine.host.glow_parameters(), GlowParameters::default());
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:glow=0"]);
    assert_eq!(
        engine.host.glow_parameters(),
        GlowParameters {
            blur_size: 0.0,
            intensity: 50,
            blur_passes: 0
        }
    );
    engine.emit_event("new_glow", &mut Message::new("", vec![])?)?;
    assert_eq!(trace.take(), ["script:changed=0"]);
    assert_eq!(
        engine.host.glow_parameters(),
        GlowParameters {
            blur_size: 0.5,
            intensity: 350,
            blur_passes: 3
        }
    );
    Ok(())
}

#[test]
fn time_scale_changes_the_next_script_frame_and_preserves_input_and_fixed_timing() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        native int AddControlTreeNode(int parent, string base, string output, float timeout);
        object journal;
        #event_handler("frame","Tick");
        #event_handler("after","After");
        #event_handler("resume","Resume");
        #event_handler("Control Activation","Pressed");
        void Tick() { Trace("frame="+GetDeltaTime()); }
        void After() { Trace("after="+GetDeltaTime()); }
        void Resume() { SendMessage(&journal,"lsf",45029,"SetTimeScale",2.5); }
        void Pressed() { Trace("pressed="+GetEventData()); }
        void Main() {
            CreateEntity(&journal,"ILogAndActions");
            MapControl(CreateControl("Prefix"),80);
            AddControlTreeNode(-1,"Prefix","Solo",0.05);
            PostEvent("after",125);
            SetTimeScale(0);
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:frame=16"]);
    engine.control_input(ControlInput::Button {
        code: 80,
        pressed: true,
    });
    engine.frame(50)?;
    assert_eq!(trace.take(), ["script:frame=0", "script:pressed=Prefix"]);
    engine.control_input(ControlInput::Button {
        code: 80,
        pressed: false,
    });
    engine.frame(50)?;
    assert_eq!(trace.take(), ["script:frame=0", "script:pressed=Solo"]);

    engine.emit_event("resume", &mut Message::new("", vec![])?)?;
    engine.frame(21)?;
    assert_eq!(trace.take(), ["script:frame=52"]);
    let mut timer = clock::Timer::default();
    timer.set_delta(17);
    timer.advance(std::time::Duration::from_millis(30))?;
    engine.frame_with_timer(&timer)?;
    assert_eq!(trace.take(), ["script:frame=17"]);
    assert_eq!(engine.frame(16)?.posted_events, 1);
    assert_eq!(trace.take(), ["script:after=40", "script:frame=40"]);
    Ok(())
}

#[test]
fn configured_battle_timer_expires_in_realize_and_can_restart_or_cancel() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object timer;
        int timeouts;
        #event_handler("expired","Expired");
        #event_handler("cancel_timer","Cancel");
        void Expired() {
            timeouts++;
            Trace("expired="+timeouts);
            SendMessage(&timer,"lfs",0,0.125,"expired");
        }
        void Cancel() { SendMessage(&timer,"l",1); }
        void Main() {
            timer.timerpos="10,20,110,50";
            timer.timeroffset="2,3,4,5";
            timer.timerforeuv="0,0.5,1,1";
            timer.timerforecolor=255;
            CreateEntity(&timer,"BITimer");
            LayerAddObject(1,&timer,0);
            SendMessage(&timer,"lfs",0,0.25,"expired");
        }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    let id = engine.host.entities.first_id("BITimer").unwrap();
    engine.frame(125)?;
    {
        let timer = engine.host.battle_timer_state(id).unwrap();
        assert_eq!(timer.background.rect, [10, 20, 110, 50]);
        assert_eq!(timer.foreground.rect, [12, 23, 106, 45]);
        assert_eq!(timer.foreground.uv, [0.0, 0.5, 1.0, 1.0]);
        assert_eq!(timer.foreground.color, 255);
        assert_eq!(timer.remaining_fraction(), 0.5);
    }
    assert!(trace.take().is_empty());
    engine.frame(125)?;
    assert_eq!(trace.take(), ["script:expired=1"]);
    assert_eq!(
        engine
            .host
            .battle_timer_state(id)
            .unwrap()
            .remaining_seconds,
        0.125
    );
    engine.frame(125)?;
    assert_eq!(trace.take(), ["script:expired=2"]);
    engine.emit_event("cancel_timer", &mut Message::new("", vec![])?)?;
    engine.frame(2000)?;
    assert!(trace.take().is_empty());
    assert_eq!(
        engine
            .host
            .battle_timer_state(id)
            .unwrap()
            .remaining_fraction(),
        0.0
    );
    engine.shutdown()?;
    assert!(engine.host.battle_timer_state(id).is_none());
    Ok(())
}

#[test]
fn configured_battle_log_receives_viewport_messages_and_runs_quick_actions() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(
        temp.0.join("engine.ini"),
        "run=main.c\nscreen_x=640\nscreen_y=480\n",
    )?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object Render, journal;
        #event_handler("SetWindowSize","WindowSize");
        #event_handler("BI_FastCommand","FastAction");
        #event_handler("test_visibility","Visibility");
        #event_handler("test_prune","Prune");
        #event_handler("test_clear","Clear");
        void WindowSize() {
            int width = GetEventData();
            int height = GetEventData();
            int television = GetEventData();
            Trace(width+","+height+","+television+","+IsEntity(&journal));
        }
        void FastAction() { Trace("action="+GetEventData()); }
        void Visibility() { SendMessage(&journal,"ll",45027,GetEventData()); }
        void Prune() { SendMessage(&journal,"lls",45020,1,""); }
        void Clear() { SendMessage(&journal,"l",45028); }
        void Main() {
            Trace(Render.screen_x+","+Render.screen_y+","+Render.full_screen);
            int control = CreateControl("BIFastCommand");
            MapControl(control,114);
            CreateEntity(&journal,"ILogAndActions");
            journal.Log.width=300;
            journal.Log.height=65;
            journal.Log.right=224;
            journal.Log.offsetString=20;
            journal.Log.speed=0.5;
            journal.Log.color_speed=0.5;
            journal.ActiveActions.horzQ=4;
            journal.ActiveActions.vertQ=2;
            journal.ActiveActions.width=48;
            journal.ActiveActions.top=17;
            journal.ActiveActions.Action.IconNum=5;
            journal.ActiveActions.text1.text="F3";
            SendMessage(&journal,"lll",45022,1,1);
            SendMessage(&journal,"ls",45021,"Action");
            SendMessage(&journal,"lls",45020,0,"first");
            SendMessage(&journal,"lls",45020,0,"FIRST");
            SendMessage(&journal,"lls",45020,1,"old");
            SendMessage(&journal,"lls",45020,1,"pinned");
            SendMessage(&journal,"lls",45020,0,"second");
            LayerAddObject(0,&journal,0);
        }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:1280,720,0", "script:1280,720,0,0"]);
    let id = engine.host.entities.first_id("ILogAndActions").unwrap();
    {
        let state = engine.host.log_and_actions_state(id).unwrap();
        assert_eq!(state.viewport, [1280, 720]);
        assert!(state.visible && state.show_actions && state.show_log);
        assert_eq!(state.log.as_ref().unwrap().right, 224);
        let action = state.action.as_ref().unwrap();
        assert_eq!(action.rect, [0, 17, 48, 64]);
        assert_eq!(action.uv, Some([0.25, 0.5, 0.5, 1.0]));
        assert_eq!(action.hints[0].as_ref().unwrap().text, "F3");
        assert_eq!(
            state
                .lines
                .iter()
                .map(|line| (line.text.as_str(), line.offset))
                .collect::<Vec<_>>(),
            [("first", 0.0), ("pinned", 20.0), ("second", 40.0)]
        );
    }
    engine.control_input(ControlInput::Button {
        code: 114,
        pressed: true,
    });
    engine.frame(10)?;
    assert!(trace.take().is_empty());
    engine.frame(10)?;
    assert_eq!(trace.take(), ["script:action=Action"]);
    let visible = |value| Message::new("l", vec![storm_core::Value::Long(value)]);
    engine.emit_event("test_visibility", &mut visible(0)?)?;
    engine.frame(200)?;
    {
        let state = engine.host.log_and_actions_state(id).unwrap();
        assert_eq!(
            state
                .lines
                .iter()
                .map(|line| line.alpha)
                .collect::<Vec<_>>(),
            [245.0, 10000.0, 245.0]
        );
    }
    engine.emit_event("test_visibility", &mut visible(1)?)?;
    engine.frame(500)?;
    assert_eq!(
        engine.host.log_and_actions_state(id).unwrap().lines[0].text,
        "pinned"
    );
    engine.emit_event("test_prune", &mut Message::new("", vec![])?)?;
    engine.emit_event("test_clear", &mut Message::new("", vec![])?)?;
    assert!(
        engine
            .host
            .log_and_actions_state(id)
            .unwrap()
            .lines
            .is_empty()
    );
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_xinterface_uses_the_live_viewport_and_synchronous_script_callbacks() -> Result<()> {
    let temp = Temporary::new()?;
    let game_dir = &temp.0;
    let ini = game_dir.join("resource/ini/interfaces");
    let textures = game_dir.join("resource/textures/interfaces");
    std::fs::create_dir_all(&ini)?;
    std::fs::create_dir_all(&textures)?;
    std::fs::write(
        ini.join("interfaces.ini"),
        "[COMMON]\nMousePointer=Interfaces\\cursor.tga,10,size:(32,24),pos:(-16,-12)\nRepeatDelay=250\n\
         [PC_SCREEN_1920_1080]\nbDynamicScaling=1\n[PC_SCREEN]\nwScreenWidth=800\nwScreenHeight=600\n",
    )?;
    let mut tx = [0u32, 1, 1, 1, 21, 4]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    tx.extend([0, 0, 255, 255]);
    std::fs::write(textures.join("cursor.tga.tx"), tx)?;
    std::fs::write(
        game_dir.join("engine.ini"),
        "run=main.c\nscreen_x=800\nscreen_y=600\n",
    )?;
    std::fs::write(
        game_dir.join("quest.txt"),
        "#QUEST Q1 Заголовок\r\n#TEXT 1 Запись\r\n#TEXT 1 duplicate",
    )?;
    std::fs::write(
        game_dir.join("extra.txt"),
        "#QUEST Q1 duplicate\n#TEXT 2 Вторая запись",
    )?;
    std::fs::write(
        game_dir.join("main.c"),
        r#"
        object ui;
        string files[3];
        int wheel;
        #event_handler("evGetMouseWeel", "GetWheel");
        #event_handler("test_scroll", "SetWheel");
        ref QuestFiles() {
            Trace("init-bound=" + IsEntity(&ui));
            files[0] = "quest.txt";
            files[1] = "extra.txt";
            files[2] = "quest.txt";
            return &files;
        }
        ref GetWheel() { Trace("wheel-init"); return &wheel; }
        void SetWheel() { wheel=GetEventData(); }
        void Main() {
            ui.GameTime.sec=45;
            ui.GameTime.min=12;
            ui.GameTime.hour=7;
            SetEventHandler("GetQuestTextFileName", "QuestFiles", 0);
            CreateEntity(&ui,"xinterface");
            DelEventHandler("GetQuestTextFileName", "QuestFiles");
            Trace("created=" + IsEntity(&ui));
            wheel=2;
        }
        "#,
    )?;
    let game = GameDirectory::open(game_dir)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(1600, 900);
    engine.frame(16)?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:init-bound=0",
            "script:wheel-init",
            "script:created=1"
        ]
    );
    let id = engine.host.entities.first_id("xinterface").unwrap();
    {
        let state = engine.host.interface_state(id).unwrap();
        assert_eq!(state.screen.section, "PC_SCREEN_1920_1080");
        assert_eq!(state.screen.size, [1066, 600]);
        assert_eq!(state.screen.rect, [133, 0, 933, 600]);
        assert_eq!(state.screen.repeat_delay, 250);
        assert_eq!(state.game_time, [45, 12, 7]);
        let cursor = state.cursor.as_ref().unwrap();
        assert_eq!((cursor.size, cursor.offset), ([32, 24], [-16, -12]));
        assert_eq!(
            cursor.texture.as_ref().unwrap().mips[0].rgba,
            [255, 0, 0, 255]
        );
        assert_eq!(state.quest_title("Q1"), Some("Заголовок"));
        assert_eq!(state.quest_text("Q1", "1"), Some("Запись"));
        assert_eq!(state.quest_text("Q1", "2"), Some("Вторая запись"));
        assert_eq!(state.wheel_steps()?, 2);
    }
    engine.emit_event(
        "test_scroll",
        &mut Message::new("l", vec![storm_core::Value::Long(-3)])?,
    )?;
    assert_eq!(engine.host.interface_state(id).unwrap().wheel_steps()?, -3);
    engine.shutdown()?;
    assert!(engine.host.interface_state(id).is_none());
    Ok(())
}

#[test]
fn a_failed_interface_script_callback_does_not_leave_a_live_entity() -> Result<()> {
    let temp = Temporary::new()?;
    let ini = temp.0.join("resource/ini/interfaces");
    std::fs::create_dir_all(&ini)?;
    std::fs::write(ini.join("interfaces.ini"), "[PC_SCREEN]\n")?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("GetQuestTextFileName", "Files");
        string Files() { return "../outside.txt"; }
        void Main() { CreateEntity(&ui,"xinterface"); }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    let error = engine.frame(0).expect_err("invalid quest path");
    assert!(format!("{error:#}").contains("invalid game path component"));
    assert!(engine.host.entities.first_id("xinterface").is_none());
    engine.shutdown()?;
    assert!(engine.host.entities.all_ids().is_empty());
    Ok(())
}

#[test]
fn rand_and_frnd_share_the_original_windows_stream() -> Result<()> {
    use storm_script::{Limits, Runtime, Value};
    let mut runtime = Runtime::compile(
        "int Draw(int limit) { return Rand(limit); } float Fraction() { return frnd(); }",
    )?;
    let mut host = NativeHost::with_random_seed(1)?;
    let limits = Limits::default();
    assert_eq!(
        runtime.call("Draw", &[Value::Int(0)], &mut host, limits)?,
        Value::Int(0)
    );
    assert_eq!(
        runtime.call("Fraction", &[], &mut host, limits)?,
        Value::Float(18467.0 / 32767.0),
    );
    for (limit, expected) in [(-10, -9), (i32::MAX, 26500), (i32::MIN, -19169), (100, 69)] {
        assert_eq!(
            runtime.call("Draw", &[Value::Int(limit)], &mut host, limits)?,
            Value::Int(expected),
        );
    }
    // Fixed-seed output published in the Microsoft Learn rand example.
    let mut host = NativeHost::with_random_seed(1792)?;
    for expected in [
        5890, 1279, 19497, 1207, 11420, 3377, 15317, 29489, 9716, 23323,
    ] {
        assert_eq!(
            runtime.call("Draw", &[Value::Int(32767)], &mut host, limits)?,
            Value::Int(expected),
        );
    }
    Ok(())
}

#[test]
fn configured_user_language_files_reuse_ids_and_close_the_last_reference() -> Result<()> {
    let temp = Temporary::new()?;
    let texts = temp.0.join("RESOURCE/INI/TEXTS");
    std::fs::create_dir_all(texts.join("Russian"))?;
    std::fs::create_dir_all(texts.join("English"))?;
    std::fs::write(
        texts.join("language.ini"),
        "[COMMON]\ndefaultLanguage=Russian\nstrings=common.ini\n[DIRECTORY]\nRussian=Russian\nEnglish=English\n",
    )?;
    std::fs::write(texts.join("Russian/common.ini"), "string=Port,\"Порт\"\n")?;
    std::fs::write(
        texts.join("English/common.ini"),
        "string=Port,\"Harbour\"\n",
    )?;
    std::fs::write(texts.join("English/Places.TXT"), "Port {Barbados}\n")?;
    let (legacy, _, replaced) = encoding_rs::WINDOWS_1251.encode(
        "\r\nPort  {  Барбадос  }\r\nPORT {дубликат}\r\nHelp\r\n{\r\nпервая\r\nвторая\r\n}\r\nEmpty {}\r\nPrevious label\r\nIgnored label {строка}\r\n",
    );
    assert!(!replaced);
    std::fs::write(texts.join("Russian/Places.TXT"), legacy)?;
    std::fs::write(texts.join("Russian/other.txt"), "Quest {Привет}\n")?;
    std::fs::write(texts.join("Russian/empty.txt"), "")?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "script_interface_functions"
        native int LanguageOpenFile(string file);
        native void LanguageCloseFile(int id);
        native string LanguageConvertString(int id, string key);
        native string LanguageGetLanguage();
        native void LanguageSetLanguage(string name);
        native string XI_ConvertString(string id);
        object language;
        void Main() {
            CreateEntity(&language, "obj_strservice");
            int first = LanguageOpenFile("places.txt");
            int second = sti(SendMessage(&language,"ls",45100,"PLACES.TXT"));
            int other = LanguageOpenFile("other.txt");
            Trace("ids=" + first + "," + second + "," + other);
            string key = "PoRt";
            string translated, selected;
            SendMessage(&language,"llee",45102,first,&key,&translated);
            SendMessage(&language,"le",45103,&selected);
            Trace(selected + "=" + translated);
            Trace(LanguageConvertString(first, "PoRt"));
            Trace(LanguageConvertString(first, "help"));
            Trace("[" + LanguageConvertString(first, "Empty") + LanguageConvertString(other, "missing") + "]");
            Trace(LanguageConvertString(first, "Previous label") + "[" + LanguageConvertString(first, "Ignored label") + "]");
            LanguageCloseFile("places.txt");
            LanguageCloseFile(first);
            Trace(LanguageConvertString(second, "port"));
            SendMessage(&language,"ll",45101,second);
            Trace("[" + LanguageConvertString(first, "port") + "]");
            Trace("failed=" + LanguageOpenFile("empty.txt") + "," + LanguageOpenFile("missing.txt"));
            Trace("reopened=" + LanguageOpenFile("places.txt"));
            Trace(LanguageConvertString(other, "quest"));
            LanguageSetLanguage("russian");
            Trace(LanguageGetLanguage());
            LanguageSetLanguage("English");
            Trace(LanguageGetLanguage() + ":" + XI_ConvertString("Port") + ":" + LanguageConvertString(first, "Port"));
            Trace("[" + LanguageConvertString(other, "quest") + "]");
            LanguageSetLanguage("Unknown");
            Trace(LanguageGetLanguage() + ":" + XI_ConvertString("Port") + ":" + LanguageConvertString(first, "Port"));
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:ids=0,0,1",
            "script:Russian=Барбадос",
            "script:Барбадос",
            "script:первая\r\nвторая",
            "script:[]",
            "script:строка[]",
            "script:Барбадос",
            "script:[]",
            "script:failed=-1,-1",
            "script:reopened=0",
            "script:Привет",
            "script:Russian",
            "script:English:Harbour:Barbados",
            "script:[]",
            "script:Russian:Порт:Барбадос",
        ],
    );
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_startup_allows_longer_main_without_widening_frame_fuel() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #event_handler("frame", "Tick");
        void Busy() {
            int i, sum;
            for (i=0; i<20000; i++) sum += i;
        }
        void Main() { Busy(); Trace("main done"); }
        void Tick() { Busy(); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    let error = engine.frame(0).expect_err("normal frame instruction limit");
    let error = format!("{error:#}");
    assert!(
        error.contains("event frame, handler tick")
            && error.contains("script instruction budget exhausted")
    );
    assert_eq!(*trace.borrow(), ["script:main done"]);
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_island_locator_populates_original_groups_through_a_script_reference() -> Result<()> {
    let temp = Temporary::new()?;
    let models = temp.0.join("RESOURCE/Models/Islands/Barbados");
    std::fs::create_dir_all(&models)?;
    let labels = [
        ("Reload", "reload_1", [12.5, -3.0, 4.0]),
        ("reload_1", "berth_a", [8.0, 0.0, 5.0]),
        ("reload_1", "berth_b", [9.0, 0.0, 6.0]),
        ("Quest_Ships", "quest_1", [1.0, 2.0, 3.0]),
        ("net_team", "team_1", [13.0, 0.0, 0.0]),
        ("fire", "fire_1", [0.0, 7.0, 0.0]),
        ("IslandShips1", "spawn_a", [0.0, 0.0, 22.0]),
    ];
    let mut names = Vec::new();
    let mut name_offsets = std::collections::BTreeMap::new();
    let mut records = Vec::new();
    for (group, name, position) in labels {
        let mut record = [0u32; 27];
        for (index, name) in [group, name].into_iter().enumerate() {
            record[index] = *name_offsets.entry(name).or_insert_with(|| {
                let offset = names.len() as u32;
                names.extend_from_slice(name.as_bytes());
                names.push(0);
                offset
            });
        }
        // Unit Y and forward +X yield a positive pi/2 heading.
        let matrix: [f32; 16] = [
            0.0,
            0.0,
            -1.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            position[0],
            position[1],
            position[2],
            1.0,
        ];
        for (word, value) in record[3..19].iter_mut().zip(matrix) {
            *word = value.to_bits();
        }
        records.extend(record.into_iter().flat_map(u32::to_le_bytes));
    }
    let mut words = [0u32; 18];
    words[0] = u32::from_le_bytes(*b"50.1");
    words[2] = names.len() as u32;
    words[3] = name_offsets.len() as u32;
    words[7] = labels.len() as u32;
    let mut gm = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    gm.extend(names);
    gm.extend(name_offsets.into_values().flat_map(u32::to_le_bytes));
    gm.extend(records);
    std::fs::write(models.join("Barbados_Locators.gm"), gm)?;

    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object islands[2], locator;
        void Main() {
            islands[0].filespath.models = "islands\barbados";
            islands[0].locators = "barbados_locators";
            islands[0].reload.port.name = "RELOAD_1";
            islands[0].reload.missing.name = "unmatched";
            islands[0].Quest_ships.description = "keep";
            islands[0].LoadGroup.g1 = "IslandShips1";
            islands[1].locators = "absent";
            CreateEntity(&locator, "locator");
            SendMessage(&locator, "le", 1504, &islands[0]);
            SendMessage(&locator, "le", 1504, &islands[1]);
            Trace(islands[0].reload.port.x + "," + islands[0].reload.port.y + "," + islands[0].reload.port.ay);
            Trace(islands[0].reload.port.ships.l0.x + "," + islands[0].reload.port.ships.l1.z);
            Trace("missing=" + CheckAttribute(&islands[0], "reload.missing.x"));
            Trace(islands[0].Quest_ships.description + "," + islands[0].Quest_ships.quest_1.z);
            Trace(islands[0].net_team.team_1.x + "," + islands[0].IslandShips1.spawn_a.z);
            Trace(islands[0].fire.fire_1.y + "," + CheckAttribute(&islands[0], "fire.fire_1.ay"));
            Trace("unloaded=" + CheckAttribute(&islands[1], "reload"));
            DeleteClass(&locator);
        }"#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:12.5,-3,1.5708",
            "script:8,6",
            "script:missing=0",
            "script:keep,3",
            "script:13,22",
            "script:7,0",
            "script:unloaded=0",
        ],
    );
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_controls_dispatch_ordered_desktop_edges_and_tree_commands() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        native int AddControlTreeNode(int parent, string base, string output, float timeout);
        #event_handler("Control Activation","Pressed");
        #event_handler("Control Deactivation","Released");
        #event_handler("evMouseWeel","WheelMoved");
        #event_handler("toggle","ToggleLock");
        int locked;
        void Pressed() { Trace("+" + GetEventData()); }
        void Released() { Trace("-" + GetEventData()); }
        void WheelMoved() { Trace("wheel=" + GetEventData()); }
        void ToggleLock() {
            locked = 1 - locked;
            LockControl("ICancel", locked);
        }
        int Bind(string name, int system) {
            int id = CreateControl(name);
            MapControl(id, system);
            return id;
        }
        void Main() {
            int cancel = Bind("ICancel", 27);
            Trace("duplicate=" + (cancel == CreateControl("icancel")));
            Bind("IAction", 32);
            Bind("Click", 1);
            Bind("Wheel", 258);
            int axis = Bind("Motion", 256);
            SetControlFlags(axis, 1);
            SetControlTreshold(axis, 0.1);
            Bind("Shift",16);
            Bind("Prefix",80);
            Bind("Chord",87);
            int parent = AddControlTreeNode(-1,"Prefix","Solo",0.05);
            AddControlTreeNode(parent,"Chord","Combo",0.0);
            MapControl(-1,27);
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    let button = |code, pressed| ControlInput::Button { code, pressed };
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:duplicate=1"]);

    for input in [
        button(32, true),
        button(27, true),
        button(1, true),
        ControlInput::MouseMotion { x: 2.5, y: -3.0 },
        button(161, true),
    ] {
        engine.control_input(input);
    }
    engine.mouse_wheel(1)?;
    assert_eq!(trace.take(), ["script:wheel=1"]);
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        [
            "script:+ICancel",
            "script:+IAction",
            "script:+Click",
            "script:+Wheel",
            "script:+Motion",
            "script:+Shift"
        ],
    );
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:-Wheel", "script:-Motion"]);
    engine.emit_event("toggle", &mut Message::new("", vec![])?)?;
    engine.control_input(ControlInput::FocusLost);
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        ["script:-IAction", "script:-Click", "script:-Shift"]
    );

    engine.control_input(button(80, true));
    engine.control_input(button(87, true));
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        ["script:+Prefix", "script:+Chord", "script:+Combo"]
    );
    engine.control_input(button(80, false));
    engine.control_input(button(87, false));
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        ["script:-Prefix", "script:-Chord", "script:-Combo"]
    );

    engine.control_input(button(80, true));
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:+Prefix"]);
    engine.control_input(button(80, false));
    engine.frame(50)?;
    assert_eq!(trace.take(), ["script:-Prefix", "script:+Solo"]);
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:-Solo"]);
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_segments_link_globals_and_externs_and_unload_at_frame_boundaries() -> Result<()> {
    let temp = Temporary::new()?;
    let program = temp.0.join("PROGRAM");
    std::fs::create_dir_all(program.join("Controls"))?;
    std::fs::write(
        temp.0.join("engine.ini"),
        "program_directory=program\nrun=main.c\n",
    )?;
    std::fs::write(
        program.join("main.c"),
        r#"
        #define INITIAL_COUNT 7
        #define ON_PING "ping"
        #event_handler("reload", "Reload");
        #event_handler("probe-unloaded", "ProbeUnloaded");
        #event_handler(ON_PING, "BaseHook");
        int total = 10;
        extern void ExternControlsInit();
        extern void StopControls();
        void BaseHook() { Trace("base="+total); }
        void RecordControl(int count) { total += count; }
        void Main() {
            string file = "controls\init_pc.c";
            Trace("main="+SegmentIsLoaded("main.c"));
            Trace("before="+SegmentIsLoaded(file));
            Trace("nil="+LoadSegment("missing.c"));
            Trace("invalid="+LoadSegment("../outside.c"));
            Trace("conflict="+LoadSegment("broken.c"));
            Trace("rejected="+CheckFunction("Rejected"));
            Trace("loaded="+LoadSegment(file));
            Trace("repeat="+LoadSegment(file));
            ExternControlsInit();
            ExternControlsInit();
            Event(ON_PING);
            Event("dynamic");
            StopControls();
            Trace("pending="+SegmentIsLoaded(file));
            Trace("queued="+LoadSegment(file));
            Trace("remembered="+CheckFunction("ExternControlsInit"));
            Event(ON_PING);
            Event("dynamic");
        }
        void Reload() {
            string file = "controls\init_pc.c";
            Trace("reaped="+SegmentIsLoaded(file));
            Trace("reload="+LoadSegment(file));
            ExternControlsInit();
            Event(ON_PING);
            StopControls();
        }
        void ProbeUnloaded() { ExternControlsInit(); }
        "#,
    )?;
    std::fs::write(
        program.join("Controls/Init_PC.c"),
        r#"
        #include "CONTROLS\helper.c"
        #libriary "script_libriary_test"
        #event_handler(ON_PING, "ModuleHook");
        native float Clampf(float value);
        int counter[INITIAL_COUNT];
        void ExternControlsInit() {
            counter[0] += StartCounter();
            if (Clampf(2.0) == 1.0) RecordControl(counter[0]);
            SetEventHandler("dynamic","ModuleHook",0);
            Trace("run="+counter[0]);
        }
        void ModuleHook() { Trace("module="+counter[0]); }
        void StopControls() {
            UnloadSegment("controls\init_pc.c");
            Trace("return from unloaded function");
        }
        "#,
    )?;
    std::fs::write(
        program.join("Controls/helper.c"),
        "int StartCounter() { return INITIAL_COUNT; }",
    )?;
    std::fs::write(
        program.join("broken.c"),
        "void Rejected() {} void BaseHook() {}",
    )?;
    std::fs::write(temp.0.join("outside.c"), "void Rejected() {}")?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    engine.emit_event("reload", &mut Message::new("", vec![])?)?;
    let error = engine
        .emit_event("probe-unloaded", &mut Message::new("", vec![])?)
        .expect_err("unloaded extern must be unavailable immediately");
    assert!(
        format!("{error:#}").contains("extern script function externcontrolsinit is not loaded")
    );
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:main=1",
            "script:before=0",
            "script:nil=0",
            "script:invalid=0",
            "script:conflict=0",
            "script:rejected=0",
            "script:loaded=1",
            "script:repeat=1",
            "script:run=7",
            "script:run=14",
            "script:base=31",
            "script:module=14",
            "script:module=14",
            "script:return from unloaded function",
            "script:pending=1",
            "script:queued=1",
            "script:remembered=1",
            "script:base=31",
            "script:reaped=0",
            "script:reload=1",
            "script:run=7",
            "script:base=38",
            "script:module=7",
            "script:return from unloaded function",
        ],
    );
    Ok(())
}

#[test]
fn configured_rigging_formats_attributes_and_uses_original_sail_power() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r##"#libriary "script_rigging_files"
        native string GetAssembledString(string template, aref data);
        native float funcGetSailSpeed(int holes, int maximum, float power);
        native int ShipSailState(int character);
        object data;
        void Main() {
            aref view, unbound;
            data.ship.name = "Бриг";
            data.ship.count = "  -31 shots";
            data.ship.power = " +1.256 knots";
            data.ship.bad = "invalid";
            data.ship.decimal = "-0.0";
            makearef(view, data.ship);
            Trace(GetAssembledString("[#sSHIP.name#|#dship.count#|#fship.power#]", &data));
            Trace(GetAssembledString("#f.2power#/#f.0power#/#dmissing#/#ubad#/#fbad#/#f.1decimal", view));
            Trace(GetAssembledString("abc##xyz#sname", view));
            Trace("[" + GetAssembledString("literal", unbound) + "]");
            if (funcGetSailSpeed(1, 4, 100.0) == 25.0 &&
                funcGetSailSpeed(0, 4, 100.0) == 0.0 &&
                funcGetSailSpeed(5, 4, 100.0) == 100.0 &&
                funcGetSailSpeed(1, 0, 100.0) == 100.0) {
                Trace("sail power loss");
            }
            Trace("absent sail=" + ShipSailState(7));
        }"##,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:[Бриг|-31|1.256000]",
            "script:1.26/1///0.000000/-0.0",
            "script:abcxyzБриг",
            "script:[]",
            "script:sail power loss",
            "script:absent sail=0",
        ],
    );
    Ok(())
}

#[test]
fn configured_interface_reads_original_common_strings_and_localized_text() -> Result<()> {
    let temp = Temporary::new()?;
    let texts = temp.0.join("resource/ini/texts");
    std::fs::create_dir_all(texts.join("russian"))?;
    std::fs::write(
        texts.join("language.ini"),
        "[Common]\n; defaultLanguage=English\ndefaultLanguage = Russian\nstrings=common.ini\n[Directory]\nRUSSIAN=russian\n",
    )?;
    let (common, _, replaced) = encoding_rs::WINDOWS_1251.encode(
        "string = PortTitle, \"  Порт  \"\r\nstring = PORTTITLE,\"duplicate\"\r\nstring = Empty, ignored\r\n[other]\r\nstring = Other,\"must stay hidden\"\r\n",
    );
    assert!(!replaced);
    std::fs::write(texts.join("russian/common.ini"), common)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"#libriary "script_interface_functions"
        native string LanguageGetLanguage();
        native string XI_ConvertString(string key);
        void Main() {
            Trace(LanguageGetLanguage());
            Trace("["+XI_ConvertString("porttitle")+"]");
            Trace("["+XI_ConvertString("Empty")+XI_ConvertString("Other")+"]");
        }"#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        ["script:Russian", "script:[  Порт  ]", "script:[]"],
    );
    Ok(())
}

#[test]
fn configured_location_library_uses_live_arrays_and_entity_attributes() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"#libriary "ScriptLocationLibrary"
        native int NativeFindCharacter(ref items, string id);
        native int NativeFindLocation(ref items, string id);
        native int NativeFindLoadCharacter();
        native string GetNextLineString();
        object characters[3], a, b;
        void Main() {
            characters[2].id = "Captain";
            Trace("initial="+NativeFindCharacter(&characters,"captain"));
            characters[0].id = "CAPTAIN";
            Trace("cached="+NativeFindCharacter(&characters,"Captain"));
            Trace("other-cache="+NativeFindLocation(&characters,"captain"));
            SetArraySize(&characters,2);
            Trace("resized="+NativeFindCharacter(&characters,"captain"));
            characters[0].id = "departed";
            Trace("missing="+NativeFindCharacter(&characters,"captain"));
            Trace("empty="+NativeFindLocation(&characters,""));
            Trace("absent="+NativeFindLoadCharacter());
            CreateEntity(&a,"location");
            CreateEntity(&b,"LOCATION");
            Trace("unset="+NativeFindLoadCharacter());
            a.index=" +31port";
            b.index=42;
            Trace("first="+NativeFindLoadCharacter());
            DeleteClass(&a);
            Trace("remaining="+NativeFindLoadCharacter());
            Trace("line"+GetNextLineString()+"end");
        }"#,
    )?;
    struct Location;
    impl Entity for Location {}
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut host = host(&trace)?;
    host.entities
        .register_class("location", || Ok(Box::new(Location)))?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:initial=2",
            "script:cached=2",
            "script:other-cache=0",
            "script:resized=0",
            "script:missing=-1",
            "script:empty=-1",
            "script:absent=-1",
            "script:unset=-1",
            "script:first=31",
            "script:remaining=42",
            "script:line\r\nend",
        ],
    );
    Ok(())
}

#[test]
fn location_bindings_select_ambient_presets_and_wait_in_real_milliseconds() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"#libriary "ScriptLocationLibrary"
        native void ExecuteTechnique(string preset);
        native void SystemDelay(int milliseconds);
        void Main() { ExecuteTechnique("AMB_DUNGEON"); }"#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    assert_eq!(engine.host.ambient_light.get(), None);
    engine.frame(16)?;
    assert_eq!(engine.host.ambient_light.get(), Some([0x20, 0x20, 0x30]));
    for (name, expected) in [
        ("inca_temple", [0x30, 0x30, 0x2c]),
        ("amb_residence", [0x85, 0x85, 0x80]),
        ("amb_inca_temple", [0x30, 0x30, 0x2c]),
        ("", [0x30, 0x30, 0x2c]),
        ("unknown_effect", [0x30, 0x30, 0x2c]),
    ] {
        assert_eq!(
            engine
                .host
                .call("executetechnique", &[Value::String(name.into())])?,
            Value::Void,
        );
        assert_eq!(engine.host.ambient_light.get(), Some(expected));
    }
    for (requested, minimum) in [(-5, 1), (0, 1), (7, 7)] {
        let start = std::time::Instant::now();
        assert_eq!(
            engine.host.call("systemdelay", &[Value::Int(requested)])?,
            Value::Void
        );
        assert!(start.elapsed() >= std::time::Duration::from_millis(minimum));
        assert_eq!(engine.host.delta_ms, 16);
    }
    assert!(
        engine
            .host
            .call("systemdelay", &[Value::Float(1.0)])
            .is_err()
    );
    assert!(
        engine
            .host
            .call("executetechnique", &[Value::Int(1)])
            .is_err()
    );
    engine.shutdown()?;
    Ok(())
}

#[test]
fn configured_game_uses_the_original_offline_steam_results() -> Result<()> {
    use storm_script::{Host, Value};
    let temp = Temporary::new()?;
    let game = GameDirectory::open(&temp.0)?;
    let mut host = NativeHost::new()?;
    host.register_game_library("SteamApiScriptLib", &game)?;
    host.load_library("SteamApiScriptLib")?;
    let id = [Value::Int(123)];
    let name = [Value::String("sample".into())];
    let stat = [name[0].clone(), Value::Int(3)];
    for (function, args, expected) in [
        ("getsteamenabled", &[][..], 0),
        ("getdlcenabled", &id[..], 1),
        ("getdlccount", &[], 0),
        ("getdlcdata", &id, 0),
        ("dlcstartoverlay", &id, 0),
        ("setachievement", &name, 0),
        ("getachievement", &name, 0),
        ("setstat", &stat, 0),
        ("getstat", &name, 0),
        ("storestats", &[], 0),
        ("clearachievement", &name, 0),
        ("resetstats", &id, 0),
    ] {
        assert_eq!(
            host.call(function, args)?,
            Value::Int(expected),
            "{function}"
        );
    }
    assert!(host.call("getdlcenabled", &name).is_err());
    Ok(())
}

#[test]
fn configured_game_initializes_the_original_math_library_and_its_counter() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"#libriary "SCRIPT_LIBRIARY_TEST"
        native float Bring2Range(float a,float b,float c,float d,float x);
        native float Bring2RangeNoCheck(float a,float b,float c,float d,float x);
        native float Clampf(float x);
        native float Degree2Radian(float degrees);
        native int RDTSC_B();
        native int RDTSC_E(int start);
        void Main() {
            int start=RDTSC_B();
            Trace("clamp="+MakeInt(Clampf(2.0)));
            Trace("range="+MakeInt(Bring2Range(10.0,20.0,8.0,2.0,5.0)));
            Trace("unbounded="+MakeInt(Bring2RangeNoCheck(10.0,20.0,0.0,4.0,10.0)));
            Trace("radians="+MakeInt(Degree2Radian(180.0)*1000.0));
            int elapsed=RDTSC_E(start);
            Trace("timed");
        }"#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        [
            "script:clamp=1",
            "script:range=20",
            "script:unbounded=35",
            "script:radians=3141",
            "script:timed",
        ]
    );
    Ok(())
}

#[test]
fn a_library_binding_collision_leaves_the_existing_functions_intact() -> Result<()> {
    use storm_script::{Host, Value};
    let mut host = NativeHost::new()?;
    let temp = Temporary::new()?;
    let game = GameDirectory::open(&temp.0)?;
    host.register_function("Clampf", |_| Ok(Value::Float(0.5)))?;
    host.register_game_library("script_libriary_test", &game)?;
    let error = host.load_library("script_libriary_test").unwrap_err();
    assert!(format!("{error:#}").contains("duplicate native function: clampf"));
    assert!(host.call("bring2range", &[]).is_err());
    assert_eq!(host.call("clampf", &[])?, Value::Float(0.5));
    Ok(())
}

#[test]
fn configured_scripts_use_original_engine_headers_separately_from_game_program_files() -> Result<()>
{
    let temp = Temporary::new()?;
    std::fs::create_dir_all(temp.0.join("PROGRAM/storm-engine"))?;
    std::fs::write(
        temp.0.join("engine.ini"),
        "program_directory=PROGRAM\nrun=main.c\n",
    )?;
    std::fs::write(
        temp.0.join("PROGRAM/storm-engine/layers.h"),
        "#define SEA_EXECUTE 999\n",
    )?;
    std::fs::write(
        temp.0.join("PROGRAM/main.c"),
        "#include \"storm-engine\\layers.h\"\n#include \"storm-engine/events.h\"\nvoid Main(){Trace(\"layer=\"+SEA_EXECUTE);Trace(GET_CANNON_BY_TYPE_EVENT);}",
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    assert_eq!(
        engine.sources(),
        ["main.c", "storm-engine/layers.h", "storm-engine/events.h"]
    );
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(
        *trace.borrow(),
        ["script:layer=2", "script:GetCannonByTypeEvent"]
    );
    Ok(())
}

#[test]
fn all_bundled_shared_headers_are_verbatim_and_reserved_misses_never_read_game_shadows()
-> Result<()> {
    let temp = Temporary::new()?;
    std::fs::create_dir_all(temp.0.join("storm-engine"))?;
    std::fs::write(temp.0.join("storm-engine/unknown.h"), "#define SHADOW 7")?;
    let mut source = GameSource {
        directory: GameDirectory::open(&temp.0)?,
        encoding: ScriptEncoding::Windows1251,
    };
    let names = [
        "layers.h",
        "events.h",
        "controls.h",
        "sound.h",
        "messages.h",
        "mast_msg.h",
        "sail_msg.h",
        "bimanager/messages.h",
        "battle_interface/msg_control.h",
        "battle_interface/log_msg.h",
        "sea_ai/sea_people.h",
        "sea_ai/script_defines.h",
        "interface/messages.h",
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/libs/shared_headers/include/shared");
    for name in names {
        let actual = source.load(&format!("storm-engine/{name}"))?;
        assert_eq!(actual, std::fs::read_to_string(root.join(name))?);
    }
    assert!(source.load("storm-engine/unknown.h").is_err());
    assert!(source.load("storm-engine/../layers.h").is_err());
    assert!(source.load(&"a".repeat(1025)).is_err());
    assert_eq!(
        source.load("STORM-ENGINE\\LAYERS.H")?,
        source.load("storm-engine/layers.h")?
    );
    Ok(())
}

#[test]
fn registered_script_libraries_initialize_once_before_native_calls_and_register_real_bindings()
-> Result<()> {
    use std::{cell::Cell, rc::Rc};
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let mut host = NativeHost::new()?;
    host.register_library("shared_lib", move |host| {
        seen.set(seen.get() + 1);
        host.register_function("Answer", |_| Ok(storm_script::Value::Int(42)))
    })?;
    let mut runtime = storm_script::Runtime::compile(
        "#libriary \"SHARED_LIB\"\n#libriary \"shared_lib\"\nnative int Answer();int Main(){return Answer();}",
    )?;
    assert_eq!(count.get(), 0);
    for _ in 0..3 {
        assert_eq!(
            runtime.call("Main", &[], &mut host, storm_script::Limits::default())?,
            storm_script::Value::Int(42)
        );
    }
    let mut fresh = storm_script::Runtime::compile(
        "#libriary \"shared_lib\"\nnative int Answer();int Main(){return Answer();}",
    )?;
    assert_eq!(
        fresh.call("Main", &[], &mut host, storm_script::Limits::default())?,
        storm_script::Value::Int(42)
    );
    assert_eq!(count.get(), 1);
    assert!(host.register_library("SHARED_LIB", |_| Ok(())).is_err());
    Ok(())
}

#[test]
fn library_initialization_errors_fault_engine_and_reentrant_loading_is_checked() -> Result<()> {
    use std::{cell::Cell, rc::Rc};
    use storm_script::Host;
    let mut host = NativeHost::new()?;
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    host.register_library("recursive", move |host| {
        seen.set(seen.get() + 1);
        host.load_library("recursive")
    })?;
    let runtime = storm_script::Runtime::compile("#libriary \"recursive\"\nvoid Main(){}")?;
    let mut engine = Engine::from_runtime(runtime, host);
    let error = engine.frame(16).unwrap_err();
    assert!(format!("{error:#}").contains("initialization reentry"));
    assert_eq!(engine.phase(), Phase::Faulted);
    assert!(engine.frame(16).is_err());
    engine.shutdown()?;
    engine.shutdown()?;
    assert_eq!(count.get(), 1);
    assert_eq!(engine.phase(), Phase::Stopped);
    Ok(())
}
use anyhow::{Result, ensure};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use storm_core::{AttributeId, AttributeStore, Entity, EntityDestroy, EntityInit, Message, Stage};
use storm_script::{Host, Runtime, SourceLoader, Value};
type Trace = Rc<RefCell<Vec<String>>>;
fn record(trace: &Trace, text: impl Into<String>) {
    trace.borrow_mut().push(text.into());
}
struct Probe {
    trace: Trace,
    root: Option<AttributeId>,
}
impl Entity for Probe {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        self.root = ctx.attributes;
        record(&self.trace, "init");
        ctx.store
            .set_attribute(ctx.attributes.unwrap(), "initialized", Some("1"))?;
        Ok(true)
    }
    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        ensure!(message.long()? == 1, "probe command");
        let n = message.long()?;
        record(&self.trace, format!("message:{n}"));
        store.set_attribute(self.root.unwrap(), "native_value", Some(&n.to_string()))?;
        Ok(n as u64)
    }
    fn process_stage(&mut self, stage: Stage, delta: u32) {
        record(&self.trace, format!("{stage:?}:{delta}"));
    }
    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        record(
            &self.trace,
            format!(
                "attribute:{}:{}",
                store.name(changed).unwrap(),
                store.get(changed).unwrap().value().unwrap()
            ),
        );
        Ok(0)
    }
    fn destroy(&mut self, ctx: &mut EntityDestroy<'_>) -> Result<()> {
        ensure!(
            !ctx.manager.is_valid(ctx.id) && ctx.manager.get(ctx.id).is_some(),
            "destruction lifetime"
        );
        record(&self.trace, "destroy");
        Ok(())
    }
}
fn host(trace: &Trace) -> Result<NativeHost> {
    let mut host = NativeHost::new()?;
    let records = trace.clone();
    host.set_trace_sink(move |text| {
        record(&records, format!("script:{text}"));
        Ok(())
    });
    let records = trace.clone();
    host.entities.register_class("runtime_test", move || {
        Ok(Box::new(Probe {
            trace: records.clone(),
            root: None,
        }))
    })?;
    Ok(host)
}
struct ObserverService {
    name: &'static str,
    section: Section,
    trace: Trace,
    skip_once: bool,
}
impl Service for ObserverService {
    fn section(&self) -> Section {
        self.section
    }
    fn run_start(&mut self, frame: &mut ServiceFrame) -> Result<()> {
        record(&self.trace, format!("{}:start", self.name));
        if self.skip_once {
            frame.skip_frame = true;
            self.skip_once = false;
        }
        Ok(())
    }
    fn run_end(&mut self, _: &mut ServiceFrame) -> Result<()> {
        record(&self.trace, format!("{}:end", self.name));
        Ok(())
    }
}

const LIFECYCLE: &str = r#"
object actor; int frames;
void Main() {
    Trace("main");
    CreateEntity(&actor,"runtime_test");
    actor.ready=1;
    LayerSetExecute(0); LayerSetRealize(1);
    LayerAddObject(0,actor,5); LayerAddObject(1,actor,5);
    Trace(SendMessage(actor,"ll",1,7)==7);
    SetEventHandler("frame","OnFrame",0);
    SetEventHandler("retire","OnRetire",0);
    SetEventHandler("ExitApplication","OnExit",0);
    PostEvent("retire",10);
}
void OnFrame() { frames++; Trace(frames); actor.tick=frames; }
void OnRetire() { Trace("retire"); DeleteClass(actor); Trace(IsEntity(actor)); }
void OnExit() { Trace("exit"); }
"#;

#[test]
fn script_owned_entities_messages_and_events_share_the_native_core_frame_order() -> Result<()> {
    let trace = Trace::default();
    let mut engine = Engine::from_runtime(Runtime::compile(LIFECYCLE)?, host(&trace)?);
    for (name, section) in [
        ("all-a", Section::All),
        ("all-b", Section::All),
        ("execute", Section::Execute),
        ("realize", Section::Realize),
    ] {
        engine.register_service(ObserverService {
            name,
            section,
            trace: trace.clone(),
            skip_once: false,
        })?;
    }
    assert_eq!(
        engine.frame(10)?,
        FrameResult {
            continued: true,
            skipped: false,
            posted_events: 0
        }
    );
    let id = engine.host.entities.all_ids()[0];
    assert!(id.raw() > u64::from(u32::MAX));
    assert_eq!(
        *trace.borrow(),
        [
            "script:main",
            "init",
            "attribute:ready:1",
            "message:7",
            "script:1",
            "script:1",
            "attribute:tick:1",
            "all-b:start",
            "all-a:start",
            "execute:start",
            "Execute:10",
            "execute:end",
            "realize:start",
            "Realize:10",
            "realize:end",
            "all-b:end",
            "all-a:end"
        ]
    );
    trace.borrow_mut().clear();
    assert_eq!(engine.frame(0)?.posted_events, 1);
    assert!(!engine.host.entities.is_valid(id) && engine.host.entities.get(id).is_none());
    assert_eq!(
        *trace.borrow(),
        [
            "script:retire",
            "script:0",
            "script:2",
            "attribute:tick:2",
            "all-b:start",
            "all-a:start",
            "execute:start",
            "execute:end",
            "realize:start",
            "realize:end",
            "destroy",
            "all-b:end",
            "all-a:end"
        ]
    );
    trace.borrow_mut().clear();
    engine.frame(5)?;
    assert_eq!(trace.borrow()[0], "script:3");
    assert!(!trace.borrow().iter().any(|s| s.starts_with("attribute:")));
    engine.shutdown()?;
    engine.shutdown()?;
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|s| *s == "script:exit")
            .count(),
        1
    );
    assert_eq!(engine.phase(), Phase::Stopped);
    assert!(engine.frame(0).is_err());
    Ok(())
}

#[test]
fn service_skip_leaves_pending_deletion_until_a_later_lifecycle_and_exit_cleans_up() -> Result<()> {
    let trace = Trace::default();
    let runtime = Runtime::compile(
        r#"object a; void Main(){CreateEntity(&a,"runtime_test"); DeleteClass(a); Trace(IsEntity(a));}"#,
    )?;
    let mut engine = Engine::from_runtime(runtime, host(&trace)?);
    engine.register_service(ObserverService {
        name: "all",
        section: Section::All,
        trace: trace.clone(),
        skip_once: true,
    })?;
    assert!(engine.frame(16)?.skipped);
    assert!(!trace.borrow().contains(&"destroy".into()));
    assert!(!trace.borrow().contains(&"all:end".into()));
    assert!(!engine.frame(16)?.skipped);
    assert_eq!(trace.borrow().iter().filter(|s| *s == "destroy").count(), 1);
    engine.request_exit();
    assert!(!engine.frame(16)?.continued);
    engine.shutdown()?;
    let mut again = Engine::from_runtime(
        Runtime::compile("object a; void Main(){CreateEntity(&a,\"runtime_test\");}")?,
        host(&trace)?,
    );
    again.frame(0)?;
    again.shutdown()?;
    assert!(again.host.entities.all_ids().is_empty());
    assert_eq!(trace.borrow().iter().filter(|s| *s == "destroy").count(), 2);
    Ok(())
}

#[test]
fn original_layer_builtins_freeze_remove_retype_and_keep_unsigned_priority() -> Result<()> {
    let trace = Trace::default();
    let runtime = Runtime::compile(
        r#"object a; int frames;
    void Main(){CreateEntity(&a,"runtime_test");LayerAddObject(0,a,-1);LayerAddObject(1,a,0);SetEventHandler("frame","Tick",0);}
    void Tick(){frames++;if(frames==1)LayerFreeze(0,1);if(frames==2){LayerFreeze(0,0);LayerDelObject(1,a);}if(frames==3)LayerSetRealize(0);}
    "#,
    )?;
    let mut engine = Engine::from_runtime(runtime, host(&trace)?);
    engine.frame(11)?;
    engine.frame(12)?;
    engine.frame(13)?;
    assert_eq!(
        trace
            .borrow()
            .iter()
            .filter(|s| s.starts_with("Execute:") || s.starts_with("Realize:"))
            .cloned()
            .collect::<Vec<_>>(),
        ["Realize:11", "Execute:12", "Realize:13"]
    );
    assert!(
        engine
            .host
            .entities
            .is_valid(engine.host.entities.all_ids()[0])
    );
    engine.shutdown()?;
    struct Ordered(&'static str, Trace);
    impl Entity for Ordered {
        fn process_stage(&mut self, _: Stage, _: u32) {
            record(&self.1, self.0);
        }
    }
    let mut native = NativeHost::new()?;
    for name in ["first", "last"] {
        let records = trace.clone();
        native
            .entities
            .register_class(name, move || Ok(Box::new(Ordered(name, records.clone()))))?;
    }
    trace.borrow_mut().clear();
    let mut priorities = Engine::from_runtime(
        Runtime::compile(
            r#"object a,b; void Main(){
        CreateEntity(&a,"first");CreateEntity(&b,"last");
        LayerAddObject(0,b,-1);LayerAddObject(0,a,5);}"#,
        )?,
        native,
    );
    priorities.frame(0)?;
    assert_eq!(*trace.borrow(), ["first", "last"]);
    priorities.shutdown()?;
    Ok(())
}

#[test]
fn delete_entities_destroys_the_old_scene_before_creating_the_next_one() -> Result<()> {
    let trace = Trace::default();
    let mut engine = Engine::from_runtime(
        Runtime::compile(
            r#"
            object first, second, replacement;
            #event_handler("transition","Transition");
            void Main() {
                CreateEntity(&first,"runtime_test");
                CreateEntity(&second,"runtime_test");
                LayerAddObject(0,&first,0);
                LayerAddObject(0,&second,1);
            }
            void Transition() {
                DeleteClass(&second);
                DeleteEntities();
                Trace("old="+IsEntity(first)+":"+IsEntity(second));
                CreateEntity(&replacement,"runtime_test");
                LayerAddObject(0,&replacement,0);
                Trace("new="+IsEntity(replacement)+":"+IsEntity(first));
            }
            "#,
        )?,
        host(&trace)?,
    );
    engine.frame(0)?;
    let old = engine.host.entities.all_ids();
    trace.borrow_mut().clear();

    engine.emit_event("transition", &mut Message::new("", vec![])?)?;
    assert_eq!(
        trace.take(),
        [
            "destroy",
            "destroy",
            "script:old=0:0",
            "init",
            "script:new=1:0"
        ]
    );
    let active = engine.host.entities.all_ids();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].raw() as u32, old[0].raw() as u32);
    assert!(old.iter().all(|&id| engine.host.entities.get(id).is_none()));
    engine.frame(16)?;
    assert_eq!(trace.take(), ["Execute:16"]);
    Ok(())
}

#[test]
fn failing_shutdown_releases_entities_and_never_repeats_the_application_exit_event() -> Result<()> {
    struct Failing(Trace);
    impl Entity for Failing {
        fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
            record(&self.0, "failing-destroy");
            anyhow::bail!("native release failed")
        }
    }
    let trace = Trace::default();
    let mut native = host(&trace)?;
    let records = trace.clone();
    native
        .entities
        .register_class("failing", move || Ok(Box::new(Failing(records.clone()))))?;
    let mut engine = Engine::from_runtime(
        Runtime::compile(
            r#"object a;void Main(){
        CreateEntity(&a,"failing");SetEventHandler("ExitApplication","Exit",0);}
        void Exit(){Trace("exit-once");}"#,
        )?,
        native,
    );
    engine.frame(0)?;
    let id = engine.host.entities.all_ids()[0];
    assert!(engine.shutdown().is_err());
    assert!(engine.host.entities.get(id).is_none());
    assert_eq!(engine.phase(), Phase::Faulted);
    engine.shutdown()?;
    assert_eq!(engine.phase(), Phase::Stopped);
    assert_eq!(*trace.borrow(), ["script:exit-once", "failing-destroy"]);
    Ok(())
}

#[test]
fn unsupported_functions_classes_and_invalid_layers_fault_with_source_context() -> Result<()> {
    for source in [
        "native void Missing(); void Main(){Missing();}",
        "object a; void Main(){CreateEntity(&a,\"missing_class\");}",
        "void Main(){LayerSetExecute(32);}",
        "void Main(){LayerFreeze(0,\"1\");}",
    ] {
        let trace = Trace::default();
        let mut engine = Engine::from_runtime(Runtime::compile(source)?, host(&trace)?);
        let error = format!("{:#}", engine.frame(16).unwrap_err());
        assert!(error.contains("<source>:1:"), "{error}");
        assert_eq!(engine.phase(), Phase::Faulted);
        assert!(engine.frame(16).is_err());
        engine.shutdown()?;
    }
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
#[test]
fn platform_close_events_use_the_existing_runtime_and_shutdown_emits_exit_once() -> Result<()> {
    let trace = Trace::default();
    let source = r#"int closed;void Main(){SetEventHandler("DestroyWindow","Closed",0);SetEventHandler("ExitApplication","Exit",0);}
        void Closed(){closed++;Trace(closed);}void Exit(){Trace("exit");}"#;
    let mut engine = Engine::from_runtime(Runtime::compile(source)?, host(&trace)?);
    assert!(
        engine
            .emit_event("DestroyWindow", &mut Message::new("", vec![])?)
            .is_err()
    );
    engine.frame(0)?;
    engine.emit_event("DestroyWindow", &mut Message::new("", vec![])?)?;
    engine.shutdown()?;
    engine.shutdown()?;
    assert_eq!(*trace.borrow(), ["script:1", "script:exit"]);
    assert!(
        engine
            .emit_event("DestroyWindow", &mut Message::new("", vec![])?)
            .is_err()
    );
    Ok(())
}
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "storm-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn configured_game_startup_uses_case_insensitive_paths_and_explicit_legacy_encoding() -> Result<()>
{
    let temp = Temporary::new()?;
    std::fs::create_dir(temp.0.join("Program"))?;
    std::fs::create_dir(temp.0.join("Program/Data"))?;
    std::fs::write(
        temp.0.join("ENGINE.INI"),
        "; startup\r\n PrOgRaM_DiReCtOrY = PROGRAM\r\nrun = MAIN.C\r\nrun=wrong.c\n[script]\nrun=ignored.c",
    )?;
    // Storm strings have no C escape processing, so use one Windows separator.
    std::fs::write(
        temp.0.join("Program/Main.c"),
        b"#include \"data\\HELPER.C\"\nvoid Main(){Trace(Text());}",
    )?;
    let (bytes, _, errors) = encoding_rs::WINDOWS_1251.encode("string Text(){return \"парус\";}");
    assert!(!errors);
    std::fs::write(temp.0.join("Program/Data/helper.c"), bytes)?;
    let game = GameDirectory::open(&temp.0)?;
    let trace = Trace::default();
    let startup = Startup::load(&game)?;
    let mut engine = startup.engine(&game, ScriptEncoding::Windows1251, host(&trace)?)?;
    assert_eq!(engine.sources(), ["main.c", "data/helper.c"]);
    engine.frame(0)?;
    engine.shutdown()?;
    assert_eq!(*trace.borrow(), ["script:парус"]);
    assert!(
        startup
            .engine(&game, ScriptEncoding::Utf8, host(&trace)?)
            .is_err()
    );
    assert!(
        Startup::parse("run=main.c;literal")
            .unwrap()
            .run
            .ends_with(";literal")
    );
    assert!(Startup::parse("[script]\nrun=main.c").is_err());
    assert!(Startup::parse("run=\n").is_err());
    Ok(())
}

#[test]
fn rooted_files_reject_escape_and_nonregular_or_oversized_inputs() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::create_dir(temp.0.join("Program"))?;
    std::fs::write(temp.0.join("Program/test.c"), b"abc")?;
    let game = GameDirectory::open(&temp.0)?;
    assert_eq!(game.read("PROGRAM\\TEST.C", 3)?, b"abc");
    for path in [
        "../test.c",
        "/Program/test.c",
        "C:\\test.c",
        "Program//test.c",
        "missing.c",
        "",
    ] {
        assert!(game.resolve(path).is_err(), "{path}");
    }
    assert!(game.read("Program/test.c", 2).is_err());
    assert!(game.read("Program", 10).is_err());
    #[cfg(unix)]
    {
        let other = Temporary::new()?;
        std::fs::write(other.0.join("outside.c"), b"private")?;
        std::os::unix::fs::symlink(&other.0, temp.0.join("escape"))?;
        assert!(game.read("escape/outside.c", 100).is_err());
    }
    let mut source = GameSource {
        directory: game.directory("Program")?,
        encoding: ScriptEncoding::Utf8,
    };
    assert_eq!(source.load("test.c")?, "abc");
    assert!(ScriptEncoding::Utf8.decode(&[0xff]).is_err());
    assert_eq!(
        ScriptEncoding::Windows1251.decode(b"\xef\xbb\xbfhello")?,
        "hello"
    );
    assert!(ScriptEncoding::Utf8.decode(b"a\0b").is_err());
    assert!(
        ScriptEncoding::Windows1251
            .decode(b"\xef\xbb\xbfa\0b")
            .is_err()
    );
    let mut native = NativeHost::new()?;
    native.register_function("Answer", |_| Ok(Value::Int(42)))?;
    assert!(
        native
            .register_function("ANSWER", |_| Ok(Value::Int(0)))
            .is_err()
    );
    assert_eq!(Host::call(&mut native, "answer", &[])?, Value::Int(42));
    Ok(())
}

#[test]
fn configured_xinterface_loads_empty_dialogs_and_draws_live_bitmap_strings() -> Result<()> {
    use std::rc::Rc;
    use storm_render::interface_text::Alignment;
    use storm_script::Value;
    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    std::fs::create_dir_all(ini.join("INTERFACES"))?;
    std::fs::create_dir_all(temp.0.join("RESOURCE/Textures/Fonts"))?;
    std::fs::write(
        ini.join("INTERFACES/interfaces.ini"),
        "[PC_SCREEN]\nwScreenWidth=800\nwScreenHeight=600\nwScreenLeft=12\nwScreenTop=17\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/menu.ini"),
        "[MAIN]\n;item=BUTTON,legacy\nitem=80,XBOX,BUTTON,console\nitem=LANG,Polish,BUTTON,other\n[Legacy]\nitem=BUTTON,unused",
    )?;
    let mut font = b"[Interface_Normal]\nTexture=Fonts/Test.tga\nTexture_xsize=2\nTexture_ysize=2\nHeight=2\nPCScale=0.6\nchar_A=0,0,2,2\nchar_".to_vec();
    font.extend([0xdf]);
    font.extend(b"=0,0,2,2\n");
    std::fs::write(ini.join("fonts.ini"), font)?;
    let mut texture = [0u32, 2, 2, 1, 21, 16]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    texture.extend([255; 16]);
    std::fs::write(temp.0.join("RESOURCE/Textures/Fonts/Test.tga.tx"), texture)?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object xi;
        int cancelled;
        #event_handler("Edit", "Edit");
        #event_handler("Hide", "Hide");
        #event_handler("Show", "Show");
        #event_handler("Release", "Release");
        #event_handler("Missing", "Missing");
        #event_handler("exitCancel", "Cancel");
        #event_handler("State", "State");
        void Main() {
            CreateEntity(&xi, "XINTERFACE");
            LayerAddObject(5, &xi, 0);
            SendMessage(&xi, "ls", 45010, "resource\ini\interfaces\menu.ini");
            xi.strings.version = "AЯ";
            SendMessage(&xi, "lssllllf", 45012, "Version", "interface_normal",
                argb(255, 255, 255, 255), 780, 580, 1, 0.9);
        }
        void Edit() {
            xi.strings.Version = "ЯA";
            SendMessage(&xi, "lsl", 45051, "VERSION", argb(128, 210, 100, 30));
        }
        void Hide() { SendMessage(&xi, "ls", 45015, "Version"); }
        void Show() { SendMessage(&xi, "ls", 45014, "version"); }
        void Release() { SendMessage(&xi, "l", 45011); }
        void Missing() { SendMessage(&xi, "ls", 45010, "resource\ini\interfaces\missing.ini"); }
        void Cancel() { cancelled++; }
        int State() { return cancelled; }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    let draws = engine.host.interface_text_draws();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].text, "AЯ");
    assert_eq!(draws[0].canvas, [800, 600]);
    assert_eq!(draws[0].position, [792, 597]);
    assert!(draws[0].alignment == Alignment::Right);
    assert_eq!(draws[0].font.glyphs[&'Я'], [0, 0, 2, 2]);

    let mut message = Message::new("", vec![])?;
    engine.emit_event("Edit", &mut message)?;
    engine.frame(1)?;
    let edited = engine.host.interface_text_draws();
    assert_eq!(edited[0].text, "ЯA");
    assert_eq!(edited[0].rgba, [210, 100, 30, 128]);
    assert!(Rc::ptr_eq(&draws[0].font, &edited[0].font));
    engine.emit_event("Hide", &mut message)?;
    engine.frame(1)?;
    assert!(engine.host.interface_text_draws().is_empty());
    engine.emit_event("Show", &mut message)?;
    engine.frame(1)?;
    assert_eq!(engine.host.interface_text_draws().len(), 1);
    engine.emit_event("Release", &mut message)?;
    engine.frame(1)?;
    assert!(engine.host.interface_text_draws().is_empty());
    engine.emit_event("Missing", &mut message)?;
    engine.frame(1)?;
    engine.frame(1)?;
    assert_eq!(
        engine.emit_event("State", &mut message)?,
        Some(Value::Int(1))
    );
    Ok(())
}

#[test]
fn configured_dialog_draws_ordered_pictures_and_titles_through_window_state() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::{Picture, Text};

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&textures)?;
    std::fs::create_dir_all(ini.join("INTERFACES"))?;
    std::fs::create_dir_all(ini.join("texts/test"))?;
    std::fs::write(
        ini.join("INTERFACES/interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("texts/language.ini"),
        "[COMMON]\ndefaultLanguage=Test\nstrings=common.ini\n[DIRECTORY]\nTest=test\n",
    )?;
    std::fs::write(
        ini.join("texts/test/common.ini"),
        "string=Caption,\"Test Title\"\n",
    )?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Font]\nTexture=interfaces/panel.tga\nTexture_xsize=2\nTexture_ysize=2\nHeight=2\nchar_T=0,0,2,2\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/defaultnode.ini"),
        "[PLATE]\nclass=PICTURE\ntextureName=interfaces/panel.tga\n[Title]\nfont=Font\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/pictures.ini"),
        "[Atlas]\nsTextureName=panel.tga\nwTextureWidth=2\nwTextureHeight=2\npicture=Patch,0,0,1,1\n",
    )?;
    let mut texture = [0u32, 2, 2, 1, 21, 16].map(u32::to_le_bytes).concat();
    texture.extend([255; 16]);
    std::fs::write(textures.join("panel.tga.tx"), texture)?;
    std::fs::write(
        ini.join("INTERFACES/dialog.ini"),
        r#"
        [MAIN]
        item=95,PICTURE,Front
        item=20,PLATE,Back
        item=TITLE,Header
        item=WINDOW,Child
        item=WINDOW,Parent
        [Back]
        bAbsoluteRectangle=5
        position=0,0,800,600
        [Front]
        position=20,30,80,100
        [Header]
        position=0,8,800,42
        [Child]
        nodelist=Front
        [Parent]
        nodelist=Child,Back
        nodelist=Header,Absent
        show=0
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "script_interface_functions"
        native void XI_WindowShow(string name, bool show);
        native void XI_WindowDisable(string name, bool disabled);
        native bool XI_IsWindowEnable(string name);
        object ui;
        #event_handler("Show","Show");
        #event_handler("Release","Release");
        #event_handler("Active","Active");
        #event_handler("Enable","Enable");
        void Main() {
            ui.title="Caption";
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\dialog.ini");
            ui.strings.overlay="T";
            SendMessage(&ui,"lssllllf",45012,"Overlay","Font",argb(255,255,255,255),1,2,0,1.0);
        }
        void Show() {
            XI_WindowShow("parent",true);
            SendMessage(&ui,"lslss",45045,"Front",6,"Atlas","patch");
            SendMessage(&ui,"lsll",45045,"Front",4,argb(180,101,202,15));
        }
        void Release() { SendMessage(&ui,"l",45011); }
        int Active() { return XI_IsWindowEnable("Child"); }
        void Enable() { XI_WindowDisable("Parent",false); }
    "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    assert!(matches!(
        engine.host.interface_draws().as_slice(),
        [Text(_)]
    ));
    let mut event = Message::new("", vec![])?;
    engine.emit_event("Show", &mut event)?;
    engine.frame(1)?;
    let draws = engine.host.interface_draws();
    let [Picture(back), Text(title), Text(overlay), Picture(front)] = draws.as_slice() else {
        panic!("expected a picture, title, dynamic text and foreground picture");
    };
    assert_eq!(back.rect, [0.0, 0.0, 1066.0, 600.0]);
    assert_eq!(title.text, "Test Title");
    assert_eq!(title.position, [533, 8]);
    assert_eq!(overlay.text, "T");
    assert_eq!(front.uv, [0.25; 4]);
    assert_eq!(front.rgba, [101, 202, 15, 180]);
    assert!(Rc::ptr_eq(&front.texture, &back.texture));
    assert_eq!(
        engine.emit_event("Active", &mut event)?,
        Some(Value::Int(0))
    );
    engine.emit_event("Enable", &mut event)?;
    assert_eq!(
        engine.emit_event("Active", &mut event)?,
        Some(Value::Int(1))
    );
    engine.emit_event("Release", &mut event)?;
    engine.frame(1)?;
    assert!(engine.host.interface_draws().is_empty());
    Ok(())
}

#[test]
fn dialog_buttons_dispatch_to_the_frontmost_unlocked_node() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::Picture;
    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI/INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ini)?;
    std::fs::create_dir_all(&textures)?;
    let mut texture = [0u32, 2, 2, 1, 21, 16].map(u32::to_le_bytes).concat();
    texture.extend([255; 16]);
    std::fs::write(textures.join("icons.tga.tx"), texture)?;
    std::fs::write(
        ini.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("pictures.ini"),
        "[Icons]\nsTextureName=icons.tga\nwTextureWidth=2\nwTextureHeight=2\npicture=Symbol,0,0,2,2\n",
    )?;
    std::fs::write(
        ini.join("defaultnode.ini"),
        r#"
        [BUTTON]
        position=0,0,80,80
        group=Icons
        picture=Symbol
        shadowColor=0,0,0,0
        pressPictureOffset=4,5
        pressDelay=4
    "#,
    )?;
    std::fs::write(
        ini.join("buttons.ini"),
        r#"
        [MAIN]
        item=100,BUTTON,Top
        item=80,BUTTON,Bottom
        item=WINDOW,Panel
        start=Top
        [Top]
        command=click,event:Click
        command=activate,event:Click,select:Bottom
        [Bottom]
        command=click,event:Click
        UseActionByDeclick=1
        [Panel]
        nodelist=Top,Bottom
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        #libriary "script_interface_functions"
        native void XI_WindowDisable(string name, bool disable);
        object ui;
        bool reloadOnSound;
        #event_handler("ArmReload","ArmReload");
        #event_handler("ievnt_command","Command");
        #event_handler("ISound","Sound");
        #event_handler("Click","Click");
        #event_handler("Lock","Lock");
        #event_handler("Unlock","Unlock");
        #event_handler("Current","Current");
        #event_handler("DisableTop","DisableTop");
        void Main() {
            MapControl(CreateControl("ILClick"),1);
            MapControl(CreateControl("IAction"),32);
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(4,&ui,0);
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\buttons.ini");
        }
        void ArmReload() { reloadOnSound = true; }
        void Sound() {
            Trace("sound="+GetEventData());
            if (reloadOnSound)
                SendMessage(&ui,"ls",45010,"resource\ini\interfaces\buttons.ini");
        }
        void Command() {
            string action=GetEventData();
            string node=GetEventData();
            Trace(action+":"+node);
        }
        void Click() {
            int code=GetEventData();
            string node=GetEventData();
            Trace("named="+code+":"+node);
        }
        void Lock() { XI_WindowDisable("Panel",true); }
        void Unlock() { XI_WindowDisable("Panel",false); }
        void DisableTop() { SendMessage(&ui,"lsl",45024,"Top",0); }
        string Current() {
            string node;
            SendMessage(&ui,"le",45028,&node);
            return node;
        }
    "#,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(1280, 720);
    engine.host.set_interface_pointer(Some([190.0, 45.0]));
    engine.frame(0)?;
    let mut event = Message::new("", vec![])?;
    let current = |engine: &mut Engine, event: &mut Message| engine.emit_event("Current", event);
    assert_eq!(
        current(&mut engine, &mut event)?,
        Some(Value::String("Top".into()))
    );
    let input = |engine: &mut Engine, code, pressed| -> Result<()> {
        engine.control_input(ControlInput::Button { code, pressed });
        engine.frame(16)?;
        engine.frame(16)?;
        Ok(())
    };

    input(&mut engine, 1, true)?;
    assert_eq!(trace.take(), ["script:sound=1", "script:click:Top"]);
    let draws = engine.host.interface_draws();
    let [Picture(bottom), Picture(top)] = draws.as_slice() else {
        panic!("two button faces")
    };
    assert_eq!(bottom.rect, [133.0, 0.0, 213.0, 80.0]);
    assert_eq!(top.rect, [137.0, 5.0, 217.0, 85.0]);
    input(&mut engine, 1, false)?;
    assert_eq!(trace.take(), ["script:named=12:Top"]);

    engine.emit_event("Lock", &mut event)?;
    input(&mut engine, 1, true)?;
    input(&mut engine, 1, false)?;
    assert!(trace.borrow().is_empty());
    engine.emit_event("Unlock", &mut event)?;
    input(&mut engine, 32, true)?;
    assert_eq!(trace.take(), ["script:sound=1", "script:activate:Top"]);
    input(&mut engine, 32, false)?;
    assert_eq!(trace.take(), ["script:named=5:Top"]);
    assert_eq!(
        current(&mut engine, &mut event)?,
        Some(Value::String("Bottom".into()))
    );

    engine.emit_event("DisableTop", &mut event)?;
    input(&mut engine, 1, true)?;
    assert!(trace.borrow().is_empty());
    input(&mut engine, 1, false)?;
    assert_eq!(trace.take(), ["script:sound=1", "script:click:Bottom"]);
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:named=12:Bottom"]);
    engine.emit_event("ArmReload", &mut event)?;
    input(&mut engine, 1, true)?;
    assert!(trace.borrow().is_empty());
    input(&mut engine, 1, false)?;
    assert_eq!(trace.take(), ["script:sound=1"]);
    engine.frame(16)?;
    assert!(trace.borrow().is_empty());
    assert_eq!(
        current(&mut engine, &mut event)?,
        Some(Value::String("Top".into()))
    );

    Ok(())
}

#[test]
fn image_collections_preserve_atlas_order_and_support_script_edits() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::Picture;

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI/INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ini)?;
    std::fs::create_dir_all(&textures)?;
    for (name, tint) in [("first", [90, 30, 20, 255]), ("second", [20, 70, 30, 255])] {
        let mut bytes = [0u32, 4, 4, 1, 21, 64].map(u32::to_le_bytes).concat();
        bytes.extend(tint.repeat(16));
        std::fs::write(textures.join(format!("{name}.tga.tx")), bytes)?;
    }
    std::fs::write(
        ini.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("pictures.ini"),
        r#"
        [First]
        sTextureName=first.tga
        wTextureWidth=4
        wTextureHeight=4
        picture=Border,0,0,2,2
        picture=Corner,0,2,2,4
        [Second]
        sTextureName=second.tga
        wTextureWidth=4
        wTextureHeight=4
        picture=Border,2,0,4,2
        picture=Corner,2,2,4,4
        "#,
    )?;
    std::fs::write(
        ini.join("collection.ini"),
        r#"
        [MAIN]
        item=80,IMAGECOLLECTION,Relative
        item=100,IMAGECOLLECTION,Absolute
        [Relative]
        groupName=First
        offset=7,-3
        picture=editsection:initial
        picture=Border,col:{180,110,22,43},pos:{1,2,8,9}
        picture=editsection:end
        [Absolute]
        groupName=First
        bAbsoluteRectangle=5
        picture=Corner,pos:{5,6,15,16}
        "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("Swap", "Swap");
        #event_handler("Edit", "Edit");
        #event_handler("Clear", "Clear");
        #event_handler("Append", "Append");
        void Main() {
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\collection.ini");
        }
        void Swap() { SendMessage(&ui,"lsls",45045,"Relative",1,"Second"); }
        void Append() {
            SendMessage(&ui,"lslslllll",45045,"Relative",0,"Border",
                argb(255,128,128,128),10,20,30,40);
        }
        void Edit() {
            Append();
            SendMessage(&ui,"lslll",45045,"Relative",3,0,argb(200,32,64,96));
            SendMessage(&ui,"lslls",45045,"Relative",4,1,"Corner");
        }
        void Clear() { SendMessage(&ui,"lsl",45045,"Relative",2); }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    let initial = engine.host.interface_draws();
    let [Picture(relative), Picture(absolute)] = initial.as_slice() else {
        panic!("expected both image collections");
    };
    assert_eq!(relative.rect, [134.0, 2.0, 141.0, 9.0]);
    assert_eq!(relative.uv, [0.125, 0.125, 0.375, 0.375]);
    assert_eq!(relative.rgba, [110, 22, 43, 180]);
    assert_eq!(absolute.rect, [5.0, 6.0, 15.0, 16.0]);

    let mut event = Message::new("", vec![])?;
    engine.emit_event("Swap", &mut event)?;
    engine.frame(16)?;
    let swapped = engine.host.interface_draws();
    let Picture(swap) = &swapped[0] else {
        panic!("swapped collection")
    };
    assert!(!Rc::ptr_eq(&relative.texture, &swap.texture));
    assert_eq!(relative.uv, swap.uv);

    engine.emit_event("Edit", &mut event)?;
    engine.frame(16)?;
    let edited = engine.host.interface_draws();
    let [Picture(border), Picture(corner), Picture(last)] = edited.as_slice() else {
        panic!("expected appended and edited parts before the next widget");
    };
    assert_eq!(border.uv, [0.625, 0.125, 0.875, 0.375]);
    assert_eq!(border.rgba, [32, 64, 96, 200]);
    assert_eq!(corner.uv, [0.625, 0.625, 0.875, 0.875]);
    assert_eq!(corner.rect, [150.0, 17.0, 170.0, 37.0]);
    assert_eq!(last.rect, absolute.rect);
    assert!(Rc::ptr_eq(&border.texture, &corner.texture));

    engine.emit_event("Clear", &mut event)?;
    engine.frame(16)?;
    assert_eq!(engine.host.interface_draws().len(), 1);
    engine.emit_event("Append", &mut event)?;
    engine.frame(16)?;
    let rebuilt = engine.host.interface_draws();
    let Picture(rebuilt) = &rebuilt[0] else {
        panic!("rebuilt collection")
    };
    assert!(Rc::ptr_eq(&rebuilt.texture, &border.texture));
    assert_eq!(rebuilt.uv, border.uv);
    Ok(())
}

#[test]
fn dialog_modal_preserves_fade_layers_and_edits_localized_string_collections() -> Result<()> {
    use storm_render::interface_text::{
        Alignment,
        InterfaceDraw::{Rectangle, Text},
    };

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let ui = ini.join("INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ui)?;
    std::fs::create_dir_all(&textures)?;
    std::fs::create_dir_all(ini.join("texts/test"))?;
    let mut tx = [0u32, 4, 4, 1, 21, 64].map(u32::to_le_bytes).concat();
    tx.extend([255; 64]);
    std::fs::write(textures.join("face.tga.tx"), tx)?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Face]\nTexture=Interfaces/face.tga\nTexture_xsize=4\nTexture_ysize=4\nHeight=4\nchar_А=0,0,4,4\nchar_Б=0,0,4,4\nchar_В=0,0,4,4\n",
    )?;
    std::fs::write(
        ui.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("texts/language.ini"),
        "[COMMON]\ndefaultLanguage=Test\nstrings=common.ini\n[DIRECTORY]\nTest=test\n",
    )?;
    std::fs::write(
        ini.join("texts/test/common.ini"),
        "string=Question,\"АБ\"\n",
    )?;
    std::fs::write(
        ui.join("confirm.ini"),
        r#"
        [MAIN]
        item=600,RECTANGLE,Fader
        item=601,STRINGCOLLECTION,Labels
        item=WINDOW,Confirm
        [Fader]
        bAbsoluteRectangle=5
        position=0,0,800,600
        leftColor=128,64,128,255
        topColor=255,255,128,64
        rightColor=255,128,255,64
        bottomColor=64,128,255,255
        [Labels]
        string=Question,font:Face,pos:{400,230},fc:{255,240,220,200},bc:{0,0,0,0},scale:1.0,state:{CS}
        string=#В,font:Face,pos:{420,250},fc:{255,255,255,255},bc:{0,0,0,0},scale:1.0,state:{R}
        [Confirm]
        show=0
        nodelist=Fader,Labels
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r##"
        #libriary "script_interface_functions"
        native void XI_WindowShow(string node, bool shown);
        object ui;
        #event_handler("Show","Show");
        #event_handler("Hide","Hide");
        #event_handler("Reconfigure","Reconfigure");
        #event_handler("Remove","Remove");
        void Main() {
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\confirm.ini");
        }
        void Show() { XI_WindowShow("Confirm",true); }
        void Hide() { XI_WindowShow("Confirm",false); }
        void Reconfigure() {
            SendMessage(&ui,"lslllll",45045,"Fader",0,2,3,1000,590);
            SendMessage(&ui,"lslll",45045,"Fader",1,argb(200,32,60,80),argb(255,255,0,0));
            SendMessage(&ui,"lslls",45045,"Labels",1,1,"#БА");
            SendMessage(&ui,"lslll",45045,"Labels",2,2,1);
            SendMessage(&ui,"lslll",45045,"Labels",3,2,argb(100,40,60,80));
            SendMessage(&ui,"lslsssllllllfl",45045,"Labels",0,"Prompt","АБВ","Face",
                200,262,argb(255,255,255,255),0,2,1,1.0,7);
        }
        void Remove() { SendMessage(&ui,"lslss",45045,"Labels",0,"prompt",""); }
    "##,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    assert!(engine.host.interface_draws().is_empty());

    let mut event = Message::new("", vec![])?;
    engine.emit_event("Show", &mut event)?;
    engine.frame(16)?;
    let draws = engine.host.interface_draws();
    let [Rectangle(fader), Text(question), Text(literal)] = draws.as_slice() else {
        panic!("fade should precede the confirmation lines");
    };
    assert_eq!(fader.rect, [0, 0, 1066, 600]);
    assert_eq!(
        fader.corners,
        [
            [63, 64, 63, 127],
            [32, 127, 254, 32],
            [127, 127, 16, 254],
            [64, 254, 63, 63]
        ]
    );
    assert_eq!(question.position, [533, 230]);
    assert_eq!(question.text, "АБ");
    assert_eq!(question.rgba, [240, 220, 200, 255]);
    assert!(matches!(question.alignment, Alignment::Center) && question.shadow);
    assert_eq!(literal.text, "В");
    assert!(matches!(literal.alignment, Alignment::Right) && !literal.shadow);

    engine.emit_event("Reconfigure", &mut event)?;
    engine.frame(16)?;
    let draws = engine.host.interface_draws();
    let [Rectangle(fader), Text(first), Text(second), Text(dynamic)] = draws.as_slice() else {
        panic!("three lines after script replacement");
    };
    assert_eq!(fader.rect, [2, 3, 1000, 590]);
    assert_eq!(fader.corners, [[31, 59, 79, 199]; 4]);
    assert_eq!(fader.border, [255, 0, 0, 255]);
    assert_eq!(first.text, "БА");
    assert_eq!(second.text, "БА");
    assert_eq!(second.position, literal.position);
    assert_eq!(second.rgba, [40, 60, 80, 100]);
    assert_eq!(dynamic.position, [333, 262]);
    assert_eq!(dynamic.scale, 0.5);

    engine.emit_event("Remove", &mut event)?;
    engine.frame(16)?;
    assert_eq!(engine.host.interface_text_draws().len(), 2);
    engine.emit_event("Hide", &mut event)?;
    engine.frame(16)?;
    assert!(engine.host.interface_draws().is_empty());
    Ok(())
}

#[test]
fn dialog_table_refreshes_script_cells_and_reports_native_selection_indices() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::{Picture, Text};
    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let ui = ini.join("INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ui)?;
    std::fs::create_dir_all(&textures)?;
    let mut tx = [0u32, 8, 4, 1, 21, 128].map(u32::to_le_bytes).concat();
    tx.extend([255; 128]);
    std::fs::write(textures.join("table.tga.tx"), tx)?;
    std::fs::write(
        ui.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Face]\nTexture=Interfaces/table.tga\nTexture_xsize=8\nTexture_ysize=4\nHeight=4\nchar_A=0,0,4,4\nchar_B=0,0,4,4\nchar_C=0,0,4,4\nchar_D=0,0,4,4\nchar_H=0,0,4,4\nchar_X=0,0,4,4\n",
    )?;
    std::fs::write(
        ui.join("table.ini"),
        r#"
        [MAIN]
        item=100,TABLE,Skills
        start=Skills
        [Skills]
        position=20,30,62,72
        rowquantity=3
        colquantity=2
        rowsheight=10,16,16
        colswidth=20,22
        cellspacesize=2,0
        headerlineheight=1
        hlineheight=2
        fontcell=Face
        fonttitle=Face
        fontcellalignment=center
        iscolselect=1
        backimage=color=(255,40,50,60)
        selectimage=color=(128,90,50,30)
        command=click,select:Skills
        command=downstep
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("SetValue","SetValue");
        #event_handler("Refresh","Refresh");
        #event_handler("TableSelectChange","Selected");
        #event_handler("OnTableClick","Clicked");
        #event_handler("QueryRow","QueryRow");
        void Main() {
            MapControl(CreateControl("ILClick"),1);
            MapControl(CreateControl("IDown"),40);
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(4,&ui,0);
            LayerAddObject(5,&ui,0);
            ui.Skills.top=0;
            ui.Skills.hr.td1.str="H";
            ui.Skills.tr1.td1.str="A";
            ui.Skills.tr1.td1.textoffset="-2,-1";
            ui.Skills.tr1.td2.str="B";
            ui.Skills.tr2.td1.str="C";
            ui.Skills.tr2.td2.str="D";
            ui.Skills.tr3.td1.str="X";
            ui.Skills.tr2.speccolor=argb(85,25,70,60);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\table.ini");
        }
        void SetValue() { ui.Skills.tr1.td1.str="X"; }
        void Refresh() { SendMessage(&ui,"lsl",45045,"Skills",0); }
        void Selected() {
            string node=GetEventData(); int row=GetEventData(); int col=GetEventData();
            Trace(node+"="+row+":"+col+":"+ui.Skills.select+":"+ui.Skills.top);
        }
        void Clicked() {
            string node=GetEventData(); int row=GetEventData(); int col=GetEventData();
            Trace(node+" clicked "+row+":"+col);
        }
        void QueryRow() {
            int row=SendMessage(&ui,"lsl",45045,"Skills",1);
            Trace("visible="+row);
        }
    "#,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(800, 600);
    engine.frame(0)?;
    let draws = engine.host.interface_draws();
    let [
        Picture(back),
        Picture(row),
        Picture(highlight),
        Text(head),
        Text(cell),
        ..,
    ] = draws.as_slice()
    else {
        panic!("table background, selection, and cells");
    };
    assert_eq!(back.rect, [20.0, 30.0, 62.0, 72.0]);
    assert_eq!(row.rgba, [25, 70, 60, 85]);
    assert_eq!(highlight.rect, [20.0, 40.0, 40.0, 56.0]);
    assert_eq!(head.text, "H");
    assert_eq!(cell.text, "A");
    assert_eq!(cell.position, [27, 41]);

    let mut event = Message::new("", vec![])?;
    let texts = |engine: &Engine| {
        engine
            .host
            .interface_text_draws()
            .into_iter()
            .map(|draw| draw.text)
            .collect::<Vec<_>>()
    };
    engine.emit_event("SetValue", &mut event)?;
    engine.frame(16)?;
    assert_eq!(texts(&engine), ["H", "A", "B", "C", "D"]);
    engine.emit_event("Refresh", &mut event)?;
    engine.frame(16)?;
    assert_eq!(texts(&engine), ["H", "X", "B", "C", "D"]);

    let tap = |engine: &mut Engine, code| -> Result<()> {
        for pressed in [true, false] {
            engine.control_input(ControlInput::Button { code, pressed });
            engine.frame(16)?;
            engine.frame(16)?;
        }
        Ok(())
    };
    tap(&mut engine, 40)?;
    tap(&mut engine, 40)?;
    assert_eq!(
        trace.take(),
        ["script:Skills=2:1:2:0", "script:Skills=3:1:3:1"]
    );
    assert_eq!(texts(&engine), ["H", "C", "D", "X"]);

    engine.host.set_interface_pointer(Some([49.0, 46.0]));
    tap(&mut engine, 1)?;
    engine.emit_event("QueryRow", &mut event)?;
    assert_eq!(
        trace.take(),
        [
            "script:Skills=2:2:2:1",
            "script:Skills clicked 1:2",
            "script:visible=1"
        ]
    );
    Ok(())
}

#[test]
fn dialog_pc_edit_box_edits_unicode_and_confirms_the_script_owned_value() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::{Picture, Text};

    let temp = Temporary::new()?;
    let resource = temp.0.join("RESOURCE");
    let ui = resource.join("INI/INTERFACES");
    let textures = resource.join("Textures/Interfaces");
    std::fs::create_dir_all(&ui)?;
    std::fs::create_dir_all(&textures)?;
    let mut tx = [0u32, 8, 4, 1, 21, 128].map(u32::to_le_bytes).concat();
    tx.extend([255; 128]);
    std::fs::write(textures.join("editor.tga.tx"), tx)?;
    std::fs::write(
        ui.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        resource.join("INI/fonts.ini"),
        "[Face]\nTexture=Interfaces/editor.tga\nTexture_xsize=8\nTexture_ysize=4\nHeight=4\nchar_А=0,0,4,4\nchar_Б=0,0,4,4\nchar_В=0,0,4,4\nchar_Г=0,0,4,4\nchar_Я=0,0,4,4\nchar_Ш=0,0,8,4\nchar_Ю=0,0,4,4\nchar__=0,0,4,4\n",
    )?;
    std::fs::write(
        ui.join("pictures.ini"),
        "[UI]\nsTextureName=editor.tga\nwTextureWidth=8\nwTextureHeight=4\npicture=Face,0,0,8,4\n",
    )?;
    std::fs::write(
        ui.join("profile.ini"),
        r#"
        [MAIN]
        item=BUTTON,Outside
        item=100,PCEDITBOX,Profile
        item=PCEDITBOX,Blank
        start=Outside
        [Outside]
        position=300,300,340,340
        [Profile]
        command=click,select:Profile
        command=activate,event:Confirm
        position=20,30,62,44
        strFont=Face
        argbFontColor=255,240,220,200
        stringoffset=2,3
        stringLength=5
        stringWidth=20
        excludechars=* ?
        LeftImage=piclist=UI,picname=Face,size=(3,4)
        MiddleImage=color=(255,128,128,128),piclist=UI,picname=Face
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("Confirm","Confirm");
        #event_handler("editexit","EditExit");
        #event_handler("Rename","Rename");
        void Main() {
            MapControl(CreateControl("ILClick"),1);
            MapControl(CreateControl("IStartButton"),13);
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(4,&ui,0);
            LayerAddObject(5,&ui,0);
            ui.Profile.str = "АБВ";
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\profile.ini");
            Trace(ui.Profile.str);
            if (CheckAttribute(&ui,"Blank.str")) Trace("blank initialized");
        }
        void Confirm() { Trace("confirm="+ui.Profile.str); }
        void EditExit() {
            string node=GetEventData();
            if (node=="Profile") Trace("exit="+ui.Profile.str);
        }
        void Rename() { ui.Profile.str = "Ю"; }
    "#,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(800, 600);
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:АБВ", "script:blank initialized"]);
    assert!(!engine.host.interface_editor_focused());
    engine.control_input(ControlInput::Text('Г'));
    engine.frame(16)?;
    engine.frame(16)?;
    let draws = engine.host.interface_draws();
    let [Picture(left), Picture(middle), Text(name)] = draws.as_slice() else {
        panic!("edit box art and script name");
    };
    assert_eq!(left.rect, [20.0, 30.0, 23.0, 44.0]);
    assert_eq!(middle.rect, [23.0, 30.0, 62.0, 44.0]);
    assert_eq!(name.text, "АБВ");
    assert_eq!(name.position, [22, 33]);

    engine.host.set_interface_pointer(Some([37.0, 38.0]));
    engine.control_input(ControlInput::Button {
        code: 1,
        pressed: true,
    });
    for _ in 0..4 {
        engine.frame(16)?;
    }
    assert!(engine.host.interface_editor_focused());
    engine.control_input(ControlInput::Button {
        code: 1,
        pressed: false,
    });
    for code in [36, 39, 8] {
        engine.control_input(ControlInput::Button {
            code,
            pressed: true,
        });
        engine.control_input(ControlInput::Button {
            code,
            pressed: false,
        });
    }
    engine.control_input(ControlInput::Text('Я'));
    engine.control_input(ControlInput::Text('*'));
    for code in [46, 35] {
        engine.control_input(ControlInput::Button {
            code,
            pressed: true,
        });
        engine.control_input(ControlInput::Button {
            code,
            pressed: false,
        });
    }
    for c in ['Г', 'Ш', 'А'] {
        engine.control_input(ControlInput::Text(c));
    }
    engine.control_input(ControlInput::Button {
        code: 13,
        pressed: true,
    });
    for _ in 0..3 {
        engine.frame(16)?;
    }
    assert_eq!(trace.take(), ["script:confirm=ЯВГШ"]);
    let draws = engine.host.interface_draws();
    let [_, _, Text(name), Text(caret)] = draws.as_slice() else {
        panic!("edited name and caret");
    };
    assert_eq!(name.text, "ЯВГШ");
    assert_eq!(caret.text, "_");
    assert_eq!(caret.position, [42, 33]);

    engine.control_input(ControlInput::Button {
        code: 13,
        pressed: false,
    });
    engine.frame(16)?;
    engine.frame(16)?;
    assert_eq!(trace.take(), ["script:exit=ЯВГШ"]);
    engine.emit_event("Rename", &mut Message::new("", vec![])?)?;
    engine.frame(16)?;
    let draws = engine.host.interface_draws();
    let [_, _, Text(name), Text(caret)] = draws.as_slice() else {
        panic!("script replacement and clamped caret");
    };
    assert_eq!(name.text, "Ю");
    assert_eq!(caret.position, [26, 33]);
    Ok(())
}

#[test]
fn dialog_text_button_uses_atlas_caps_and_dispatches_on_mouse_release() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::{Picture, Text};

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let ui = ini.join("INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ui)?;
    std::fs::create_dir_all(&textures)?;
    std::fs::create_dir_all(ini.join("texts/test"))?;
    let mut tx = [0u32, 8, 4, 1, 21, 128].map(u32::to_le_bytes).concat();
    tx.extend([255; 128]);
    std::fs::write(textures.join("buttons.tga.tx"), tx)?;
    std::fs::write(
        ui.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("texts/language.ini"),
        "[COMMON]\ndefaultLanguage=Test\nstrings=common.ini\n[DIRECTORY]\nTest=test\n",
    )?;
    std::fs::write(ini.join("texts/test/common.ini"), "string=Apply,\"АБ\"\n")?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Face]\nTexture=Interfaces/buttons.tga\nTexture_xsize=8\nTexture_ysize=4\nHeight=8\nchar_А=0,0,4,4\nchar_Б=4,0,4,4\n",
    )?;
    std::fs::write(
        ui.join("pictures.ini"),
        r#"
        [Buttons]
        sTextureName=buttons.tga
        wTextureWidth=8
        wTextureHeight=4
        picture=Left,0,0,2,2
        picture=Middle,2,0,6,2
        picture=FocusLeft,0,2,2,4
        picture=FocusMiddle,2,2,6,4
    "#,
    )?;
    std::fs::write(
        ui.join("defaultnode.ini"),
        r#"
        [TEXTBUTTON2]
        class=TEXTBUTTON
        group=Buttons
        buttonLeft=Left
        buttonMiddle=Middle
        selectButtonLeft=FocusLeft
        selectButtonMiddle=FocusMiddle
        font=Face
        strOffset=5
        fontColorSelected=255,200,210,220
        fontColorPressed=255,130,140,150
        fontColor=255,180,180,180
        unselectableFontColor=255,60,60,60
        faceColor=255,128,128,128
        faceColorPressed=255,100,90,80
        backColor=128,20,30,40
        shadowOffset=1,2
        pressPictureOffset=2,3
        pressShadowOffset=-1,1
        pressDelay=2
        UseActionByDeclick=1
    "#,
    )?;
    std::fs::write(
        ui.join("confirm.ini"),
        r#"
        [MAIN]
        item=100,TEXTBUTTON2,Apply
        start=Apply
        [Apply]
        position=20,30,100,46
        string=Apply
        command=click,event:Confirmed
        command=activate,event:Confirmed
    "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r##"
        object ui;
        #event_handler("ISound","Sound");
        #event_handler("ievnt_command","Command");
        #event_handler("Confirmed","Confirmed");
        #event_handler("Edit","Edit");
        void Main() {
            MapControl(CreateControl("ILClick"),1);
            MapControl(CreateControl("IAction"),32);
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(4,&ui,0);
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\confirm.ini");
        }
        void Sound() { Trace("sound"); }
        void Command() { string action=GetEventData(); Trace(action); }
        void Confirmed() { int code=GetEventData(); Trace("confirmed="+code); }
        void Edit() {
            SendMessage(&ui,"lsls",45045,"Apply",0,"#БА");
            SendMessage(&ui,"lslllll",45045,"Apply",1,40,70,130,90);
            SendMessage(&ui,"lsl",45024,"Apply",0);
        }
    "##,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(800, 600);
    engine.host.set_interface_pointer(Some([50.0, 38.0]));
    engine.frame(0)?;
    let draws = engine.host.interface_draws();
    let [
        Picture(shadow),
        _,
        _,
        Picture(fill),
        Picture(left),
        Picture(middle),
        Picture(right),
        Text(text),
    ] = draws.as_slice()
    else {
        panic!("shadow, fill, face, and caption");
    };
    assert_eq!(shadow.rect, [21.0, 32.0, 23.0, 48.0]);
    assert_eq!(fill.rgba, [20, 30, 40, 128]);
    assert_eq!(left.rect, [20.0, 30.0, 22.0, 46.0]);
    assert_eq!(middle.rect, [22.0, 30.0, 98.0, 46.0]);
    assert_eq!(right.rect, [98.0, 30.0, 100.0, 46.0]);
    assert_eq!(right.uv, [left.uv[2], left.uv[1], left.uv[0], left.uv[3]]);
    assert_eq!(left.uv, [0.0625, 0.625, 0.1875, 0.875]);
    assert_eq!(text.position, [60, 38]);
    assert_eq!(text.vertical_anchor, Some(6.0));
    assert_eq!(text.text, "АБ");
    assert_eq!(text.rgba, [200, 210, 220, 255]);

    let input = |engine: &mut Engine, code, pressed| -> Result<()> {
        engine.control_input(ControlInput::Button { code, pressed });
        engine.frame(16)?;
        engine.frame(16)?;
        Ok(())
    };
    input(&mut engine, 1, true)?;
    for _ in 0..4 {
        engine.frame(16)?;
    }
    assert!(trace.borrow().is_empty());
    let pressed = engine.host.interface_draws();
    let [Picture(shadow), _, _, _, Picture(left), _, _, Text(text)] = pressed.as_slice() else {
        panic!("pressed face and caption");
    };
    assert_eq!(shadow.rect, [21.0, 34.0, 23.0, 50.0]);
    assert_eq!(left.rect, [22.0, 33.0, 24.0, 49.0]);
    assert_eq!(left.rgba, [100, 90, 80, 255]);
    assert_eq!(text.position, [62, 41]);
    assert_eq!(text.vertical_anchor, Some(6.0));
    assert_eq!(text.rgba, [130, 140, 150, 255]);

    input(&mut engine, 1, false)?;
    engine.frame(16)?;
    assert_eq!(
        trace.take(),
        ["script:sound", "script:click", "script:confirmed=12"]
    );
    input(&mut engine, 32, true)?;
    input(&mut engine, 32, false)?;
    assert_eq!(
        trace.take(),
        ["script:sound", "script:activate", "script:confirmed=5"]
    );

    engine.emit_event("Edit", &mut Message::new("", vec![])?)?;
    engine.frame(16)?;
    let edited = engine.host.interface_draws();
    let [_, _, _, _, Picture(left), _, _, Text(text)] = edited.as_slice() else {
        panic!("moved button and label");
    };
    assert_eq!(left.rect, [40.0, 70.0, 42.0, 90.0]);
    assert_eq!(text.text, "БА");
    assert_eq!(text.position, [85, 80]);
    assert_eq!(text.rgba, [60, 60, 60, 255]);
    Ok(())
}

#[test]
fn dialog_check_buttons_share_mouse_and_script_state_with_native_callbacks() -> Result<()> {
    use storm_render::interface_text::InterfaceDraw::{Picture, Text};

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let ui = ini.join("INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ui)?;
    std::fs::create_dir_all(&textures)?;
    std::fs::create_dir_all(ini.join("texts/test"))?;
    let mut tx = [0u32, 4, 4, 1, 21, 64].map(u32::to_le_bytes).concat();
    tx.extend([255; 64]);
    std::fs::write(textures.join("check.tga.tx"), tx)?;
    std::fs::write(
        ui.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("texts/language.ini"),
        "[COMMON]\ndefaultLanguage=Test\nstrings=common.ini\n[DIRECTORY]\nTest=test\n",
    )?;
    std::fs::write(ini.join("texts/test/common.ini"), "string=First,\"АБ\"\n")?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Face]\nTexture=Interfaces/check.tga\nTexture_xsize=4\nTexture_ysize=4\nHeight=4\nSpacebar=2\nchar_А=0,0,4,4\nchar_Б=0,0,4,4\n",
    )?;
    std::fs::write(
        ui.join("pictures.ini"),
        "[Icon]\nsTextureName=check.tga\nwTextureWidth=4\nwTextureHeight=4\npicture=On,0,0,4,2\npicture=Off,0,2,4,4\n",
    )?;
    std::fs::write(
        ui.join("defaultnode.ini"),
        "[RADIOBUTTON]\nclass=CHECKBUTTON\nexclusiveChoose=1\n",
    )?;
    std::fs::write(
        ui.join("choices.ini"),
        r#"
        [MAIN]
        item=100,RADIOBUTTON,Radio
        start=Radio
        [Radio]
        position=20,30,100,62
        command=click
        command=activate
        icongroup=Icon
        normalpicture=Off
        selectpicture=On
        normalpiccolor=255,128,128,128
        normalFontColor=255,170,170,170
        selectFontColor=255,223,223,223
        disableFontColor=255,70,70,70
        font=Face
        rect_textoffset=12,1,0,0
        textClickable=1
        iconsize=8,8
        lineheight=10
        sectioninterval=4
        section1=0,0,First
        section2=0,0,#Б
    "#,
    )?;
    std::fs::write(
        ui.join("flags.ini"),
        r#"
        [MAIN]
        item=100,CHECKBUTTON,Flags
        [Flags]
        position=200,30,220,40
        command=click
        exclusiveChoose=0
        individualpos=1
        icongroup=Icon
        normalpicture=Off
        selectpicture=On
        iconsize=8,8
        section1=1,0,#А
        section2=1,0,#Б
        pos1=30,0
        pos2=45,0
        "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r##"
        object ui;
        #event_handler("CheckButtonChange","Change");
        #event_handler("ISound","Sound");
        #event_handler("ievnt_command","Command");
        #event_handler("Select","Select");
        #event_handler("Disable","Disable");
        #event_handler("LockClicks","LockClicks");
        #event_handler("Edit","Edit");
        #event_handler("OpenFlags","OpenFlags");
        void Main() {
            MapControl(CreateControl("ILClick"),1);
            MapControl(CreateControl("IAction"),32);
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(4,&ui,0);
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\choices.ini");
            Trace("initial="+ui.Radio.state1+ui.Radio.state2);
        }
        void Change() {
            string node=GetEventData();
            int choice=GetEventData();
            int checked=GetEventData();
            aref flags;
            makearef(flags,ui.(node));
            Trace(node+":"+choice+":"+checked+":"+flags.state1+flags.state2);
        }
        void Sound() { Trace("sound"); }
        void Command() { string action=GetEventData(); Trace(action); }
        void Select() {
            Trace("before");
            SendMessage(&ui,"lslll",45045,"Radio",2,2,1);
            SendMessage(&ui,"lslll",45045,"Radio",2,2,0);
            int state=SendMessage(&ui,"lsll",45045,"Radio",3,2);
            int index=SendMessage(&ui,"lsl",45045,"Radio",4);
            Trace("after="+state+index);
        }
        void Disable() { SendMessage(&ui,"lslll",45045,"Radio",5,2,1); }
        void LockClicks() {
            SendMessage(&ui,"lslll",45045,"Radio",-1,1,0);
            int clickable=SendMessage(&ui,"lsll",45045,"Radio",-1,2);
            Trace("clickable="+clickable);
        }
        void Edit() {
            SendMessage(&ui,"lslsll",45045,"Radio",0,"#А",0,0);
            SendMessage(&ui,"lslls",45045,"Radio",1,3,"БА");
            SendMessage(&ui,"lsllllll",45045,"Radio",-1,4,24,50,110,100);
        }
        void OpenFlags() {
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\flags.ini");
        }
    "##,
    )?;
    let trace = Trace::default();
    let game = GameDirectory::open(&temp.0)?;
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host(&trace)?)?;
    engine.host.set_interface_window(800, 600);
    engine.frame(0)?;
    assert_eq!(trace.take(), ["script:initial=10"]);
    let draws = engine.host.interface_draws();
    let [Picture(first), Text(label), Picture(second), Text(_)] = draws.as_slice() else {
        panic!("two radio choices with labels");
    };
    let on_uv = [0.125, 0.125, 0.875, 0.375];
    let off_uv = [0.125, 0.625, 0.875, 0.875];
    assert_eq!(first.uv, on_uv);
    assert_eq!(second.uv, off_uv);
    assert_eq!(label.text, "АБ");
    assert_eq!(label.position, [32, 31]);
    assert_eq!(label.rgba, [223, 223, 223, 255]);

    let mut event = Message::new("", vec![])?;
    engine.emit_event("Select", &mut event)?;
    assert_eq!(
        trace.take(),
        ["script:before", "script:Radio:2:1:01", "script:after=11"]
    );
    let input = |engine: &mut Engine, code, pressed| -> Result<()> {
        engine.control_input(ControlInput::Button { code, pressed });
        engine.frame(16)?;
        Ok(())
    };
    input(&mut engine, 32, true)?;
    input(&mut engine, 32, false)?;
    assert_eq!(trace.take(), ["script:sound", "script:activate"]);

    engine.host.set_interface_pointer(Some([58.0, 34.0]));
    input(&mut engine, 1, true)?;
    input(&mut engine, 1, false)?;
    assert_eq!(
        trace.take(),
        ["script:sound", "script:Radio:1:1:10", "script:click"]
    );
    engine.emit_event("Disable", &mut event)?;
    engine.host.set_interface_pointer(Some([23.0, 47.0]));
    input(&mut engine, 1, true)?;
    input(&mut engine, 1, false)?;
    assert_eq!(trace.take(), ["script:sound", "script:click"]);

    engine.emit_event("LockClicks", &mut event)?;
    input(&mut engine, 1, true)?;
    input(&mut engine, 1, false)?;
    assert_eq!(trace.take(), ["script:clickable=0"]);

    engine.emit_event("Edit", &mut event)?;
    engine.frame(16)?;
    let draws = engine.host.interface_draws();
    let [
        Picture(first),
        Text(_),
        Picture(_),
        Text(disabled),
        Picture(third),
        Text(edited),
    ] = draws.as_slice()
    else {
        panic!("three radio choices with labels");
    };
    assert_eq!(first.rect, [24.0, 50.0, 32.0, 58.0]);
    assert_eq!(first.uv, on_uv);
    assert_eq!(disabled.rgba, [70, 70, 70, 255]);
    assert_eq!(third.rect, [24.0, 78.0, 32.0, 86.0]);
    assert_eq!(edited.text, "БА");
    assert!(trace.borrow().is_empty());

    engine.emit_event("OpenFlags", &mut event)?;
    engine.host.set_interface_pointer(Some([232.0, 32.0]));
    input(&mut engine, 1, true)?;
    input(&mut engine, 1, false)?;
    assert_eq!(
        trace.take(),
        ["script:sound", "script:Flags:1:0:01", "script:click"]
    );
    let draws = engine.host.interface_draws();
    let [Picture(first), Picture(second)] = draws.as_slice() else {
        panic!("two independently placed checkboxes");
    };
    assert_eq!(first.uv, off_uv);
    assert_eq!(second.uv, on_uv);
    Ok(())
}

#[test]
fn dialog_frames_preserve_original_panel_geometry_and_border_tint() -> Result<()> {
    use storm_render::interface_text::{InterfaceDraw::Picture, PictureColor};

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI/INTERFACES");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&ini)?;
    std::fs::create_dir_all(&textures)?;
    let mut bytes = [0u32, 64, 64, 1, 21, 16_384].map(u32::to_le_bytes).concat();
    bytes.extend([255; 16_384]);
    std::fs::write(textures.join("panels.tga.tx"), bytes)?;
    std::fs::write(
        ini.join("interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("pictures.ini"),
        r#"
        [Panels]
        sTextureName=panels.tga
        wTextureWidth=64
        wTextureHeight=64
        picture=Bar,0,0,32,4
        picture=Corner,32,4,40,12
        "#,
    )?;
    std::fs::write(
        ini.join("defaultnode.ini"),
        r#"
        [FRAME_METAL]
        class=FRAME
        backimage=piclist=Panels, piccutuv=(0.5,0.125), color=(200,128,100,90)
        groupName=Panels
        topline_pic=Bar
        lefttop_pic=Corner
        lefttop_size=7,9
        righttop_size=11,8
        leftbottom_size=13,14
        rightbottom_size=16,18
        "#,
    )?;
    std::fs::write(
        ini.join("frame.ini"),
        r#"
        [MAIN]
        item=FRAME_METAL,Bio
        item=FRAME,Flat
        [Bio]
        position=10,20,58,54
        captionheight=12
        captionimage=piclist=Panels, picname=Bar
        captionoffset=1,2,3,4
        captiondividerheight=3
        [Flat]
        position=10,80,58,104
        backimage=color=(99,14,75,110)
        "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("Recolor", "Recolor");
        void Main() {
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\frame.ini");
        }
        void Recolor() {
            SendMessage(&ui,"lsll",45045,"Bio",0,argb(123,45,67,89));
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(1280, 720);
    engine.frame(0)?;
    let draws = engine.host.interface_draws();
    let [
        Picture(background),
        Picture(caption),
        Picture(top),
        Picture(bottom),
        Picture(left),
        Picture(right),
        Picture(left_top),
        Picture(right_top),
        Picture(left_bottom),
        Picture(right_bottom),
        Picture(divider),
        Picture(flat),
    ] = draws.as_slice()
    else {
        panic!("expected frame images, border quads and caption divider");
    };
    assert_eq!(background.rect, [143.0, 20.0, 191.0, 54.0]);
    assert_eq!(background.uv, [0.5, 0.125, 1.0, 0.65625]);
    assert_eq!(background.rgba, [128, 100, 90, 200]);
    assert_eq!(caption.rect, [144.0, 22.0, 188.0, 30.0]);
    assert_eq!(
        caption.uv,
        [0.5 / 64.0, 0.5 / 64.0, 31.5 / 64.0, 3.5 / 64.0]
    );
    assert_eq!(top.rect, [150.0, 20.0, 180.0, 25.0]);
    assert_eq!(bottom.rect, [150.0, 49.0, 180.0, 54.0]);
    assert_eq!(left.rect, [143.0, 29.0, 148.0, 40.0]);
    assert_eq!(right.rect, [186.0, 28.0, 191.0, 36.0]);
    assert_eq!(left_top.rect, [143.0, 20.0, 150.0, 29.0]);
    assert_eq!(
        left_top.uv,
        [32.5 / 64.0, 4.5 / 64.0, 39.5 / 64.0, 11.5 / 64.0]
    );
    assert_eq!(right_top.rect, [180.0, 20.0, 191.0, 28.0]);
    assert_eq!(left_bottom.rect, [143.0, 40.0, 156.0, 54.0]);
    assert_eq!(right_bottom.rect, [175.0, 36.0, 191.0, 54.0]);
    assert_eq!(divider.rect, [148.0, 32.0, 186.0, 35.0]);
    assert_eq!(divider.uv, top.uv);
    assert_eq!(top.rgba, [255; 4]);
    assert_eq!(bottom.uv, [0.0, 0.0, 1.0, 1.0]);
    assert!(Rc::ptr_eq(&background.texture, &divider.texture));
    assert!(matches!(flat.color, PictureColor::AlphaMask));
    assert_eq!(flat.rgba, [14, 75, 110, 99]);

    engine.emit_event("Recolor", &mut Message::new("", vec![])?)?;
    engine.frame(16)?;
    let recolored = engine.host.interface_draws();
    assert_eq!(draws.len(), recolored.len());
    for (index, (original, updated)) in draws.iter().zip(&recolored).enumerate() {
        let (Picture(original), Picture(updated)) = (original, updated) else {
            panic!("expected the same pictures after recoloring");
        };
        assert_eq!(original.rect, updated.rect);
        assert_eq!(original.uv, updated.uv);
        let rgba = if (2..=10).contains(&index) {
            [45, 67, 89, 123]
        } else {
            original.rgba
        };
        assert_eq!(updated.rgba, rgba);
    }
    Ok(())
}

#[test]
fn formatted_interface_text_wraps_cyrillic_and_keeps_script_scroll_and_colors() -> Result<()> {
    use storm_render::interface_text::Alignment;

    let temp = Temporary::new()?;
    let ini = temp.0.join("RESOURCE/INI");
    let textures = temp.0.join("RESOURCE/Textures/Interfaces");
    std::fs::create_dir_all(&textures)?;
    std::fs::create_dir_all(ini.join("INTERFACES"))?;
    std::fs::create_dir_all(ini.join("texts/test"))?;
    let mut texture = [0u32, 4, 4, 1, 21, 64].map(u32::to_le_bytes).concat();
    texture.extend([255; 64]);
    std::fs::write(textures.join("glyph.tga.tx"), texture)?;
    std::fs::write(
        ini.join("fonts.ini"),
        "[Face]\nTexture=Interfaces/glyph.tga\nTexture_xsize=4\nTexture_ysize=4\nHeight=4\nSpacebar=2\nchar_А=0,0,4,4\nchar_Б=0,0,4,4\nchar_В=0,0,4,4\n",
    )?;
    std::fs::write(
        ini.join("texts/language.ini"),
        "[COMMON]\ndefaultLanguage=Test\nstrings=common.ini\n[DIRECTORY]\nTest=test\n",
    )?;
    std::fs::write(
        ini.join("texts/test/common.ini"),
        "string=Caption,\"АБВ Б\"\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/interfaces.ini"),
        "[PC_SCREEN]\nbDynamicScaling=1\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/defaultnode.ini"),
        "[FORMATEDTEXT]\nfont=Face\nlineSpace=10\nalignment=center\nColor=255,255,255,255\n",
    )?;
    std::fs::write(
        ini.join("INTERFACES/format.ini"),
        r#"
        [MAIN]
        item=FORMATEDTEXT,Main
        item=100,FORMATEDTEXT,Single
        [Main]
        position=5,10,17,40
        [Single]
        position=20,12,28,52
        string=Caption
        valignment=1
        wrapoff=1
        alignment=right
        "#,
    )?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object ui;
        #event_handler("Scroll","Scroll");
        #event_handler("Append","Append");
        #event_handler("FitCount","FitCount");
        #event_handler("LastY","LastY");
        #event_handler("Clear","Clear");
        int Append() {
            string extra="АБ АБ";
            return sti(SendMessage(&ui,"lsle",45045,"Main",0,&extra));
        }
        int FitCount() {
            string extra="АБ АБ АБ";
            return sti(SendMessage(&ui,"lsle",45045,"Main",9,&extra));
        }
        int LastY() { return sti(SendMessage(&ui,"lsll",45045,"Main",12,1)); }
        void Scroll() {
            SendMessage(&ui,"lslf",45045,"Main",2,0.75);
            SendMessage(&ui,"lslll",45045,"Main",8,0,argb(220,10,20,30));
        }
        void Clear() { SendMessage(&ui,"lss",45038,"Main",""); }
        void Main() {
            CreateEntity(&ui,"XINTERFACE");
            LayerAddObject(5,&ui,0);
            SendMessage(&ui,"ls",45010,"resource\ini\interfaces\format.ini");
            SendMessage(&ui,"lss",45038,"Main","АБ АБ АБ\n<color=200,220,30,40>В</color> Б");
        }
        "#,
    )?;
    let game = GameDirectory::open(&temp.0)?;
    let mut engine =
        Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, NativeHost::new()?)?;
    engine.host.set_interface_window(800, 600);
    engine.frame(0)?;
    let draws = engine.host.interface_text_draws();
    assert_eq!(
        draws
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["АБ", "АБ", "АБ", "АБ"]
    );
    assert_eq!(draws[0].position, [11, 10]);
    assert_eq!(draws[2].position, [11, 30]);
    assert_eq!(draws[3].position, [28, 27]);
    assert!(draws[3].alignment == Alignment::Right);

    let mut args = Message::new("", vec![])?;
    assert_eq!(
        engine.emit_event("FitCount", &mut args)?,
        Some(Value::Int(3))
    );
    engine.emit_event("Scroll", &mut args)?;
    engine.frame(16)?;
    let draws = engine.host.interface_text_draws();
    assert_eq!(draws.len(), 3);
    assert_eq!(
        (&draws[0].text[..], draws[0].position, draws[0].rgba),
        ("В", [6, 10], [220, 30, 40, 200])
    );
    assert_eq!(
        (&draws[1].text[..], draws[1].position, draws[1].rgba),
        (" Б", [10, 10], [10, 20, 30, 220])
    );

    assert_eq!(engine.emit_event("Append", &mut args)?, Some(Value::Int(2)));
    assert_eq!(engine.emit_event("LastY", &mut args)?, Some(Value::Int(20)));
    engine.frame(16)?;
    let draws = engine.host.interface_text_draws();
    assert_eq!(draws[2].text, "АБ");
    assert_eq!(draws[3].text, "АБ");
    engine.emit_event("Clear", &mut args)?;
    engine.frame(16)?;
    assert_eq!(engine.host.interface_text_draws().len(), 1);
    Ok(())
}
