use super::tests::Trace;
use super::*;
use serde_json::{Value as Json, json};
use storm_core::Value as Field;

fn empty() -> Result<Message> {
    Message::new("", vec![])
}

#[test]
fn retained_clear_routing_and_table_states_match_including_the_bounded_hang() -> Result<()> {
    let reference: Json =
        serde_json::from_str(include_str!("../../reference/clear_events/cases.json"))?;
    let mut snapshots = 0;
    let mut hangs = 0;
    for case in reference["cases"].as_array().unwrap() {
        let mut handlers = Handlers::default();
        for (index, op) in case["operations"].as_array().unwrap().iter().enumerate() {
            match op[0].as_str().unwrap() {
                "add" => handlers.add(
                    op[1].as_str().unwrap(),
                    &op[2].to_string(),
                    op[3].as_i64().unwrap() as i32,
                    op[4].as_bool().unwrap(),
                )?,
                "delete" => handlers.delete(op[1].as_str().unwrap(), &op[2].to_string())?,
                "frame" => handlers.frame()?,
                "clear"
                    if case["nonterminating_clear"] == true
                        && index == case["snapshots"].as_array().unwrap().len() =>
                {
                    assert!(
                        format!("{:#}", handlers.clear().unwrap_err())
                            .contains("would not terminate")
                    );
                    hangs += 1;
                    continue;
                }
                "clear" => handlers.clear()?,
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
                                },
                                h.is_static
                            ])
                        })
                        .collect::<Vec<_>>()
                })
            });
            assert_eq!(
                json!(actual),
                case["snapshots"][index],
                "snapshot {snapshots}, {op}"
            );
            snapshots += 1;
        }
    }
    assert_eq!(reference["cases"].as_array().unwrap().len(), 25);
    assert_eq!(snapshots, 627);
    assert_eq!(hangs, 1);
    Ok(())
}

#[test]
fn public_and_script_clear_keep_globals_static_status_and_runtime_isolation() -> Result<()> {
    let source = r#"
        #event_handler("Keep", "Kept")
        #event_handler("Disabled", "Disabled")
        int state=5;
        int Kept(){state++;return state;}
        void Disabled(){state+=10;}
        void Dynamic(){state+=100;}
        void Init(){SetEventHandler("Keep","Dynamic",0);SetEventHandler("New","Dynamic",1);}
        void Off(){DelEventHandler("Disabled","Disabled");}
        void Clear(){ClearEvents();}
        int Read(){return state;}
    "#;
    let mut runtime = Runtime::compile(source)?;
    runtime.clear_events()?;
    assert!(runtime.state.is_none());
    runtime.call("Init", &[], &mut (), Limits::default())?;
    runtime.call("Off", &[], &mut (), Limits::default())?;
    runtime.clear_events()?;
    assert_eq!(
        runtime.emit("Keep", &mut empty()?, &mut (), Limits::default())?,
        Some(Value::Int(6))
    );
    for name in ["Disabled", "New"] {
        assert_eq!(
            runtime.emit(name, &mut empty()?, &mut (), Limits::default())?,
            None
        );
    }
    runtime.call("Clear", &[], &mut (), Limits::default())?;
    assert_eq!(
        runtime.call("Read", &[], &mut (), Limits::default())?,
        Value::Int(6)
    );
    assert_eq!(
        runtime.emit("Keep", &mut empty()?, &mut (), Limits::default())?,
        Some(Value::Int(7))
    );
    let mut other = Runtime::compile(source)?;
    assert_eq!(
        other.call("Read", &[], &mut (), Limits::default())?,
        Value::Int(5)
    );
    Ok(())
}

#[test]
fn clear_does_not_cancel_prepare_or_mutate_posted_payloads_and_timers() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        #event_handler("E", "H")
        int last;
        void H(){last=GetEventData();}
        void Dynamic(){last=999;}
        void Init(){PostEvent("E",10,"l",7);SetEventHandler("E","Dynamic",0);}
        void Clear(){ClearEvents();}
        int Read(){return last;}
    "#,
    )?;
    runtime.call("Init", &[], &mut (), Limits::default())?;
    let snapshot = |runtime: &Runtime| {
        runtime
            .handlers
            .queue
            .entries
            .iter()
            .map(|q| {
                let m = q.message.as_ref().unwrap();
                let [Field::Long(value)] = m.values() else {
                    panic!("test payload")
                };
                json!([
                    q.name,
                    q.elapsed,
                    q.period,
                    q.processed,
                    q.invalid,
                    m.format(),
                    value
                ])
            })
            .collect::<Vec<_>>()
    };
    let before = snapshot(&runtime);
    runtime.call("Clear", &[], &mut (), Limits::default())?;
    assert_eq!(snapshot(&runtime), before);
    assert_eq!(
        (
            runtime.posted_events(),
            runtime.pending_events(),
            runtime.retained_post_events()
        ),
        (1, 1, 1)
    );
    assert_eq!(runtime.advance_frame(9, &mut (), Limits::default())?, 0);
    assert_eq!(runtime.advance_frame(1, &mut (), Limits::default())?, 1);
    assert_eq!(
        runtime.call("Read", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert_eq!(
        (
            runtime.completed_handlers(),
            runtime.pending_events(),
            runtime.retained_post_events()
        ),
        (1, 0, 0)
    );
    Ok(())
}

#[test]
fn clearing_from_a_running_handler_keeps_its_payload_and_reads_the_live_table() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        #event_handler("E", "First")
        #event_handler("E", "Last")
        native void Record(int n);
        void Init(){SetEventHandler("E","Dynamic",0);}
        void First(){int x=GetEventData();ClearEvents();int y=GetEventData();Record(x+y);}
        void Last(){int x=GetEventData();Record(x);}
        void Dynamic(){Record(999);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    let mut message = Message::new("ll", vec![Field::Long(11), Field::Long(22)])?;
    runtime.emit("E", &mut message, &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Int(33)], vec![Value::Int(11)]]);
    assert_eq!(runtime.completed_handlers(), 2);
    assert_eq!(message.long()?, 22);
    Ok(())
}

#[test]
fn nested_clear_preserves_the_shared_break_flag_and_inherited_message_cursor() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        #event_handler("Outer","A")
        #event_handler("Outer","B")
        #event_handler("Inner","I")
        native void Record(int n);
        void A(){Event("Inner");int x=GetEventData();Record(x);}
        void B(){Record(999);}
        void I(){int x=GetEventData();EventsBreak();ClearEvents();Record(x);}
    "#,
    )?;
    let mut host = Trace::default();
    for _ in 0..2 {
        let mut message = Message::new("ll", vec![Field::Long(7), Field::Long(9)])?;
        runtime.emit("Outer", &mut message, &mut host, Limits::default())?;
    }
    assert_eq!(
        host.0,
        vec![
            vec![Value::Int(7)],
            vec![Value::Int(9)],
            vec![Value::Int(7)],
            vec![Value::Int(9)]
        ]
    );
    assert_eq!(runtime.completed_handlers(), 4);
    Ok(())
}

#[test]
fn clear_time_hang_is_located_and_recoverable_without_discarding_queued_events() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        #event_handler("E","A")
        #event_handler("E","B")
        void A(){}
        void B(){}
        void Bad(){DelEventHandler("E","B");PostEvent("Later",100,"l",7);ClearEvents();}
        void Recover(){SetEventHandler("E","B",1);}
        void Clear(){ClearEvents();}
    "#,
    )?;
    let error = runtime
        .call("Bad", &[], &mut (), Limits::default())
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("bad <source>:") && error.contains("would not terminate"),
        "{error}"
    );
    assert_eq!(runtime.pending_events(), 1);
    assert!(!runtime.handlers.queue.entries[0].processed);
    runtime.call("Recover", &[], &mut (), Limits::default())?;
    runtime.clear_events()?;
    runtime.emit("E", &mut empty()?, &mut (), Limits::default())?;
    assert_eq!(runtime.completed_handlers(), 2);
    let error = runtime
        .call(
            "Clear",
            &[],
            &mut (),
            Limits {
                instructions: 0,
                ..Limits::default()
            },
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("instruction budget"));
    runtime.call("Clear", &[], &mut (), Limits::default())?;
    assert_eq!(runtime.pending_events(), 1);
    Ok(())
}

#[test]
fn clearing_reclaims_dynamic_slots_but_retains_event_names_and_original_skipped_entries()
-> Result<()> {
    let mut handlers = Handlers::default();
    for n in 0..MAX_HANDLERS {
        handlers.register("E", &format!("H{n}"), 0)?;
    }
    handlers.clear()?;
    assert_eq!(handlers.entries["e"].len(), 1);
    assert!(handlers.entries["e"][0].status == Status::Deleted);
    for n in 0..MAX_HANDLERS - 1 {
        handlers.register("E", &format!("New{n}"), 0)?;
    }
    assert!(handlers.register("E", "Overflow", 0).is_err());
    let mut handlers = Handlers::default();
    for n in 0..MAX_EVENTS {
        handlers.register(&format!("E{n}"), "H", 0)?;
    }
    handlers.clear()?;
    assert!(handlers.entries.values().all(Vec::is_empty));
    assert!(handlers.register("Overflow", "H", 0).is_err());
    handlers.register("E0", "H", 0)?;
    Ok(())
}

#[test]
fn clear_is_a_case_insensitive_zero_argument_void_builtin_with_no_host_call() -> Result<()> {
    for source in ["void Main(){ClearEvents(1);}", "void ClearEvents(){}"] {
        assert!(Program::compile(source).is_err());
    }
    let program = Program::compile("int Main(){cLeArEvEnTs();return 7;}")?;
    assert_eq!(
        program.call("Main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert_eq!(
        program.call("ClearEvents", &[], &mut (), Limits::default())?,
        Value::Void
    );
    Ok(())
}

#[test]
fn clearing_ship_fixture_keeps_the_static_commit_and_complete_previous_report() -> Result<()> {
    fn run(entry: &str) -> Result<Json> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/static_ship");
        let mut runtime =
            Runtime::compile_with_loader("MAIN.C", &mut crate::DirectorySource::new(root)?)?;
        let mut host = crate::preview::AttributeHost::new()?;
        let result = runtime.call(entry, &[], &mut host, Limits::default())?;
        assert_eq!(host.report(&result)["ship_attribute_notifications"], 0);
        assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 0);
        assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 1);
        assert_eq!(
            (
                runtime.completed_handlers(),
                runtime.posted_events(),
                runtime.pending_events()
            ),
            (3, 1, 0)
        );
        Ok(host.report(&result))
    }
    assert_eq!(run("ClearMain")?, run("Main")?);
    Ok(())
}
