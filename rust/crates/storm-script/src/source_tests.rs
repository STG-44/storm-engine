use super::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Default)]
struct Memory {
    files: BTreeMap<String, String>,
    reads: Vec<String>,
}
impl Memory {
    fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            reads: Vec::new(),
        }
    }
}
impl SourceLoader for Memory {
    fn load(&mut self, path: &str) -> Result<String> {
        self.reads.push(path.into());
        self.files
            .get(path)
            .cloned()
            .with_context(|| format!("missing test source {path}"))
    }
}
fn evaluate(source: &str) -> Result<Value> {
    Program::compile(source)?.call("main", &[], &mut (), Limits::default())
}

#[test]
fn defines_are_typed_case_insensitive_constants_and_do_not_replace_names_or_strings() -> Result<()>
{
    assert_eq!(
        evaluate(
            r#"
        #define Number -7;
        #define Factor -1.5
        #define Label "Number #include"
        int main() {
            if(Factor != -1.5) return -1;
            if(Label != "Number #include") return -2;
            object o; o.Number=3;
            int before = ReadConstant();
            int number=9;
            return before+number+sti(o.Number);
        }
        int ReadConstant(){return NUMBER;}
    "#
        )?,
        Value::Int(5)
    );
    assert_eq!(
        evaluate("int main(){return AFTER;} #define AFTER 19")?,
        Value::Int(19)
    );
    assert_eq!(
        evaluate("#define N 3 int N=8; int main(){return n;}")?,
        Value::Int(8)
    );
    Ok(())
}

#[test]
fn named_array_dimensions_require_an_earlier_int_define_even_when_functions_resolve_later()
-> Result<()> {
    assert_eq!(
        evaluate("#define N 3 int a[N]; int main(){int n=8; int b[N]; b[2]=7; return b[2];}")?,
        Value::Int(7)
    );
    for source in [
        "int a[N]; #define N 3",
        "void main(){int a[N];} #define N 3",
        "#define N 3.0 int a[N];",
        "#define N -1 int a[N];",
        "#define N 4097 int a[N];",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    let mut loader = Memory::new(&[
        ("main.c", "#include \"sizes.h\" int a[N];"),
        ("sizes.h", "#define N 3"),
    ]);
    assert!(Program::compile_with_loader("main.c", &mut loader).is_err());
    Ok(())
}

#[test]
fn includes_append_in_fifo_order_and_deduplicate_case_slashes_dot_paths_and_cycles() -> Result<()> {
    let mut loader = Memory::new(&[
        (
            "main.c",
            r#"#include "a.c" #include "B.C" #include "./A.C" int main(){return Result();}"#,
        ),
        ("a.c", r#"#include "Nested\C.c" #define COUNT 3"#),
        (
            "b.c",
            r#"#include "MAIN.C" int values[COUNT]; int Result(){values[2]=7; return values[2]+LATER;}"#,
        ),
        ("nested/c.c", r#"#include "b.c" #define LATER 5"#),
    ]);
    let p = Program::compile_with_loader("MAIN.C", &mut loader)?;
    assert_eq!(p.sources(), ["main.c", "a.c", "b.c", "nested/c.c"]);
    assert_eq!(loader.reads, p.sources());
    assert_eq!(
        p.call("main", &[], &mut (), Limits::default())?,
        Value::Int(12)
    );
    let mut scoped = Memory::new(&[
        (
            "main.c",
            "int hits; int main(){if(0) #include \"empty.c\"; hits++; return hits;}",
        ),
        ("empty.c", ""),
    ]);
    assert_eq!(
        Program::compile_with_loader("main.c", &mut scoped)?.call(
            "main",
            &[],
            &mut (),
            Limits::default()
        )?,
        Value::Int(1)
    );
    Ok(())
}

#[test]
fn memory_compilation_never_reads_includes_and_directives_inside_comments_and_strings_stay_literal()
-> Result<()> {
    assert_eq!(
        evaluate(
            r##"// #include "missing.c"
        /* #define BAD 2 */
        string main(){return "#include #define";}
    "##
        )?,
        Value::String("#include #define".into())
    );
    let error = Program::compile("#include \"missing.c\"").err().unwrap();
    assert!(format!("{error:#}").contains("explicit SourceLoader"));
    Ok(())
}

#[test]
fn malformed_unsupported_and_duplicate_directives_fail_explicitly() {
    for text in [
        "#define N",
        "#define N true",
        "#define N Other",
        "#define N(x) x",
        "#define N (1+2)",
        "#define N 1 #define n 2",
        "#define N -\"text\"",
        "#ifdef X",
        "#event_handler(\"a\",\"b\")",
        "#include <file.c>",
        "#include 3",
        "#include",
        "void main(){#define N 3}",
        "#define N 3 int value=N;",
        "#define N 3 void main(){N=4;}",
    ] {
        assert!(Program::compile(text).is_err(), "{text}");
    }
}

#[test]
fn compile_runtime_and_missing_include_errors_keep_the_source_file_and_line() -> Result<()> {
    let mut files = Memory::new(&[
        ("main.c", "#include \"bad.c\" int main(){return fail();}"),
        ("bad.c", "// first\r\nint fail(){\r\nreturn missing;\r\n}"),
    ]);
    let error = Program::compile_with_loader("main.c", &mut files)
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("bad.c:3:"), "{error:#}");
    files.files.insert(
        "bad.c".into(),
        "// first\nint fail(){\nreturn 1/0;\n}".into(),
    );
    let p = Program::compile_with_loader("main.c", &mut files)?;
    let error = p.call("main", &[], &mut (), Limits::default()).unwrap_err();
    assert!(format!("{error:#}").contains("bad.c:3:"), "{error:#}");
    let mut missing = Memory::new(&[("main.c", "\n#include \"missing.c\"")]);
    let error = Program::compile_with_loader("main.c", &mut missing)
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("main.c:2:1"), "{error:#}");
    Ok(())
}

#[test]
fn invalid_virtual_paths_fail_before_the_loader_can_read_them() {
    for path in [
        "",
        "/a.c",
        "../a.c",
        "a/../b.c",
        "C:\\a.c",
        "\\\\server\\a.c",
        "a//b.c",
        "é.c",
        "a\0.c",
    ] {
        let mut loader = Memory::default();
        assert!(
            Program::compile_with_loader(path, &mut loader).is_err(),
            "{path:?}"
        );
        assert!(loader.reads.is_empty());
    }
    let mut loader = Memory::new(&[("main.c", "#include \"../outside.c\"")]);
    assert!(Program::compile_with_loader("main.c", &mut loader).is_err());
    assert_eq!(loader.reads, ["main.c"]);
}

#[test]
fn file_byte_token_and_constant_caps_cover_the_entire_source_collection() -> Result<()> {
    let mut files = Memory::default();
    for i in 0..65 {
        files
            .files
            .insert(format!("f{i}.c"), format!("#include \"f{}.c\"", i + 1));
    }
    assert!(
        format!(
            "{:#}",
            Program::compile_with_loader("f0.c", &mut files)
                .err()
                .unwrap()
        )
        .contains("file count limit")
    );
    assert_eq!(files.reads.len(), 64);
    let mut files = Memory::new(&[("main.c", "#include \"more.c\"")]);
    files.files.insert("more.c".into(), " ".repeat(262145));
    assert!(
        format!(
            "{:#}",
            Program::compile_with_loader("main.c", &mut files)
                .err()
                .unwrap()
        )
        .contains("byte limit")
    );
    files.files.insert(
        "main.c".into(),
        format!("#include \"more.c\" {}", ";".repeat(9000)),
    );
    files.files.insert("more.c".into(), ";".repeat(9000));
    assert!(
        format!(
            "{:#}",
            Program::compile_with_loader("main.c", &mut files)
                .err()
                .unwrap()
        )
        .contains("token limit across files")
    );
    let mut files = Memory::default();
    for i in 0..5 {
        files.files.insert(
            format!("f{i}.c"),
            format!("#include \"f{}.c\" /*{}*/", i + 1, " ".repeat(250000)),
        );
    }
    assert!(
        format!(
            "{:#}",
            Program::compile_with_loader("f0.c", &mut files)
                .err()
                .unwrap()
        )
        .contains("byte limit")
    );
    let constants = (0..1025)
        .map(|i| format!("#define C{i} {i}\n"))
        .collect::<String>();
    assert!(
        format!("{:#}", Program::compile(&constants).err().unwrap()).contains("constant limit")
    );
    Ok(())
}

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "storm-source-{}-{}",
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
fn directory_loader_folds_case_and_uses_the_root_for_nested_includes() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::create_dir(temp.0.join("Nested"))?;
    std::fs::write(
        temp.0.join("Main.C"),
        r#"#include "Nested\Child.C" int main(){return Value();}"#,
    )?;
    std::fs::write(
        temp.0.join("Nested/Child.C"),
        r#"#include "Shared.C" int Value(){return Shared();}"#,
    )?;
    std::fs::write(temp.0.join("Shared.C"), "int Shared(){return 7;}")?;
    let p = Program::compile_with_loader("MAIN.c", &mut DirectorySource::new(&temp.0)?)?;
    assert_eq!(
        p.call("main", &[], &mut (), Limits::default())?,
        Value::Int(7)
    );
    assert_eq!(p.sources(), ["main.c", "nested/child.c", "shared.c"]);
    Ok(())
}

#[test]
fn directory_loader_rejects_non_sources_and_ambiguous_case() -> Result<()> {
    let temp = Temporary::new()?;
    std::fs::write(temp.0.join("invalid.c"), [255, 254])?;
    std::fs::write(temp.0.join("large.c"), vec![b' '; 262145])?;
    std::fs::create_dir(temp.0.join("folder.c"))?;
    let mut loader = DirectorySource::new(&temp.0)?;
    for name in ["invalid.c", "large.c", "folder.c", "missing.c"] {
        assert!(loader.load(name).is_err(), "{name}");
    }
    std::fs::write(temp.0.join("Case.c"), "int main(){return 1;}")?;
    std::fs::write(temp.0.join("case.c"), "int main(){return 2;}")?;
    if std::fs::read_dir(&temp.0)?
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("case.c")
        })
        .count()
        > 1
    {
        assert!(loader.load("case.c").is_err());
    }
    Ok(())
}

#[test]
fn directory_loader_keeps_symlinks_inside_the_explicit_root() -> Result<()> {
    #[cfg(unix)]
    {
        let root = Temporary::new()?;
        let outside = Temporary::new()?;
        std::fs::write(outside.0.join("secret.c"), "int main(){return 1;}")?;
        std::os::unix::fs::symlink(&outside.0, root.0.join("escape"))?;
        std::fs::write(root.0.join("local.c"), "int main(){return 7;}")?;
        std::os::unix::fs::symlink(root.0.join("local.c"), root.0.join("alias.c"))?;
        let mut loader = DirectorySource::new(&root.0)?;
        assert!(loader.load("escape/secret.c").is_err());
        assert_eq!(loader.load("alias.c")?, "int main(){return 7;}");
    }
    Ok(())
}

#[test]
fn four_file_ship_fixture_preserves_the_single_file_result_across_scheduler_frames() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/multi_ship");
    let mut runtime = Runtime::compile_with_loader("MAIN.C", &mut DirectorySource::new(root)?)?;
    assert_eq!(
        runtime.sources(),
        [
            "main.c",
            "data/characters.c",
            "handlers/sails.c",
            "helpers/bits.c"
        ]
    );
    let mut host = preview::AttributeHost::new()?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(6));
    assert_eq!(host.report(&result)["ship_attribute_notifications"], 0);
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 0);
    assert_eq!(runtime.advance_frame(0, &mut host, Limits::default())?, 1);
    let mut legacy = Runtime::compile(include_str!("../fixtures/global_ships.c"))?;
    let mut other = preview::AttributeHost::new()?;
    let expected = legacy.call("main", &[], &mut other, Limits::default())?;
    for _ in 0..2 {
        legacy.advance_frame(0, &mut other, Limits::default())?;
    }
    assert_eq!(host.report(&result), other.report(&expected));
    assert_eq!(runtime.completed_handlers(), 3);
    Ok(())
}

#[test]
fn source_registration_matches_unedited_retained_lexer_tables_and_compiler_case_bodies()
-> Result<()> {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../reference/sources/cases.json"))?;
    let mut checked = 0;
    for case in reference["cases"].as_array().unwrap() {
        let mut files = Memory {
            files: case["files"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, text)| (name.clone(), text.as_str().unwrap().into()))
                .collect(),
            reads: Vec::new(),
        };
        let expected = &case["expected"];
        if expected["ok"] == false {
            assert!(Program::compile_with_loader("main.c", &mut files).is_err());
        } else {
            let sources = source::load("main.c", &mut files)?;
            assert_eq!(serde_json::json!(sources.files), expected["files"]);
            let definitions = sources
                .tokens
                .iter()
                .filter_map(|token| {
                    let compile::Token::Definition(name, value) = &token.token else {
                        return None;
                    };
                    Some(match value {
                        Value::Int(n) => serde_json::json!([name, "i", n]),
                        Value::Float(n) => serde_json::json!([name, "f", n.to_bits()]),
                        _ => unreachable!(),
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(serde_json::json!(definitions), expected["defines"]);
            Program::compile_with_loader("main.c", &mut files)?;
        }
        checked += 1;
    }
    assert_eq!(checked, 21);
    Ok(())
}
