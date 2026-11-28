use super::*;
use crate::{ScriptEncoding, Startup};
use std::{
    num::{NonZeroU16, NonZeroU32},
    path::PathBuf,
    time::{Duration, Instant},
};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn configured_sound_routes_script_commands_through_the_stereo_mixer() -> Result<()> {
    let temp =
        Scratch(std::env::temp_dir().join(format!("storm-sound-test-{}", std::process::id())));
    std::fs::create_dir(&temp.0)?;
    std::fs::create_dir_all(temp.0.join("RESOURCE/Sounds/Music"))?;
    std::fs::create_dir_all(temp.0.join("RESOURCE/INI/Aliases"))?;
    std::fs::write(temp.0.join("engine.ini"), "run=main.c\n")?;
    std::fs::write(
        temp.0.join("main.c"),
        r#"
        object Sound;
        int id;
        #event_handler("scheme","Scheme");
        #event_handler("reset","Reset");
        #event_handler("stop_one","StopOne");
        #event_handler("stop_all","StopAll");
        #event_handler("thunder","Thunder");
        void Main() {
            CreateEntity(&Sound,"SOUND");
            id = SendMessage(&Sound,"lsllll",77003,"music\tone.wav",4,1,0,1);
        }
        void Scheme() { SendMessage(&Sound,"ls",77014,"test"); }
        void Reset() { SendMessage(&Sound,"l",77013); }
        void StopOne() { SendMessage(&Sound,"lll",77004,id,20); }
        void StopAll() { SendMessage(&Sound,"lll",77004,0,0); }
        void Thunder() {
            SendMessage(&Sound,"lsllllllfff",77003,"thunder",1,1,0,0,0,0,1000.0,590.0,1200.0);
        }
        "#,
    )?;
    std::fs::write(
        temp.0.join("RESOURCE/INI/sound_scheme.ini"),
        "[test]\nch_loop=tone_alias\n",
    )?;
    std::fs::write(
        temp.0.join("RESOURCE/INI/Aliases/test.ini"),
        "[tone_alias]\nname=music\\tone.wav\n",
    )?;
    std::fs::write(
        temp.0.join("RESOURCE/INI/Aliases/sound_alias.ini"),
        "[thunder]\nname=music\\tone.wav\n",
    )?;
    let data_size = 4800u32 * 4;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data_size).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes()); // PCM
    wav.extend(2u16.to_le_bytes());
    wav.extend(48000u32.to_le_bytes());
    wav.extend(192000u32.to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_size.to_le_bytes());
    for _ in 0..data_size / 2 {
        wav.extend(16384i16.to_le_bytes());
    }
    std::fs::write(temp.0.join("RESOURCE/Sounds/Music/Tone.WAV"), wav)?;

    let game = GameDirectory::open(&temp.0)?;
    let mut host = NativeHost::with_random_seed(0)?;
    let sound = Sound::register(&mut host, game.clone())?.unwrap();
    let (mixer, mut samples) =
        rodio::mixer::mixer(NonZeroU16::new(2).unwrap(), NonZeroU32::new(48000).unwrap());
    sound.0.borrow_mut().output = Output::Test(mixer);
    let mut engine = Startup::load(&game)?.engine(&game, ScriptEncoding::Utf8, host)?;
    engine.register_service(sound.clone())?;
    engine.frame(0)?;
    let id = engine.host.entities.first_id("sound").unwrap();
    assert!(
        engine
            .host
            .entities
            .layer_ids(storm_core::LayerType::Realize)
            .contains(&id)
    );
    let count = || sound.0.borrow().voices.iter().flatten().count();
    assert_eq!(count(), 1);
    assert_eq!(samples.next(), Some(0.25));
    let mut message = Message::new("", vec![])?;
    engine.emit_event("scheme", &mut message)?;
    engine.emit_event("reset", &mut message)?;
    engine.frame(0)?;
    assert_eq!(count(), 1);
    engine.emit_event("scheme", &mut message)?;
    engine.frame(0)?;
    assert_eq!(count(), 2);
    assert!(samples.by_ref().take(1000).any(|value| value == 0.5));
    engine.emit_event("reset", &mut message)?;
    engine.emit_event("stop_one", &mut message)?;
    engine.frame(10)?;
    assert_eq!(count(), 2);
    engine.frame(10)?;
    assert_eq!(count(), 1);
    assert!(samples.by_ref().take(1000).any(|value| value == 0.25));
    engine.emit_event("stop_all", &mut message)?;
    engine.frame(0)?;
    assert_eq!(count(), 0);
    // The audio callback observes stopped players on its next control tick.
    assert!(
        samples
            .by_ref()
            .skip(500)
            .take(500)
            .all(|value| value == 0.0)
    );
    engine.emit_event("thunder", &mut message)?;
    assert_eq!(count(), 1);
    // The callback keeps polling when the queue is empty at a channel boundary.
    let thunder_peak = (0..9600)
        .filter_map(|_| samples.next())
        .fold(0.0_f32, f32::max);
    assert_eq!(thunder_peak, 0.25);
    engine.shutdown()?;
    Ok(())
}

#[test]
#[ignore = "requires original sound resources and a desktop audio device"]
fn original_ogg_reaches_the_desktop_audio_device() -> Result<()> {
    let root = std::env::var_os("STORM_AUDIO_GAME_DIR").context("set STORM_AUDIO_GAME_DIR")?;
    let game = GameDirectory::open(root)?;
    let mut sound = State::new(game, Rc::new(RefCell::new(Random::new(0))));
    let id = sound.try_play(
        "music\\military.ogg",
        Playback {
            kind: STREAMED_STEREO,
            category: 2,
            paused: false,
            looped: false,
            fade_ms: 0,
            volume: 0.12,
        },
    )?;
    assert_eq!(id, 1);
    let player = &sound.voices[0].as_ref().unwrap().player;
    let start = Instant::now();
    while player.get_pos() < Duration::from_millis(80) && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(player.get_pos() >= Duration::from_millis(80));
    sound.stop(id, 0);
    assert!(sound.voices[0].is_none());
    Ok(())
}
