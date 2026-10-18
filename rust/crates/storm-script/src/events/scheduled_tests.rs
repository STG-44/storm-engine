use super::tests::Trace;
use super::*;
use storm_core::Value as Field;

fn message(values: &[i32]) -> Result<Message> {
    Message::new(
        &"l".repeat(values.len()),
        values.iter().copied().map(Field::Long).collect(),
    )
}

#[test]
fn retained_post_queue_and_compiler_frame_loops_match() -> Result<()> {
    use serde_json::{Value as Json, json};
    let fixture: Json =
        serde_json::from_str(include_str!("../../reference/post_events/cases.json"))?;
    let mut snapshots = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut queue = queue::Queue::default();
        let mut hooks = BTreeMap::<String, Json>::new();
        for (operation, expected) in case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
        {
            let mut fired = Vec::new();
            let apply = |queue: &mut queue::Queue, operation: &Json| -> Result<()> {
                match operation[0].as_str().unwrap() {
                    "post" => {
                        let id = operation[3].as_i64().unwrap() as i32;
                        queue.post(
                            operation[1].as_str().unwrap(),
                            operation[2].as_i64().unwrap() as i32,
                            if id < 0 { None } else { Some(message(&[id])?) },
                        )?;
                    }
                    "delete" => queue.cancel(operation[1].as_str().unwrap()),
                    "clear" => queue.invalidate_all(),
                    _ => unreachable!(),
                }
                Ok(())
            };
            match operation[0].as_str().unwrap() {
                "post" | "delete" | "clear" => apply(&mut queue, operation)?,
                "hook" => {
                    hooks.insert(
                        operation[1].as_str().unwrap().to_ascii_lowercase(),
                        Json::Array(operation.as_array().unwrap()[2..].to_vec()),
                    );
                }
                "frame" => {
                    let mut index = 0;
                    while index < queue.entries.len() {
                        if let Some((name, message)) =
                            queue.ready(index, operation[1].as_u64().unwrap() as u32)
                        {
                            let id = message.as_ref().map_or(-1, |m| match m.values()[0] {
                                Field::Long(n) => n,
                                _ => unreachable!(),
                            });
                            fired.push(json!([name, id]));
                            if let Some(hook) = hooks.remove(&name.to_ascii_lowercase()) {
                                apply(&mut queue, &hook)?;
                            }
                        }
                        index += 1;
                    }
                    queue.cleanup();
                }
                _ => unreachable!(),
            }
            let entries = queue
                .entries
                .iter()
                .map(|e| {
                    let id = e.message.as_ref().map_or(-1, |m| match m.values()[0] {
                        Field::Long(n) => n,
                        _ => unreachable!(),
                    });
                    json!([
                        e.name,
                        e.elapsed,
                        e.period,
                        u8::from(e.processed),
                        u8::from(e.invalid),
                        id
                    ])
                })
                .collect::<Vec<_>>();
            assert_eq!(
                json!({"fired":fired,"queue":entries}),
                *expected,
                "snapshot {snapshots}, operation {operation}"
            );
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 431);
    Ok(())
}

#[test]
fn timed_source_fixture_preserves_masks_and_commits_after_the_expected_frames() -> Result<()> {
    let mut runtime = Runtime::compile(include_str!("../../fixtures/timed_events.c"))?;
    let mut host = crate::preview::AttributeHost::new()?;
    let result = runtime.call("Main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(6));
    assert_eq!(runtime.completed_handlers(), 2);
    assert_eq!(host.report(&result)["ship_attribute_notifications"], 2);
    for (dt, expected) in [(0, 0), (9, 0), (1, 1), (0, 1)] {
        assert_eq!(
            runtime.advance_frame(dt, &mut host, Limits::default())?,
            expected
        );
    }
    assert_eq!(
        (
            runtime.completed_handlers(),
            runtime.posted_events(),
            runtime.pending_events(),
            runtime.retained_post_events()
        ),
        (4, 2, 0, 0)
    );
    let report = host.report(&result);
    assert_eq!(report["ship_attribute_notifications"], 4);
    assert_eq!(
        report["ship_attribute_changes"],
        serde_json::json!([
            {"name":"hd","value":"274"},{"name":"hd","value":"548"},
            {"name":"total","value":"6"},{"name":"committed","value":"1"}
        ])
    );
    Ok(())
}

#[test]
fn nested_message_free_event_inherits_cursor_and_payload_event_clears_parent() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Init(){SetEventHandler("Outer","Outer",0);SetEventHandler("Inner","Inner",0);}
        int Outer(){int first=GetEventData();Event("Inner");int second=GetEventData();return first+second;}
        void Inner(){int first=GetEventData();Record(first);}
        void ClearParent(){int first=GetEventData();Event("Inner","l",7);int second=GetEventData();}
        void Swap(){DelEventHandler("Outer","Outer");SetEventHandler("Outer","ClearParent",0);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    assert_eq!(
        runtime.emit(
            "Outer",
            &mut message(&[10, 20])?,
            &mut host,
            Limits::default()
        )?,
        Some(Value::Int(30))
    );
    assert_eq!(host.0, vec![vec![Value::Int(10)]]);
    runtime.call("Swap", &[], &mut host, Limits::default())?;
    let error = runtime
        .emit(
            "Outer",
            &mut message(&[10, 20])?,
            &mut host,
            Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("No data on this event"));
    assert_eq!(host.0[1], vec![Value::Int(7)]);
    Ok(())
}

#[test]
fn nested_break_flag_propagates_and_nested_entry_can_reset_it() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Init(){SetEventHandler("E","A",0);SetEventHandler("E","B",0);SetEventHandler("Inner","Inner",0);}
        void A(){Record(1);Event("Inner");Record(2);}
        void B(){Record(99);}
        void Inner(){EventsBreak();}
        void Reset(){EventsBreak();Event("Missing");Record(3);}
        void Swap(){DelEventHandler("E","A");SetEventHandler("E","Reset",0);SetEventHandler("E","Last",0);}
        void Last(){Record(4);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Init", &[], &mut host, Limits::default())?;
    runtime.emit("E", &mut message(&[])?, &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Int(1)], vec![Value::Int(2)]]);
    host.0.clear();
    runtime.call("Swap", &[], &mut host, Limits::default())?;
    runtime.emit("E", &mut message(&[])?, &mut host, Limits::default())?;
    assert_eq!(
        host.0,
        vec![
            vec![Value::Int(99)],
            vec![Value::Int(3)],
            vec![Value::Int(4)]
        ]
    );
    Ok(())
}

#[test]
fn message_builder_preserves_strict_fields_float_ref_conversion_and_extra_arguments() -> Result<()>
{
    let mut runtime = Runtime::compile(
        r#"
        native void Record(float n);
        void H(){float n=GetEventData();Record(n);}
        void Main(){SetEventHandler("E","H",0);int n=3;ref r;makeref(r,n);Event("E","f",r,99);Record(n);}
        void BadString(){string s="x";ref r;makeref(r,s);Event("E","s",r);}
        void BadLong(){Event("E","l",3.0);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Main", &[], &mut host, Limits::default())?;
    assert_eq!(host.0, vec![vec![Value::Float(3.)]; 2]);
    for name in ["BadString", "BadLong"] {
        assert!(
            runtime
                .call(name, &[], &mut host, Limits::default())
                .is_err()
        );
    }
    for args in [
        vec![Value::String("E".into()), Value::String("l".into())],
        vec![
            Value::String("E".into()),
            Value::String("i".into()),
            Value::Int(1),
        ],
    ] {
        assert!(
            runtime
                .call("Event", &args, &mut host, Limits::default())
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn posted_zero_and_overdue_events_wait_for_the_first_frame_pass() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Main(){SetEventHandler("E","H",0);PostEvent("E",0,"l",7);PostEvent("E",10,"l",8);}
        void H(){int n=GetEventData();Record(n);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Main", &[], &mut host, Limits::default())?;
    assert_eq!(runtime.advance_frame(100, &mut host, Limits::default())?, 0);
    assert!(host.0.is_empty());
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 2);
    assert_eq!(host.0, vec![vec![Value::Int(7)], vec![Value::Int(8)]]);
    assert_eq!(runtime.pending_events(), 0);
    assert_eq!(runtime.retained_post_events(), 1); // retained cleanup index quirk
    runtime.advance_frame(0, &mut host, Limits::default())?;
    assert_eq!(runtime.retained_post_events(), 0);
    Ok(())
}

#[test]
fn posting_from_a_handler_prepares_the_new_event_during_that_same_frame() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Main(){SetEventHandler("A","A",0);SetEventHandler("B","B",0);PostEvent("A",0);}
        void A(){Record(1);PostEvent("B",0,"l",2);}
        void B(){int n=GetEventData();Record(n);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Main", &[], &mut host, Limits::default())?;
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 0);
    assert_eq!(runtime.advance_frame(5, &mut host, Limits::default())?, 1);
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 1);
    assert_eq!(host.0, vec![vec![Value::Int(1)], vec![Value::Int(2)]]);
    assert_eq!(runtime.posted_events(), 2);
    Ok(())
}

#[test]
fn deletion_cancels_matching_undue_posts_and_prepares_unrelated_posts() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Main(){SetEventHandler("A","A",0);SetEventHandler("B","B",0);PostEvent("A",20);PostEvent("B",10);DelEventHandler("A","A");}
        void A(){Record(1);}
        void B(){Record(2);}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Main", &[], &mut host, Limits::default())?;
    assert_eq!(runtime.pending_events(), 1);
    // DelEventHandler already called ProcessTime(0), so B has no preparation tick.
    assert_eq!(runtime.advance_frame(10, &mut host, Limits::default())?, 1);
    assert_eq!(host.0, vec![vec![Value::Int(2)]]);
    Ok(())
}

#[test]
fn clear_post_events_inside_a_handler_skips_remaining_deliveries_and_preserves_cleanup_quirk()
-> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        native void Record(int n);
        void Main(){SetEventHandler("E","H",0);PostEvent("E",0);PostEvent("E",0);}
        void H(){Record(1);ClearPostEvents();}
    "#,
    )?;
    let mut host = Trace::default();
    runtime.call("Main", &[], &mut host, Limits::default())?;
    runtime.advance_frame(0, &mut host, Limits::default())?;
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 1);
    assert_eq!(host.0, vec![vec![Value::Int(1)]]);
    assert_eq!(
        (runtime.pending_events(), runtime.retained_post_events()),
        (0, 1)
    );
    runtime.advance_frame(0, &mut host, Limits::default())?;
    assert_eq!(runtime.retained_post_events(), 0);
    Ok(())
}

#[test]
fn posted_argument_threshold_distinguishes_missing_message_from_empty_message() -> Result<()> {
    for (source, expected) in [
        (
            "void Main(){SetEventHandler(\"E\",\"H\",0);PostEvent(\"E\",0,123);}",
            "No data on this event",
        ),
        (
            "void Main(){SetEventHandler(\"E\",\"H\",0);PostEvent(\"E\",0,\"\",123);}",
            "No (more) data on this event",
        ),
    ] {
        let mut runtime =
            Runtime::compile(&(source.to_owned() + "void H(){int n=GetEventData();}"))?;
        runtime.call("Main", &[], &mut (), Limits::default())?;
        runtime.advance_frame(0, &mut (), Limits::default())?;
        let error = runtime
            .advance_frame(0, &mut (), Limits::default())
            .unwrap_err();
        assert!(format!("{error:#}").contains(expected));
        assert_eq!(runtime.pending_events(), 0);
    }
    Ok(())
}

#[test]
fn nested_events_and_frame_fanout_share_limits_and_leave_later_calls_usable() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        void Init(){SetEventHandler("E","Recurse",0);}
        void Recurse(){Event("E","l",1);}
        int Fine(){return 7;}
    "#,
    )?;
    runtime.call("Init", &[], &mut (), Limits::default())?;
    for limits in [
        Limits::default(),
        Limits {
            instructions: 20,
            ..Limits::default()
        },
    ] {
        assert!(
            runtime
                .emit("E", &mut message(&[])?, &mut (), limits)
                .is_err()
        );
        assert_eq!(
            runtime.call("Fine", &[], &mut (), Limits::default())?,
            Value::Int(7)
        );
    }
    let mut runtime = Runtime::compile(
        "void Main(){SetEventHandler(\"E\",\"H\",0);int n=0;while(n<30){PostEvent(\"E\",0);n++;}} void H(){int n=1;}",
    )?;
    runtime.call("Main", &[], &mut (), Limits::default())?;
    runtime.advance_frame(0, &mut (), Limits::default())?;
    let error = runtime
        .advance_frame(
            0,
            &mut (),
            Limits {
                instructions: 50,
                ..Limits::default()
            },
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("instruction budget"));
    assert!(runtime.completed_handlers() > 0 && runtime.completed_handlers() < 30);
    runtime.advance_frame(0, &mut (), Limits::default())?;
    // The interrupted event was invalidated before its handler ran; it is not retried.
    assert_eq!(runtime.completed_handlers(), 29);
    runtime.call(
        "PostEvent",
        &[Value::String("E".into()), Value::Int(0)],
        &mut (),
        Limits::default(),
    )?;
    for _ in 0..2 {
        runtime.advance_frame(0, &mut (), Limits::default())?;
    }
    assert_eq!(runtime.completed_handlers(), 30);
    let mut runtime = Runtime::compile(
        r#"
        void Main(){SetEventHandler("E","H",0);PostEvent("E",0);}
        void H(){PostEvent("E",0);DelEventHandler("Other","H");}
        int Fine(){return 7;}
    "#,
    )?;
    runtime.call("Main", &[], &mut (), Limits::default())?;
    runtime.advance_frame(0, &mut (), Limits::default())?;
    let error = runtime
        .advance_frame(
            0,
            &mut (),
            Limits {
                instructions: 200,
                ..Limits::default()
            },
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("instruction budget"));
    assert!(runtime.posted_events() > 1 && runtime.posted_events() < 256);
    assert_eq!(
        runtime.call("Fine", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    Ok(())
}

#[test]
fn queued_count_and_owned_payload_budgets_are_reclaimed_after_cleanup() -> Result<()> {
    let mut runtime = Runtime::compile("void Add(){PostEvent(\"E\",0);}")?;
    for _ in 0..256 {
        runtime.call("Add", &[], &mut (), Limits::default())?;
    }
    assert!(
        runtime
            .call("Add", &[], &mut (), Limits::default())
            .is_err()
    );
    runtime.call("ClearPostEvents", &[], &mut (), Limits::default())?;
    for _ in 0..2 {
        runtime.advance_frame(0, &mut (), Limits::default())?;
    }
    assert_eq!(runtime.retained_post_events(), 0);
    let mut args = vec![
        Value::String("E".into()),
        Value::Int(0),
        Value::String("s".repeat(128)),
    ];
    args.extend((0..128).map(|_| Value::String("a".repeat(4096))));
    runtime.call("PostEvent", &args, &mut (), Limits::default())?;
    assert!(
        runtime
            .call("PostEvent", &args, &mut (), Limits::default())
            .is_err()
    );
    runtime.call("ClearPostEvents", &[], &mut (), Limits::default())?;
    runtime.advance_frame(0, &mut (), Limits::default())?;
    runtime.call("PostEvent", &args, &mut (), Limits::default())?;
    Ok(())
}
