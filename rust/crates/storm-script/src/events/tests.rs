use super::*;
use storm_core::Value as Field;

#[derive(Default)]
pub(super) struct Trace(pub(super) Vec<Vec<Value>>);
impl Host for Trace {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        ensure!(name == "record", "unexpected test native");
        self.0.push(arguments.to_vec());
        Ok(Value::Void)
    }
}
fn empty() -> Result<Message> {
    Message::new("", vec![])
}

#[test]
fn retained_event_table_status_snapshots_match() -> Result<()> {
    use serde_json::{Value as Json, json};
    let fixture: Json = serde_json::from_str(include_str!("../../reference/events/cases.json"))?;
    let mut snapshots = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut handlers = Handlers::default();
        for (operation, expected) in case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
        {
            match operation[0].as_str().unwrap() {
                "add" => handlers.register(
                    operation[1].as_str().unwrap(),
                    &operation[2].to_string(),
                    operation[3].as_i64().unwrap() as i32,
                )?,
                "delete" => {
                    handlers.delete(operation[1].as_str().unwrap(), &operation[2].to_string())?
                }
                "frame" => handlers.frame(),
                _ => unreachable!(),
            }
            let actual = ["e", "other"].map(|name| {
                handlers.entries.get(name).map(|entries| {
                    entries
                        .iter()
                        .map(|h| {
                            json!([
                                h.function.parse::<u32>().unwrap(),
                                match h.status {
                                    Status::New => 0,
                                    Status::Normal => 1,
                                    Status::Deleted => 2,
                                }
                            ])
                        })
                        .collect::<Vec<_>>()
                })
            });
            assert_eq!(
                json!(actual),
                *expected,
                "operation {operation}, snapshot {snapshots}"
            );
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 521);
    Ok(())
}

#[test]
fn rewinds_payload_per_handler_deduplicates_case_and_keeps_last_nonvoid() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n, float x, string s);
        void Init() {
            SetEventHandler("Hit", "First", 0);
            SetEventHandler("hit", "VoidHandler", 0);
            SetEventHandler("HIT", "FIRST", 1);
            SetEventHandler("Hit", "Last", 0);
            SetEventHandler("Hit", "VoidHandler2", 0);
        }
        int First() { int n=GetEventData(); float x=GetEventData(); string s=GetEventData(); Record(n,x,s); return n; }
        void VoidHandler() { int n=GetEventData(); float x=GetEventData(); string s=GetEventData(); Record(n,x,s); }
        string Last() { int n=GetEventData(); float x=GetEventData(); string s=GetEventData(); return s; }
        void VoidHandler2() {}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    let mut message = Message::new(
        "lfs",
        vec![
            Field::Long(19),
            Field::Float(-1.25),
            Field::String("rey_a2".into()),
        ],
    )?;
    assert_eq!(
        runtime.emit("hIt", &mut message, &mut host, Limits::default())?,
        Some(Value::String("rey_a2".into()))
    );
    assert_eq!(runtime.completed_handlers(), 4);
    assert_eq!(
        host.0,
        vec![
            vec![
                Value::Int(19),
                Value::Float(-1.25),
                Value::String("rey_a2".into())
            ];
            2
        ]
    );
    assert_eq!(
        runtime.emit("unknown", &mut message, &mut host, Limits::default())?,
        None
    );
    Ok(())
}

#[test]
fn live_handler_mutation_and_frame_activation_follow_original_table() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Init() { SetEventHandler("E","A",0); SetEventHandler("E","B",1); }
        void A() { Record(1); DelEventHandler("E","B"); SetEventHandler("E","C",0); }
        void B() { Record(2); }
        void C() { Record(3); }
        void RestoreB() { SetEventHandler("E","B",1); DelEventHandler("E","A"); }
        void Pending() { SetEventHandler("Later","B",1); }
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    runtime.emit("E", &mut empty()?, &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Int(1)], vec![Value::Int(3)]]);
    runtime.call("RestoreB", &[], &mut host, Limits::default())?;
    host.0.clear();
    runtime.emit("E", &mut empty()?, &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Int(2)], vec![Value::Int(3)]]);
    runtime.call("Pending", &[], &mut host, Limits::default())?;
    assert_eq!(
        runtime.emit("Later", &mut empty()?, &mut host, Limits::default())?,
        None
    );
    host.0.clear();
    runtime.process_frame();
    runtime.emit("Later", &mut empty()?, &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Int(2)]]);
    Ok(())
}

#[test]
fn events_break_finishes_current_handler_and_resets_next_dispatch() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Init() { SetEventHandler("E","A",0); SetEventHandler("E","B",0); }
        int A() { Record(1); EventsBreak(); Record(2); return 7; }
        void B() { Record(3); }
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    for _ in 0..2 {
        assert_eq!(
            runtime.emit("E", &mut empty()?, &mut host, Limits::default())?,
            Some(Value::Int(7))
        );
    }
    assert_eq!(
        host.0,
        vec![
            vec![Value::Int(1)],
            vec![Value::Int(2)],
            vec![Value::Int(1)],
            vec![Value::Int(2)]
        ]
    );
    Ok(())
}

#[test]
fn failures_are_located_and_next_dispatch_has_fresh_message_state() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        void Init() { SetEventHandler("E","Read",0); }
        int Read() { int n=GetEventData(); int m=GetEventData(); return n+m; }
    "#,
    )?;
    runtime.call("Init", &[], &mut (), Limits::default())?;
    let error = runtime
        .emit(
            "E",
            &mut Message::new("l", vec![Field::Long(1)])?,
            &mut (),
            Limits::default(),
        )
        .unwrap_err();
    let text = format!("{error:#}");
    assert!(
        text.contains("event e, handler read")
            && text.contains("read 3:")
            && text.contains("No (more) data"),
        "{text}"
    );
    assert_eq!(
        runtime.emit(
            "E",
            &mut Message::new("ll", vec![Field::Long(4), Field::Long(5)])?,
            &mut (),
            Limits::default()
        )?,
        Some(Value::Int(9))
    );
    assert!(
        runtime
            .call("Read", &[], &mut (), Limits::default())
            .is_err()
    );
    assert_eq!(runtime.completed_handlers(), 1);
    Ok(())
}

#[test]
fn shared_dispatch_budget_prevents_handler_fanout_from_resetting_fuel() -> Result<()> {
    let mut source = String::from("void Init(){");
    for n in 0..40 {
        source.push_str(&format!("SetEventHandler(\"E\",\"H{n}\",0);"));
    }
    source.push('}');
    for n in 0..40 {
        source.push_str(&format!("void H{n}(){{ int n=1; }}"));
    }
    let mut runtime = Runtime::compile(&source)?;
    runtime.call("Init", &[], &mut (), Limits::default())?;
    let result = runtime.emit(
        "E",
        &mut empty()?,
        &mut (),
        Limits {
            instructions: 50,
            ..Limits::default()
        },
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("instruction budget"));
    assert!(runtime.completed_handlers() > 0 && runtime.completed_handlers() < 40);
    runtime.emit("E", &mut empty()?, &mut (), Limits::default())?;
    Ok(())
}

#[test]
fn invalid_payload_types_and_unbounded_messages_are_rejected() -> Result<()> {
    let mut runtime = Runtime::compile(
        "void Init(){SetEventHandler(\"E\",\"H\",0);} int H(){int n=GetEventData();return n;}",
    )?;
    runtime.call("Init", &[], &mut (), Limits::default())?;
    for (format, value) in [
        ("b", Field::Byte(1)),
        ("u", Field::Dword(1)),
        ("d", Field::Double(1.0)),
        ("c", Field::Vector([0.; 3])),
        ("i", Field::Entity(storm_core::EntityId::INVALID)),
    ] {
        let error = runtime
            .emit(
                "E",
                &mut Message::new(format, vec![value])?,
                &mut (),
                Limits::default(),
            )
            .unwrap_err();
        assert!(format!("{error:#}").contains("unsupported GetEventData type"));
    }
    assert!(
        runtime
            .emit(
                "E",
                &mut Message::new(&"l".repeat(129), vec![Field::Long(1); 129])?,
                &mut (),
                Limits::default()
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn registration_validation_and_resource_caps_leave_runtime_usable() -> Result<()> {
    let mut source =
        String::from("void Add(string e,string h){SetEventHandler(e,h,0);} void Bad(int n){}");
    for n in 0..64 {
        source.push_str(&format!("void H{n}(){{}}"));
    }
    let mut runtime = Runtime::compile(&source)?;
    for (event, function) in [("", "H0"), ("E", "Missing"), ("E", "Bad")] {
        assert!(
            runtime
                .call(
                    "Add",
                    &[Value::String(event.into()), Value::String(function.into())],
                    &mut (),
                    Limits::default()
                )
                .is_err()
        );
    }
    for e in 0..8 {
        for h in 0..64 {
            runtime.call(
                "Add",
                &[
                    Value::String(format!("E{e}")),
                    Value::String(format!("H{h}")),
                ],
                &mut (),
                Limits::default(),
            )?;
        }
    }
    let error = runtime
        .call(
            "Add",
            &[Value::String("Overflow".into()), Value::String("H0".into())],
            &mut (),
            Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("handler budget"));
    runtime.emit("E0", &mut empty()?, &mut (), Limits::default())?;
    assert_eq!(runtime.completed_handlers(), 64);
    for source in [
        "void SetEventHandler(){}",
        "void F(){GetEventData(1);}",
        "void F(){SetEventHandler(1,2);}",
    ] {
        assert!(Runtime::compile(source).is_err());
    }
    Ok(())
}
