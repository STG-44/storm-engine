use super::*;
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
