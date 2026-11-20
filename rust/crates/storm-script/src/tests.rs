use super::*;

#[test]
fn script_string_tests_use_byte_offsets_and_only_fold_ascii() -> Result<()> {
    let program = Program::compile(
        r#"
        int Main() {
            if ("Порт"=="" || "ПОРТ"=="порт") return 1;
            if ("ПортA"!="Портa" || "aПорт">"Б") return 2;
            if (strlen("яAB")!=4 || findsubstr("яAB", "ab", 1)!=2) return 3;
            if (findsubstr("яAB", "Я", 0)!=-1 || findsubstr("яAB", "", 4)!=4) return 4;
            if (findsubstr("яAB", "ab", -1)!=-1 || findsubstr("яAB", "ab", 3)!=-1) return 5;
            return 0;
        }"#,
    )?;
    assert_eq!(
        program.call("Main", &[], &mut (), Limits::default())?,
        Value::Int(0)
    );
    Ok(())
}

#[test]
fn startup_math_and_color_builtins_use_original_numeric_types() -> Result<()> {
    let program = Program::compile(
        "float Wave(int n){return sin(n);} int IntegerRoot(){return sqrt(12);} float Root(float n){return sqrt(n);}
         int Color(){return argb(255, 60, 128, 200);} int Spill(){return argb(0,0,0,256);}",
    )?;
    let limits = Limits::default();
    assert_eq!(
        program.call("Wave", &[Value::Int(0)], &mut (), limits)?,
        Value::Float(0.0)
    );
    assert_eq!(
        program.call("IntegerRoot", &[], &mut (), limits)?,
        Value::Int(3)
    );
    assert_eq!(
        program.call("Root", &[Value::Float(25.0)], &mut (), limits)?,
        Value::Float(5.0)
    );
    let Value::Float(negative_zero) =
        program.call("Root", &[Value::Float(-0.0)], &mut (), limits)?
    else {
        panic!("sqrt(float) must return float");
    };
    assert_eq!(negative_zero.to_bits(), (-0.0f32).to_bits());
    let error = program
        .call("Root", &[Value::Float(-1.0)], &mut (), limits)
        .unwrap_err();
    assert!(format!("{error:#}").contains("negative sqrt argument"));
    assert_eq!(
        program.call("Color", &[], &mut (), limits)?,
        Value::Int(0xff3c80c8u32 as i32),
    );
    assert_eq!(
        program.call("Spill", &[], &mut (), limits)?,
        Value::Int(256)
    );
    Ok(())
}

#[test]
fn original_native_names_link_without_declarations_and_validate_the_host_result() -> Result<()> {
    struct RandomHost(Value);
    impl Host for RandomHost {
        fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
            assert_eq!(name, "frnd");
            assert!(arguments.is_empty());
            Ok(self.0.clone())
        }
    }
    let program = Program::compile("float main(){return FrNd();}")?;
    let error = program
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("native function frnd is not registered"));
    assert_eq!(
        program.call(
            "main",
            &[],
            &mut RandomHost(Value::Float(0.25)),
            Limits::default(),
        )?,
        Value::Float(0.25)
    );
    let error = program
        .call(
            "main",
            &[],
            &mut RandomHost(Value::Int(0)),
            Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("native frnd return type mismatch"));
    assert!(Program::compile("float main(){return frnd(1);}").is_err());
    assert!(Program::compile("native float frnd();").is_err());
    let dynamic = Program::compile("float main(){string name=\"frnd\"; return call name(1);}")?;
    let error = dynamic
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("frnd expects 0 arguments"));
    Ok(())
}

#[test]
fn a_nonzero_parameter_script_call_in_an_expression_checks_arity_when_reached() -> Result<()> {
    let program = Program::compile(
        "int f(int x,int y){return x+y;} int main(int active){if(active) return f(2); return 9;}",
    )?;
    assert_eq!(
        program.call("main", &[Value::Int(0)], &mut (), Limits::default())?,
        Value::Int(9)
    );
    let error = program
        .call("main", &[Value::Int(1)], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("f expects 2 arguments"));
    assert!(Program::compile("int f(int x,int y){return x+y;} void main(){f(2);}").is_err());
    assert!(Program::compile("int f(){return 9;} int main(){return f(2);}").is_err());
    Ok(())
}

#[test]
fn unbraced_body_runs_through_its_own_separator_after_nested_control() -> Result<()> {
    let program = Program::compile(
        r#"
        int check(int found) {
            int count = 0;
            for (int n = 0; n < 3; n++) {
                if (found) continue
                if (1) continue;
                count += 100;
                count += 1;
            }
            return count;
        }
        int nested(int outer, int inner) {
            int score = 0;
            if (outer)
                if (inner) score += 1;
                else score += 2;
                score += 10;
            return score;
        }
        "#,
    )?;
    for (function, arguments, expected) in [
        ("check", vec![Value::Int(0)], 3),
        ("check", vec![Value::Int(1)], 0),
        ("nested", vec![Value::Int(0), Value::Int(0)], 0),
        ("nested", vec![Value::Int(1), Value::Int(0)], 12),
        ("nested", vec![Value::Int(1), Value::Int(1)], 11),
    ] {
        assert_eq!(
            program.call(function, &arguments, &mut (), Limits::default())?,
            Value::Int(expected)
        );
    }
    Ok(())
}

#[test]
fn standalone_else_observes_the_shared_expression_register_after_bare_blocks() -> Result<()> {
    let program = Program::compile(
        r#"
        int zero() { return 0; }
        int nonzero() { return 7; }
        void note() {}
        int main() {
            int result = 0;
            if (0) result = 90; else {
                nonzero();
                { note(); }
                else { result += 100; }
                zero();
                { note(); }
                else { result += 1; }
            }
            if (1) {
                if (0) result += 100; else result += 2;
                zero();
            } else result += 100;
            { note(); }
            else result += 100;
            if ("0") result += 100;
            else if (0) result += 100;
            else result += 4;
            string name = "zero";
            call name();
            else result += 100;
            name = "note";
            call name();
            else result += 8;
            return result;
        }"#,
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(15)
    );
    Ok(())
}

#[test]
fn standalone_else_tracks_loops_switches_logical_tests_and_move_conversions() -> Result<()> {
    let program = Program::compile(
        r#"
        int zero() { return 0; }
        int one() { return 1; }
        int main() {
            int result = 0;
            for (int i = 0; i < 3; i++) {
                if (i < 2) continue;
                result += 1;
            }
            else result += 2;
            while (1) { if (1) { zero(); break; } }
            else result += 4;
            switch (2) { case 1: result += 100; break; }
            else result += 8;
            int evaluated = 0 || 1;
            else result += 16;
            int converted = "0";
            else result += 32;
            object cargo;
            cargo.hold = 0;
            else result += 64;
            cargo.1 = 3;
            one();
            string key = "hold";
            string contents = cargo.(key);
            else result += 128;
            cargo.hold = 1;
            aref hold;
            makearef(hold, cargo.hold);
            if (hold) { cargo.hold = 0; } else result += 256;
            return result + evaluated + converted + sti(contents);
        }"#,
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(512)
    );
    Ok(())
}

#[test]
fn runtime_keeps_expression_state_across_calls_without_losing_reference_identity() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        int source = 0;
        void seed() { while (&source) {} }
        void set() { source = 5; }
        int query() { else return 7; return 9; }
        "#,
    )?;
    let mut host = ();
    let limits = Limits::default();
    runtime.call("seed", &[], &mut host, limits)?;
    runtime.call("set", &[], &mut host, limits)?;
    assert_eq!(
        runtime.call("query", &[], &mut host, limits)?,
        Value::Int(9)
    );
    let error = Program::compile("int query(){else return 7; return 9;}")?
        .call("query", &[], &mut host, limits)
        .unwrap_err();
    assert!(format!("{error:#}").contains("non-scalar value in integer conversion"));
    Ok(())
}

#[test]
fn dynamic_calls_read_the_name_after_arguments_and_support_returns_and_ref_arguments() -> Result<()>
{
    let program = Program::compile(
        "string handler=\"First\";int redirect(){handler=\"sEcOnD\";return 6;}int first(int n){return 100+n;}int second(int n){return n+1;}void touch(ref n){n=3;}int main(){int n=0;string name=\"Touch\";ref alias=&name;call alias(&n);return n*10+(call handler(redirect()));}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(37)
    );

    let program = Program::compile("int main(){string name=\"Missing\";return call name();}")?;
    let error = program
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("unknown function missing"));
    assert!(Program::compile("int main(){int name=0;return call name();}").is_err());
    assert!(Program::compile("void main(){call \"Missing\"();}").is_err());
    Ok(())
}

#[test]
fn assignment_after_direct_call_uses_the_current_frames_left_operand() -> Result<()> {
    let program = Program::compile(
        r#"
        int side = 0;
        int observe(string text) { side += 1; return sti(text); }
        int main() {
            object cargo;
            cargo.count = 2;
            if (sti(cargo.count) == 2) {
                observe(cargo.count) = 4;
            }
            int count = 7;
            observe(cargo.count) = 9;
            int values[3];
            values[1] = 1;
            observe(cargo.count) = 3;
            return sti(cargo.count) * 1000 + count * 100 + values[1] * 10 + side;
        }"#,
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(4933)
    );
    let error = Program::compile("void main() { sti(\"5\") = 0; }")?
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("no previous script assignment target"));
    Ok(())
}

#[test]
fn return_with_a_trailing_assignment_returns_before_the_move_or_rhs_call() -> Result<()> {
    let program = Program::compile(
        "native float side_effect(); float main(){float force=100.0;return force=side_effect();}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Float(100.0)
    );
    assert!(Program::compile("float main(){return 1.0=unknown();}").is_err());
    Ok(())
}

#[test]
fn value_returns_preserve_native_handling_of_trailing_closing_parentheses() -> Result<()> {
    let program = Program::compile(
        "int values[2]={0,7}; ref select(int index){return &values[index]);} int main(){ref result=select(1);return sti(result);}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    let program = Program::compile("int f(int n){return n;} int main(){return f(7)}")?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert!(Program::compile("int main(){return );}").is_err());
    assert!(Program::compile("void main(){return}").is_err());
    assert!(Program::compile("int main(){if(1) return 7); return 0;}").is_err());
    Ok(())
}

#[test]
fn if_body_can_start_after_the_extra_closing_token_used_by_the_game_scripts() -> Result<()> {
    let program = Program::compile(
        "int main(){int n=1;if(n==1)) n=7;else n=8;if(n==8)) return 0;return n;}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert!(Program::compile("void main(){if()) return;}").is_err());
    Ok(())
}

#[test]
fn plain_assignment_in_blocks_hands_control_tokens_back_to_the_statement_parser() -> Result<()> {
    let program = Program::compile(
        "void Log(int value){} int main(){int n=0;switch(1){case 1:n=(0)||(1)||(0) if(n==1){n=6}break;}bool ready=n==6 if(ready){int step, extra int one=1 if(ready){step=(2-one));n=n+step+extra}}Log(n) return n;}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert!(Program::compile("void main(){int n; if(n==0) n=1 if(n==1) n=2;}").is_err());
    assert!(Program::compile("void main(){int n; for(n=1 n<2; n++) {}}").is_err());
    assert!(Program::compile("void main(){int n; n=1 2;}").is_err());
    assert!(Program::compile("int f(){return 1;} int main(){f()+f() return 0;}").is_err());
    Ok(())
}

#[test]
fn native_statement_boundaries_preserve_assignments_and_direct_calls() -> Result<()> {
    let program = Program::compile(
        r#"
        void apply(ref target, int amount) { target += amount; }
        int main() {
            int hull = 5;
            switch (1) {
                case 1:
                    int damage = (2 + (3))
                    damage = damage + 1
                    apply(&hull, damage);
                    break;
            }
            return hull;
        }
        "#,
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(11)
    );
    let program = Program::compile(
        "int n; void apply() { n += (2 + 3)); } int main() { apply(): if (1) apply()); return n; }",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(10)
    );
    let program = Program::compile(
        "int main(){object fleet[3]; int slots[2]; string key=\"hold\"; int n=1 fleet[slots[1]].cargo.(key))=7: return n+sti(fleet[0].cargo.hold);}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(8)
    );
    let program = Program::compile(
        "int main(){object cargo; cargo.hold=7; aref hold; makearef(hold,cargo.hold) int total=sti(hold); return total;}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert!(Program::compile("void main(){int damage; damage=5 missing();}").is_err());
    Ok(())
}

#[test]
fn native_counted_for_updates_on_continue_and_skips_update_on_break() -> Result<()> {
    let program = Program::compile(
        "int main(){int sum=0;for(int i=5;i>0,i--){switch(i){case 4:continue;break;}if(i==1)break;sum+=i;}int j=3;for(;j<3;j++)sum+=100;return sum+j;}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(13)
    );
    assert!(Program::compile("void main(){int i;for(i=0;i<1;i+=1){}}").is_err());
    assert!(Program::compile("void main(){int i;for(return;i<1;i++){}}").is_err());
    Ok(())
}

#[test]
fn library_directives_keep_first_case_insensitive_requests_in_native_append_order() -> Result<()> {
    struct Files;
    impl SourceLoader for Files {
        fn load(&mut self, path: &str) -> Result<String> {
            Ok(match path {
                "main.c" => "#include \"child.c\"\n#libriary \"ROOT_LIB\"\n#libriary \"root_lib\"\nnative int answer();int main(){return answer();}",
                "child.c" => "#libriary \"child_lib\"\n#libriary \"Root_Lib\"",
                _ => anyhow::bail!("unknown fixture file"),
            }.into())
        }
    }
    let program = Program::compile_with_loader("main.c", &mut Files)?;
    assert_eq!(
        program
            .libraries()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["ROOT_LIB", "child_lib"]
    );
    assert_eq!(
        (
            &program.libraries()[0].file,
            program.libraries()[0].line,
            program.libraries()[0].column
        ),
        (&"main.c".to_owned(), 2, 1)
    );
    struct Libraries(Vec<String>);
    impl Host for Libraries {
        fn load_library(&mut self, name: &str) -> Result<()> {
            self.0.push(name.into());
            Ok(())
        }
        fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
            assert_eq!(name, "answer");
            assert_eq!(self.0, ["ROOT_LIB", "child_lib"]);
            Ok(Value::Int(42))
        }
    }
    let mut host = Libraries(Vec::new());
    assert_eq!(
        program.call("main", &[], &mut host, Limits::default())?,
        Value::Int(42)
    );
    Ok(())
}

#[test]
fn unbound_and_invalid_libraries_fail_explicitly_with_original_directive_locations() -> Result<()> {
    let program = Program::compile("#libriary \"missing\"\nint main(){return 7;}")?;
    let error = program
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    let text = format!("{error:#}");
    assert!(
        text.contains("<source>:1:1: #libriary missing") && text.contains("not registered"),
        "{text}"
    );
    for source in [
        "#libriary 1",
        "#libriary \"\"",
        "#libriary \"../module\"",
        "#libriary \"module.dll\"",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    struct Failing(usize);
    impl Host for Failing {
        fn load_library(&mut self, _: &str) -> Result<()> {
            self.0 += 1;
            anyhow::bail!("initialization failed after a native side effect")
        }
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
            unreachable!()
        }
    }
    let mut runtime = Runtime::compile("#libriary \"missing\"\nint main(){return 7;}")?;
    let mut host = Failing(0);
    for _ in 0..2 {
        assert!(
            runtime
                .call("main", &[], &mut host, Limits::default())
                .is_err()
        );
    }
    assert!(!runtime.is_initialized());
    assert_eq!(host.0, 1);
    let program = Program::compile(
        "int main(){switch(1){case 1:switch(2){case 0:return 4;break;break;case 2:return 5;break;}return 9;break;}return 0;}",
    )?;
    assert_eq!(
        program.call("main", &[], &mut (), Limits::default())?,
        Value::Int(9)
    );
    Ok(())
}

#[test]
fn native_signed_token_spacing_streams_preserve_quoted_unicode_and_control_bytes() -> Result<()> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../reference/token_spacing/cases.json"))?;
    for case in fixture["cases"].as_array().unwrap() {
        let mut actual = Vec::new();
        for lexeme in compile::lex(
            case["source"].as_str().unwrap(),
            "spacing.c".into(),
            CompileLimits::default(),
        )? {
            let (kind, text) = match lexeme.token {
                compile::Token::End => continue,
                compile::Token::Word(s) | compile::Token::Symbol(s) => ("word", s),
                compile::Token::Literal(Value::String(s)) => ("string", s),
                compile::Token::Literal(Value::Int(n)) => ("int", n.to_string()),
                token => panic!("unexpected spacing fixture token: {token:?}"),
            };
            actual.push(serde_json::json!({"kind":kind,"text":text}));
        }
        assert_eq!(
            serde_json::json!(actual),
            case["expected"],
            "{}",
            case["source"]
        );
    }
    let source = "int main(){\u{a0} int n=7;\r\n\u{a0}\u{a0}\treturn n;}";
    assert_eq!(
        Program::compile(source)?.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert!(Program::compile("int main(){\0return 7;}").is_err());
    Ok(())
}

#[test]
fn switch_retains_one_selector_and_stops_at_the_first_matching_explicit_break() -> Result<()> {
    struct Selector(usize);
    impl Host for Selector {
        fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
            assert_eq!(name, "select");
            self.0 += 1;
            Ok(Value::Int(2))
        }
    }
    let p = Program::compile(
        "native int Select(); int main(){int n=0;switch(Select()){case 1:n=10;break;case 2:n=20;break case 2:n=30;break default:n=40;break}return n;}",
    )?;
    let mut host = Selector(0);
    assert_eq!(
        p.call("main", &[], &mut host, Limits::default())?,
        Value::Int(20)
    );
    assert_eq!(host.0, 1);
    Ok(())
}

#[test]
fn large_switch_tables_use_the_shared_source_and_execution_budgets() -> Result<()> {
    use std::fmt::Write;
    let mut source = String::from("int main(int model){switch(model){");
    for model in 0..464 {
        write!(&mut source, "case {model}:return {model};break;")?;
    }
    source.push_str("}return -1;}");
    let program = Program::compile(&source)?;
    let args = [Value::Int(463)];
    assert_eq!(
        program.call("main", &args, &mut (), Limits::default())?,
        Value::Int(463)
    );
    let limits = Limits {
        instructions: 100,
        ..Limits::default()
    };
    let error = program.call("main", &args, &mut (), limits).unwrap_err();
    assert!(format!("{error:#}").contains("script instruction budget exhausted"));
    Ok(())
}

#[test]
fn switch_string_define_boolean_default_and_empty_paths_keep_native_comparisons() -> Result<()> {
    let p = Program::compile(
        r#"
        #define HIT "Sails"
        int main(){int n=0;switch("sAILS"){case HIT:n=7;break;default:n=9;break;}
          switch(19){case 1:n=0;break;default:n+=2;break;}
          switch(true){case true:n+=3;break;}
          switch(999){case 0:n=0;break;}
          switch(n){}return n;}
    "#,
    )?;
    assert_eq!(
        p.call("main", &[], &mut (), Limits::default())?,
        Value::Int(12)
    );
    for (selector, case) in [
        ("\"12tail\"", "12"),
        ("12", "\"12tail\""),
        ("1.5", "1"),
        ("1", "1.5"),
    ] {
        let equal = eval("int", &format!("{case}=={selector}"))?;
        let p = Program::compile(&format!(
            "int main(){{switch({selector}){{case {case}:return 1;break;default:return 0;break;}}}}"
        ))?;
        let actual = p.call("main", &[], &mut (), Limits::default())?;
        assert_eq!(actual, equal);
    }
    Ok(())
}

#[test]
fn nested_switch_breaks_and_loop_continues_release_only_exited_selectors() -> Result<()> {
    let p = Program::compile(
        r#"
        int main(){int n=0;int sum=0;while(n<1200){n++;
          switch(n%2){case 0:switch(1){case 1:continue;break;}break;
            default:int k=0;while(k<3){k++;if(k==2)break;}sum+=k;break;}
        }return sum;}
    "#,
    )?;
    assert_eq!(
        p.call("main", &[], &mut (), Limits::default())?,
        Value::Int(1200)
    );
    let p = Program::compile(
        "int main(){int n=0;switch(1){case 1:while(n<3){n++;continue;}switch(2){case 2:n+=4;break;}n+=5;break;}return n;}",
    )?;
    assert_eq!(
        p.call("main", &[], &mut (), Limits::default())?,
        Value::Int(12)
    );
    Ok(())
}

#[test]
fn switch_failures_and_early_returns_do_not_leak_operand_state_between_calls() -> Result<()> {
    let p = Program::compile(
        "int main(int n){switch(n){case 0:return 1/0;break;case 1:return 7 break;default:return 9;break;}}",
    )?;
    assert!(
        p.call("main", &[Value::Int(0)], &mut (), Limits::default())
            .is_err()
    );
    for n in 1..5 {
        assert_eq!(
            p.call("main", &[Value::Int(n)], &mut (), Limits::default())?,
            Value::Int(if n == 1 { 7 } else { 9 })
        );
    }
    Ok(())
}

#[test]
fn unsupported_switch_forms_fail_with_locations_instead_of_c_fallthrough() {
    for source in [
        "int main(){switch(1){case 1:return 1;}}",
        "int main(){switch(1){case 1:case 2:break;}}",
        "int main(){switch(1){default:break;case 1:break;}}",
        "int main(){switch(1){case 1+2:break;}}",
        "int main(){switch(1){case -1:break;}}",
        "int main(){switch(1){case 1:continue;break;}}",
        "int main(){switch(1){case 1 break;}}",
    ] {
        let error = Program::compile(source).err().expect(source);
        assert!(format!("{error:#}").contains("<source>:1:"), "{error:#}");
    }
}

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
    assert_eq!(eval("int", r#""1.5tail" == 1.5"#)?, Value::Int(1));
    assert_eq!(eval("int", r#"4 == """#)?, Value::Int(0));
    assert_eq!(eval("int", r#"1.5 != "2""#)?, Value::Int(0));
    assert_eq!(eval("int", r#"" 12" == 0"#)?, Value::Int(1));
    assert_eq!(eval("string", r#""a\nb""#)?, Value::String("a\\nb".into()));
    assert!(eval("string", r#"1.0 + "s""#).is_err());
    assert_eq!(eval("int", r#""я" == "Я""#)?, Value::Int(0));
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
