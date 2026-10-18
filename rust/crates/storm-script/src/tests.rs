use super::*;

fn eval(kind: &str, expression: &str) -> Result<Value> {
    Program::compile(&format!("{kind} main() {{ return {expression}; }}"))?.call(
        "main",
        &[],
        &mut (),
        Limits::default(),
    )
}

#[test]
fn original_precedence_and_numeric_quirks() -> Result<()> {
    assert_eq!(eval("int", "2 + 3 * 4 ^ 2")?, Value::Int(50));
    assert_eq!(eval("int", "-2 ^ 2")?, Value::Int(4));
    assert_eq!(eval("int", "true || false && false")?, Value::Int(0));
    assert_eq!(eval("int", "2 ^ -1")?, Value::Int(2));
    assert_eq!(eval("int", "2 ^ -3")?, Value::Int(0));
    assert_eq!(eval("float", "2.0 ^ -3")?, Value::Float(0.5));
    assert_eq!(eval("float", "7.5 % 2")?, Value::Float(3.0));
    assert_eq!(eval("int", "7 % -2.5")?, Value::Int(1));
    assert_eq!(eval("int", "!0.5")?, Value::Int(1));
    assert_eq!(eval("int", "0.5 && 0")?, Value::Int(1));
    assert_eq!(eval("int", "0.5 || 0")?, Value::Int(1));
    assert_eq!(eval("int", "7 / -2")?, Value::Int(-3));
    assert!(Program::compile("int main(){return 2^3^2;}").is_err());
    assert!(Program::compile("int main(){return (true || false);}").is_err());
    Ok(())
}

#[test]
fn strings_are_raw_and_ascii_comparisons_ignore_case() -> Result<()> {
    assert_eq!(
        eval("string", r#""mask=" + 548"#)?,
        Value::String("mask=548".into())
    );
    assert_eq!(
        eval("string", r#"7 + ":mask""#)?,
        Value::String("7:mask".into())
    );
    assert_eq!(eval("int", r#""Sails" == "sAILS""#)?, Value::Int(1));
    assert_eq!(eval("int", r#""12tail" == 12"#)?, Value::Int(1));
    assert_eq!(eval("int", r#"" 12" == 0"#)?, Value::Int(1));
    assert_eq!(eval("string", r#""a\nb""#)?, Value::String("a\\nb".into()));
    assert!(eval("string", r#"1.0 + "s""#).is_err());
    assert!(eval("int", r#""я" == "Я""#).is_err());
    Ok(())
}

#[test]
fn functions_loops_case_and_assignment_conversions() -> Result<()> {
    let source = r#"
    /* Forward call and function-scope locals. */
    int MAIN() { return SuM(10); }
    int sum(int n) {
        int result = 0;
        while (n > 0) {
            n--;
            if (n == 8) continue;
            if (n == 2) break;
            result += n;
        }
        int value;
        value = result;
        return value;
    }"#;
    assert_eq!(
        Program::compile(source)?.call("MAIN", &[], &mut (), Limits::default())?,
        Value::Int(34)
    );
    assert_eq!(eval("float", "1 + 2.5")?, Value::Float(3.5));
    assert!(eval("int", "2.5").is_err());
    Ok(())
}

#[test]
fn short_circuit_skips_native_side_effects_and_zero_division() -> Result<()> {
    struct Counter(usize);
    impl Host for Counter {
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
            self.0 += 1;
            Ok(Value::Int(1))
        }
    }
    let p = Program::compile(
        "native int hit(); int main(){int a=0 && hit(); int b=1 || hit(); return hit()+a+b;}",
    )?;
    let mut host = Counter(0);
    assert_eq!(
        p.call("main", &[], &mut host, Limits::default())?,
        Value::Int(2)
    );
    assert_eq!(host.0, 1);
    assert_eq!(eval("int", "0 && 1/0")?, Value::Int(0));
    assert_eq!(eval("int", "1 || 1/0")?, Value::Int(1));
    Ok(())
}

#[test]
fn cpu_sail_fixture_executes_and_changes_retained_attribute_tree() -> Result<()> {
    let p = Program::compile(include_str!("../fixtures/sail_damage.c"))?;
    let mut host = preview::AttributeHost::new()?;
    let result = p.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(6));
    assert_eq!(host.report(&result)["attribute_writes"], 3);
    assert_eq!(
        host.call(
            "previewgetattribute",
            &[Value::String("ship.sails.rey_a1.1.hd".into())]
        )?,
        Value::String("548".into())
    );
    assert_eq!(
        host.call(
            "previewgetattribute",
            &[Value::String("ship.sails.rey_b1.2.hd".into())]
        )?,
        Value::String("274".into())
    );
    Ok(())
}

#[test]
fn bounded_execution_stops_loops_recursion_and_expensive_power() -> Result<()> {
    for source in [
        "void main(){while(1){}}",
        "int main(){return main();}",
        "int main(){return 2 ^ 2147483647;}",
    ] {
        let error = Program::compile(source)?
            .call(
                "main",
                &[],
                &mut (),
                Limits {
                    instructions: 500,
                    ..Limits::default()
                },
            )
            .unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains("budget") || text.contains("depth"), "{text}");
    }
    let p = Program::compile("int main(){return 1+2;}")?;
    assert!(
        p.call(
            "main",
            &[],
            &mut (),
            Limits {
                stack: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(
        p.call(
            "main",
            &[],
            &mut (),
            Limits {
                frames: 1000,
                ..Limits::default()
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn malformed_or_unsupported_sources_fail_with_locations() {
    for source in [
        "int main(){return unknown;}",
        "int main(){return missing();}",
        "int main(){break;}",
        "int main(){int x; int X;}",
        "int main(){return 0x20;}",
        "int main(){return 1e3;}",
        "int main(){return !!1;}",
        "int main(){return 1;",
        "int main(){return \"unclosed;}",
        "int main(){/* unclosed",
        "void character;",
        "#include \"game.c\"",
        "int main(){return 0;}\0",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    let error = Program::compile("int main(){\n return unknown;\n}")
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("2:"));
}

#[test]
fn runtime_errors_are_located_and_next_call_is_clean() -> Result<()> {
    for expression in ["1 / 0", "1.0 / 0.0", "7 % 0.5", "2147483648 / -1"] {
        let error = eval("int", expression).unwrap_err();
        assert!(format!("{error:#}").contains("main <source>:1:"));
    }
    let p = Program::compile("int main(int n){return 6/n;}")?;
    assert!(
        p.call("main", &[Value::Int(0)], &mut (), Limits::default())
            .is_err()
    );
    assert_eq!(
        p.call("main", &[Value::Int(2)], &mut (), Limits::default())?,
        Value::Int(3)
    );
    assert!(p.call("main", &[], &mut (), Limits::default()).is_err());
    Ok(())
}

#[test]
fn compile_and_string_growth_limits_are_enforced() -> Result<()> {
    let source = format!(
        "int main(){{return {}1{};}}",
        "(".repeat(80),
        ")".repeat(80)
    );
    assert!(Program::compile(&source).is_err());
    assert!(Program::compile(&" ".repeat(262145)).is_err());
    assert!(eval("string", &format!("\"{}\"", "a".repeat(4097))).is_err());
    assert!(
        Program::compile("void main(){string s=\"a\";while(1){s=s+s;}}")?
            .call("main", &[], &mut (), Limits::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn scalars_match_unedited_retained_cpp_methods() -> Result<()> {
    use serde_json::Value as Json;
    let fixture: Json = serde_json::from_str(include_str!("../reference/cases.json"))?;
    let input = |value: &Json| -> Value {
        let text = value[1].as_str().unwrap();
        match value[0].as_str().unwrap() {
            "i" => Value::Int(text.parse().unwrap()),
            "f" => Value::Float(text.parse().unwrap()),
            "s" => Value::String(text.into()),
            _ => unreachable!(),
        }
    };
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let mut lhs = input(&case["a"]);
        let rhs = input(&case["b"]);
        let mut operation = case["op"].as_str().unwrap();
        let reference = operation.strip_prefix("ref_");
        let variable = Variable::new(lhs.clone());
        if let Some(op) = reference {
            lhs = variable.reference();
            operation = op;
        }
        let mut fuel = 10000;
        let actual = match operation {
            "to_int" => {
                if let Value::Reference(r) = lhs {
                    r.convert(Type::Int)?
                } else {
                    lhs.convert(Type::Int)?
                }
            }
            "to_float" => {
                if let Value::Reference(r) = lhs {
                    r.convert(Type::Float)?
                } else {
                    lhs.convert(Type::Float)?
                }
            }
            "to_string" => lhs.convert(Type::String)?,
            "attr_text" => Value::String(lhs.attribute_text()?),
            "neg" => lhs.apply_negate()?,
            "!" => lhs.not()?,
            "bool" => lhs.bool_convert()?,
            _ => lhs.apply_binary(
                match operation {
                    "+" => Op::Add,
                    "-" => Op::Sub,
                    "*" => Op::Mul,
                    "/" => Op::Div,
                    "%" => Op::Mod,
                    "^" => Op::Pow,
                    "==" => Op::Eq,
                    "!=" => Op::Ne,
                    "<" => Op::Lt,
                    "<=" => Op::Le,
                    ">" => Op::Gt,
                    ">=" => Op::Ge,
                    _ => unreachable!(),
                },
                rhs,
                &mut fuel,
            )?,
        };
        let actual = actual.resolved()?;
        if reference.is_some() {
            match (variable.value()?, &actual) {
                (Value::Float(a), Value::Float(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                (a, b) => assert_eq!(&a, b, "reference case {index}"),
            }
        }
        let expected = &case["result"];
        let actual = match actual {
            Value::Int(n) => serde_json::json!(["i", n]),
            Value::Float(n) => serde_json::json!(["f", n.to_bits()]),
            Value::String(s) => serde_json::json!(["s", s]),
            _ => unreachable!(),
        };
        assert_eq!(&actual, expected, "original C++ case {index}: {case}");
    }
    Ok(())
}

#[test]
fn native_contracts_and_void_returns_fail_without_poisoning_later_calls() -> Result<()> {
    struct BadHost;
    impl Host for BadHost {
        fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
            Ok(if name == "oversized" {
                Value::String("a".repeat(4097))
            } else {
                Value::Void
            })
        }
    }
    let p = Program::compile(
        "native int bad(); native string oversized(); int main(){return bad();} string large(){return oversized();}",
    )?;
    assert!(
        p.call("main", &[], &mut BadHost, Limits::default())
            .is_err()
    );
    assert!(
        p.call("large", &[], &mut BadHost, Limits::default())
            .is_err()
    );
    assert!(p.call("main", &[], &mut (), Limits::default()).is_err());
    let p = Program::compile("void noop(){return;} void main(){return noop();}")?;
    assert!(p.call("main", &[], &mut (), Limits::default()).is_err());
    assert_eq!(
        p.call("noop", &[], &mut (), Limits::default())?,
        Value::Void
    );
    for newline in ["\n", "\r", "\r\n"] {
        let error = Program::compile(&format!("int main(){{{newline}return unknown;}}"))
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("2:"));
    }
    Ok(())
}
