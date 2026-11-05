use super::*;

fn call(runtime: &mut Runtime, name: &str) -> Result<Value> {
    runtime.call(name, &[], &mut (), Limits::default())
}

#[test]
fn globals_resolve_forward_keep_case_insensitive_state_and_isolate_runtimes() -> Result<()> {
    let source = r#"
        int bump() { Count++; return count; }
        int shadow() { int COUNT = 90; return count; }
        int fail() { count++; return 1 / 0; }
        int Count = -3, Other = 8;
    "#;
    let mut a = Runtime::compile(source)?;
    assert_eq!(call(&mut a, "BUMP")?, Value::Int(-2));
    assert_eq!(call(&mut a, "shadow")?, Value::Int(90));
    assert!(call(&mut a, "fail").is_err());
    assert_eq!(call(&mut a, "bump")?, Value::Int(0));
    assert_eq!(
        call(&mut Runtime::compile(source)?, "bump")?,
        Value::Int(-2)
    );
    let p = Program::compile(source)?;
    for _ in 0..2 {
        assert_eq!(
            p.call("bump", &[], &mut (), Limits::default())?,
            Value::Int(-2)
        );
    }
    Ok(())
}

#[test]
fn global_scalar_and_partial_array_initializers_keep_declared_types() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int numbers[4] = {-7, 19};
        float factors[3] = {-1.5, 2.5};
        string names[3] = {"mast", "sail",};
        bool ready = true;
        int main() {
            if (factors[0] != -1.5) return -1;
            if (names[1] != "SAIL") return -2;
            if (names[2] != "") return -3;
            if (factors[2] != 0.0) return -4;
            return numbers[0] + numbers[1] + numbers[3] + ready;
        }
    "#,
    )?;
    assert_eq!(call(&mut runtime, "main")?, Value::Int(13));
    Ok(())
}

#[test]
fn local_arrays_are_fresh_for_every_call_and_recursive_frame() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int visit(int depth) {
            int local[2];
            local[0] = depth;
            if (depth > 0) local[1] = visit(depth - 1);
            return local[0] + local[1];
        }
        int main() { return visit(5); }
    "#,
    )?;
    for _ in 0..3 {
        assert_eq!(call(&mut runtime, "main")?, Value::Int(15));
    }
    Ok(())
}

#[test]
fn indexed_lvalues_evaluate_their_index_once_and_allow_nested_index_expressions() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int values[3] = {10,20,30}; int index;
        int next() { index++; return index - 1; }
        int main() {
            values[2] + 1;
            values[next()] += 5;
            values[next()]++;
            values[values[1] - 20] *= 2;
            return values[0] + values[1] + index;
        }
    "#,
    )?;
    assert_eq!(call(&mut runtime, "main")?, Value::Int(59));
    Ok(())
}

#[test]
fn ref_array_reads_alias_elements_but_direct_array_reads_copy_scalars() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int values[2] = {7,9};
        int main() {
            int a = values[0] + 2;
            ref alias; makeref(alias, values);
            int b = alias[0] + 3;
            return a + b + values[0];
        }
    "#,
    )?;
    assert_eq!(call(&mut runtime, "main")?, Value::Int(29));
    Ok(())
}

#[test]
fn object_and_reference_arrays_preserve_identity_and_attribute_aliases_across_calls() -> Result<()>
{
    let mut runtime = Runtime::compile(
        r#"
        object Characters[2]; ref selected; ref aliases[2]; aref sail;
        void setup() {
            Characters[1].ship.sails.hd = 548;
            makeref(selected, Characters[1]);
            makeref(aliases[0], Characters[1]);
            makearef(sail, aliases[0].ship.sails);
        }
        int check() {
            selected.ship.sails.hd = 274;
            return sti(sail.hd);
        }
        int deep_copy() {
            object copy; copy = Characters[1]; copy.ship.sails.hd = 0;
            return sti(Characters[1].ship.sails.hd);
        }
    "#,
    )?;
    call(&mut runtime, "setup")?;
    assert_eq!(call(&mut runtime, "check")?, Value::Int(274));
    assert_eq!(call(&mut runtime, "deep_copy")?, Value::Int(274));
    Ok(())
}

#[test]
fn resize_preserves_prefix_and_array_flag_at_one_and_expires_removed_references() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int values[3] = {7,8,9}; ref all, retained, removed;
        void setup() { makeref(all, values); makeref(retained, values[0]); makeref(removed, values[2]); }
        int grow() { SetArraySize(all, 4); retained++; return values[0] + values[3] + GetArraySize(all); }
        int shrink() { SetArraySize(all, 1); return values[0] + GetArraySize(all); }
        int bad() { int value = removed; return value; }
        int restore() { SetArraySize(all, 3); return values[0] + values[2]; }
    "#,
    )?;
    call(&mut runtime, "setup")?;
    assert_eq!(call(&mut runtime, "grow")?, Value::Int(12));
    assert_eq!(call(&mut runtime, "shrink")?, Value::Int(9));
    let error = call(&mut runtime, "bad").unwrap_err();
    assert!(format!("{error:#}").contains("expired ref"));
    assert_eq!(call(&mut runtime, "restore")?, Value::Int(8));
    assert!(call(&mut runtime, "bad").is_err());
    Ok(())
}

#[test]
fn object_array_shrink_expires_attributes_and_reclaims_object_roots() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        object objects[2]; ref all; aref old;
        void setup() { objects[1].hd=548; makearef(old,objects[1]); makeref(all,objects); }
        void shrink() { SetArraySize(all,1); }
        string bad() { return old.hd; }
    "#,
    )?;
    call(&mut runtime, "setup")?;
    call(&mut runtime, "shrink")?;
    assert!(call(&mut runtime, "bad").is_err());
    Ok(())
}

#[test]
fn zero_and_single_element_declarations_are_scalars_and_bad_indices_fail_located() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int zero[0], one[1], values[2];
        int main() { zero=4; one=5; return zero + one; }
        int bad(int index) { return values[index]; }
        int scalar() { return one[0]; }
        int float_index() { return values[0.0]; }
        int missing_ref() { return GetArraySize(values); }
    "#,
    )?;
    assert_eq!(call(&mut runtime, "main")?, Value::Int(9));
    for index in [-1, 2, i32::MAX] {
        let e = runtime
            .call("bad", &[Value::Int(index)], &mut (), Limits::default())
            .unwrap_err();
        assert!(format!("{e:#}").contains("bad "));
    }
    for name in ["scalar", "float_index", "missing_ref"] {
        assert!(call(&mut runtime, name).is_err());
    }
    Ok(())
}

#[test]
fn global_and_array_limits_reject_bad_declarations_and_reclaim_failed_call_allocations()
-> Result<()> {
    for source in [
        "int a[4097];",
        "int a[true];",
        "int a[2]={true,false};",
        "int a[-1];",
        "int a[2][2];",
        "int a[2]={1,2,3};",
        "float a=1;",
        "int a=1+2;",
        "object a={};",
        "int a; int A;",
        "native int variable;",
        "int a[2]; void main(){int b[2]={1,2};}",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    assert!(Program::compile(&(0..129).map(|i| format!("int g{i};")).collect::<String>()).is_err());
    let mut runtime = Runtime::compile(
        r#"
        int globals[4096]; ref all;
        int explode(int n) { int a[4096]; if(n>0) return explode(n-1); return a[0]; }
        int healthy() { int a[4096]; return a[0]+1; }
        void resize(int n) { makeref(all,globals); SetArraySize(all,n); }
    "#,
    )?;
    for _ in 0..3 {
        let e = runtime
            .call("explode", &[Value::Int(10)], &mut (), Limits::default())
            .unwrap_err();
        assert!(format!("{e:#}").contains("array cell budget"));
        assert_eq!(call(&mut runtime, "healthy")?, Value::Int(1));
    }
    for count in [0, -1, 4097] {
        assert!(
            runtime
                .call("resize", &[Value::Int(count)], &mut (), Limits::default())
                .is_err()
        );
    }
    assert_eq!(call(&mut runtime, "healthy")?, Value::Int(1));
    Ok(())
}

#[test]
fn globals_are_shared_by_nested_and_posted_handlers_without_reinitializing() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int counts[2] = {3,5};
        void start() {
            SetEventHandler("Now","Now",0); SetEventHandler("Later","Later",0);
            Event("Now"); PostEvent("Later",0);
        }
        void Now() { counts[0]++; }
        void Later() { counts[1] += counts[0]; Event("Now"); }
        int check() { return counts[0] + counts[1]; }
    "#,
    )?;
    call(&mut runtime, "start")?;
    assert_eq!(call(&mut runtime, "check")?, Value::Int(9));
    assert_eq!(runtime.advance_frame(0, &mut (), Limits::default())?, 0);
    assert_eq!(runtime.advance_frame(0, &mut (), Limits::default())?, 1);
    assert_eq!(call(&mut runtime, "check")?, Value::Int(14));
    Ok(())
}

#[test]
fn array_storage_matches_unedited_retained_cpp_snapshots() -> Result<()> {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../reference/arrays/cases.json"))?;
    let mut checked = 0;
    for sequence in reference["sequences"].as_array().unwrap() {
        let kind = match sequence["kind"].as_str().unwrap() {
            "i" => Type::Int,
            "f" => Type::Float,
            "s" => Type::String,
            _ => unreachable!(),
        };
        let world = World::default();
        let budget = storage::Budget::default();
        let mut variable = None;
        for (step, expected) in sequence["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(sequence["snapshots"].as_array().unwrap())
        {
            match step[0].as_str().unwrap() {
                "init" => {
                    variable = Some(
                        Declaration {
                            kind,
                            elements: step[1].as_u64().unwrap() as usize,
                            initial: Vec::new(),
                        }
                        .allocate(&world, &budget)?,
                    )
                }
                "resize" => {
                    let Value::Array(array) = variable.as_ref().unwrap().value()? else {
                        unreachable!()
                    };
                    array.resize(step[1].as_i64().unwrap() as i32)?;
                }
                "write" => {
                    let base = variable.as_ref().unwrap();
                    let target = if let Value::Array(array) = base.value()? {
                        array.element(step[1].as_i64().unwrap() as i32)?
                    } else {
                        base.clone()
                    };
                    let value = match kind {
                        Type::Int => Value::Int(step[2].as_i64().unwrap() as i32),
                        Type::Float => Value::Float(step[2].as_f64().unwrap() as f32),
                        Type::String => Value::String(step[2].as_str().unwrap().into()),
                        _ => unreachable!(),
                    };
                    target.assign(value, true)?;
                }
                _ => unreachable!(),
            }
            let base = variable.as_ref().unwrap();
            let (array, count, cells) = if let Value::Array(array) = base.value()? {
                (
                    true,
                    array.len(),
                    (0..array.len())
                        .map(|i| array.element(i as i32))
                        .collect::<Result<Vec<_>>>()?,
                )
            } else {
                (false, 1, vec![base.clone()])
            };
            let values = cells
                .iter()
                .map(|cell| -> Result<_> {
                    Ok(match cell.value()? {
                        Value::Int(n) => serde_json::json!(["i", n]),
                        Value::Float(n) => serde_json::json!(["f", n.to_bits()]),
                        Value::String(s) => serde_json::json!(["s", s]),
                        _ => unreachable!(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            assert_eq!(
                serde_json::json!({"array":array,"count":count,"values":values}),
                *expected,
                "{step}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 444);
    Ok(())
}

#[test]
fn global_character_fixture_keeps_ship_damage_until_a_later_frame_notifies_the_entity() -> Result<()>
{
    let mut runtime = Runtime::compile(include_str!("../fixtures/global_ships.c"))?;
    let mut host = preview::AttributeHost::new()?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(6));
    assert_eq!(host.report(&result)["ship_attribute_notifications"], 0);
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 0);
    assert_eq!(host.report(&result)["ship_attribute_notifications"], 0);
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 1);
    let report = host.report(&result);
    assert_eq!(report["ship_attribute_notifications"], 3);
    assert_eq!(
        report["ship_attribute_changes"],
        serde_json::json!([
            {"name":"hd","value":"548"}, {"name":"hd","value":"274"}, {"name":"holes","value":"6"}
        ])
    );
    assert_eq!(runtime.completed_handlers(), 3);
    assert_eq!(runtime.pending_events(), 0);
    Ok(())
}
