use super::*;

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
    std::fs::write(
        texts.join("language.ini"),
        "[COMMON]\ndefaultLanguage=Russian\n[DIRECTORY]\nRussian=Russian\n",
    )?;
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
