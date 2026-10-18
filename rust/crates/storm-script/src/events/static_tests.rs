use super::*;

fn empty() -> Result<Message> {
    Message::new("", vec![])
}

#[test]
fn static_handlers_are_ready_before_entry_and_follow_source_order_including_dead_code() -> Result<()>
{
    let source = r#"
        #event_handler(EVENT_NAME, "First")
        int state;
        void NeverCalled() {
            if (0) { #event_handler("READY", "Second"); }
            return;
            #event_handler(EVENT_NAME, "Third")
        }
        #event_handler("ready", "FIRST");
        #define EVENT_NAME "Ready"
        int First() { state=state*10+1; return state; }
        int Second() { state=state*10+2; return state; }
        int Third() { state=state*10+3; return state; }
        int Main() { return state; }
    "#;
    let mut runtime = Runtime::compile(source)?;
    assert_eq!(
        runtime.emit("rEaDy", &mut empty()?, &mut (), Limits::default())?,
        Some(Value::Int(123))
    );
    assert_eq!(runtime.completed_handlers(), 3);
    assert_eq!(
        runtime.call("main", &[], &mut (), Limits::default())?,
        Value::Int(123)
    );
    Ok(())
}

#[test]
fn static_handlers_seed_one_shot_calls_but_are_not_reinstalled_on_each_persistent_call()
-> Result<()> {
    let source = r#"
        #event_handler("E", "Hit")
        int hits;
        void Hit() { hits++; }
        int Main() { Event("E"); return hits; }
        void Disable() { DelEventHandler("E", "Hit"); }
        void Enable() { SetEventHandler("E", "Hit", 1); }
    "#;
    let program = Program::compile(source)?;
    for _ in 0..2 {
        assert_eq!(
            program.call("main", &[], &mut (), Limits::default())?,
            Value::Int(1)
        );
    }
    let mut runtime = Runtime::compile(source)?;
    assert_eq!(
        runtime.call("main", &[], &mut (), Limits::default())?,
        Value::Int(1)
    );
    runtime.call("disable", &[], &mut (), Limits::default())?;
    for _ in 0..2 {
        runtime.process_frame()?;
        assert_eq!(
            runtime.call("main", &[], &mut (), Limits::default())?,
            Value::Int(1)
        );
    }
    assert_eq!(runtime.handlers.entries["e"].len(), 1);
    assert!(runtime.handlers.entries["e"][0].is_static);
    runtime.call("enable", &[], &mut (), Limits::default())?;
    assert_eq!(
        runtime.call("main", &[], &mut (), Limits::default())?,
        Value::Int(2)
    );
    let mut independent = Runtime::compile(source)?;
    assert_eq!(
        independent.call("main", &[], &mut (), Limits::default())?,
        Value::Int(1)
    );
    Ok(())
}

#[test]
fn directives_are_zero_width_in_control_flow_and_leave_their_semicolon() -> Result<()> {
    let source = r#"
        int hits;
        void H() { hits+=100; }
        int Main() {
            if (0) #event_handler("A", "H") hits+=10;
            if (0) #event_handler("B", "H"); hits++;
            if (1) #event_handler("C", "H") hits+=2; else hits+=20;
            while (0) #event_handler("D", "H") hits+=40;
            return hits;
        }
    "#;
    let mut runtime = Runtime::compile(source)?;
    assert_eq!(
        runtime.call("main", &[], &mut (), Limits::default())?,
        Value::Int(3)
    );
    for name in ["A", "B", "C", "D"] {
        runtime.emit(name, &mut empty()?, &mut (), Limits::default())?;
    }
    assert_eq!(runtime.completed_handlers(), 4);
    Ok(())
}

#[test]
fn static_function_bindings_reject_unknown_or_parameterized_targets_and_bad_syntax() {
    for source in [
        "#event_handler \"E\", \"H\") void H(){}",
        "#event_handler(7, \"H\") void H(){}",
        "#event_handler(\"E\" \"H\") void H(){}",
        "#event_handler(\"E\", H) void H(){}",
        "#define FN \"H\" #event_handler(\"E\", FN) void H(){}",
        "#event_handler(\"E\", \"H\", 0) void H(){}",
        "#event_handler(\"E\", \"H\" void H(){}",
        "#event_handler(\"E\", \"Missing\")",
        "#event_handler(\"E\", \"H\") void H(int n){}",
        "#event_handler(E, \"H\") void H(){}",
        "#define E 7 #event_handler(E, \"H\") void H(){}",
        "#event_handler(\"\", \"H\") void H(){}",
        "#event_handler(\"событие\", \"H\") void H(){}",
        "int Main(){return #event_handler(\"E\", \"H\") 1;} void H(){}",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
}

#[test]
fn static_registration_uses_the_shared_event_and_handler_budgets() -> Result<()> {
    let mut source = String::from("void H(){}\n");
    for n in 0..128 {
        source += &format!("#event_handler(\"E{n}\", \"H\")\n");
    }
    for _ in 0..20 {
        source += "#event_handler(\"E0\", \"H\")\n";
    }
    Program::compile(&source)?;
    source += "#event_handler(\"Overflow\", \"H\")";
    assert!(
        format!("{:#}", Program::compile(&source).err().unwrap()).contains("event name budget")
    );
    let mut source = String::new();
    for n in 0..64 {
        source += &format!("void H{n}(){{}}\n");
    }
    for e in 0..8 {
        for n in 0..64 {
            source += &format!("#event_handler(\"E{e}\", \"H{n}\")\n");
        }
    }
    Program::compile(&source)?;
    source += "#event_handler(\"Overflow\", \"H0\")";
    assert!(format!("{:#}", Program::compile(&source).err().unwrap()).contains("handler budget"));
    let mut runtime = Runtime::compile("#event_handler(\"E\", \"H\") int H(){return 7;}")?;
    let error = runtime
        .emit(
            "E",
            &mut empty()?,
            &mut (),
            Limits {
                instructions: 0,
                ..Limits::default()
            },
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("instruction budget"));
    assert_eq!(
        runtime.emit("E", &mut empty()?, &mut (), Limits::default())?,
        Some(Value::Int(7))
    );
    Ok(())
}

#[test]
fn static_lifetime_preserves_deleted_first_entries_and_bounds_the_original_frame_hang() -> Result<()>
{
    let source = r#"
        #event_handler("E", "A")
        #event_handler("E", "B")
        void A(){}
        void B(){}
        void DisableB(){DelEventHandler("E", "B");}
        void EnableB(){SetEventHandler("E", "B", 1);}
    "#;
    let mut runtime = Runtime::compile(source)?;
    runtime.call("disableb", &[], &mut (), Limits::default())?;
    assert!(format!("{:#}", runtime.process_frame().unwrap_err()).contains("would not terminate"));
    assert!(
        format!(
            "{:#}",
            runtime
                .advance_frame(0, &mut (), Limits::default())
                .unwrap_err()
        )
        .contains("would not terminate")
    );
    runtime.call("enableb", &[], &mut (), Limits::default())?;
    runtime.advance_frame(0, &mut (), Limits::default())?;
    runtime.emit("E", &mut empty()?, &mut (), Limits::default())?;
    assert_eq!(runtime.completed_handlers(), 2);
    Ok(())
}

#[test]
fn static_four_file_ship_matches_the_complete_dynamic_registration_fixture() -> Result<()> {
    fn run(folder: &str) -> Result<(serde_json::Value, Vec<String>, u64, u64)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(folder);
        let mut runtime =
            Runtime::compile_with_loader("MAIN.C", &mut crate::DirectorySource::new(root)?)?;
        let mut host = crate::preview::AttributeHost::new()?;
        let result = runtime.call("main", &[], &mut host, Limits::default())?;
        for _ in 0..2 {
            runtime.advance_frame(0, &mut host, Limits::default())?;
        }
        Ok((
            host.report(&result),
            runtime.sources().to_vec(),
            runtime.completed_handlers(),
            runtime.posted_events(),
        ))
    }
    let actual = run("static_ship")?;
    assert_eq!(actual, run("multi_ship")?);
    assert_eq!(actual.2, 3);
    assert_eq!(actual.3, 1);
    Ok(())
}

#[test]
fn static_declarations_match_the_retained_lexer_and_compiler_case() -> Result<()> {
    use serde_json::{Value as Json, json};
    let reference: Json =
        serde_json::from_str(include_str!("../../reference/static_handlers/cases.json"))?;
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let mut source = String::new();
        for (name, value) in case["defines"].as_object().unwrap() {
            source += &format!("#define {name} {value}\n");
        }
        source += case["source"].as_str().unwrap();
        for function in case["functions"].as_array().unwrap() {
            source += &format!("\nvoid {}(){{}}", function.as_str().unwrap());
        }
        let compiled = Runtime::compile(&source);
        if case["expected"]["ok"] == false {
            assert!(compiled.is_err(), "{source}");
            continue;
        }
        let runtime = compiled?;
        let actual = ["e", "other"].map(|name| {
            runtime.handlers.entries.get(name).map(|entries| {
                entries
                    .iter()
                    .map(|h| {
                        let index = case["functions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .position(|f| f.as_str().unwrap().eq_ignore_ascii_case(&h.function))
                            .unwrap();
                        json!([
                            index,
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
        assert_eq!(json!(actual), case["expected"]["table"], "{source}");
    }
    Ok(())
}

#[test]
fn static_lifetimes_match_unedited_cpp_and_its_confirmed_frame_hang_becomes_an_error() -> Result<()>
{
    use serde_json::{Value as Json, json};
    let reference: Json =
        serde_json::from_str(include_str!("../../reference/static_handlers/cases.json"))?;
    let cases = reference["table_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    let mut checked = 0;
    let mut hangs = 0;
    for case in cases {
        let mut handlers = Handlers::default();
        for (index, operation) in case["operations"].as_array().unwrap().iter().enumerate() {
            match operation[0].as_str().unwrap() {
                "add" => handlers.add(
                    operation[1].as_str().unwrap(),
                    &operation[2].to_string(),
                    operation[3].as_i64().unwrap() as i32,
                    operation[4].as_bool().unwrap(),
                )?,
                "delete" => {
                    handlers.delete(operation[1].as_str().unwrap(), &operation[2].to_string())?
                }
                "frame"
                    if case["nonterminating_frame"] == true
                        && index == case["snapshots"].as_array().unwrap().len() =>
                {
                    assert!(
                        format!("{:#}", handlers.frame().unwrap_err())
                            .contains("would not terminate")
                    );
                    hangs += 1;
                    continue;
                }
                "frame" => handlers.frame()?,
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
                "{operation}, snapshot {checked}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 55);
    assert_eq!(hangs, 1);
    Ok(())
}
