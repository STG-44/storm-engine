use crate::{Limits, Program, Runtime, Value, preview::AttributeHost};
use anyhow::Result;

fn run(source: &str) -> Result<Value> {
    Program::compile(source)?.call("main", &[], &mut (), Limits::default())
}

#[test]
fn address_arguments_update_scalars_and_capture_resolved_refs_before_later_arguments() -> Result<()>
{
    assert_eq!(
        run("void set(ref r){r=19;}int main(){int n=1;set(&n);return n;}")?,
        Value::Int(19)
    );
    assert_eq!(
        run(r#"int a=1,b=2;ref current;
        int rebind(){makeref(current,b);return 0;}
        int set(ref target,int ignored){target=7;return 0;}
        int main(){makeref(current,a);set(&current,rebind());return a*10+b;}"#)?,
        Value::Int(72)
    );
    assert_eq!(
        run(r#"int main(){string text="19rest";return sti(&text);}"#)?,
        Value::Int(19)
    );
    Ok(())
}

#[test]
fn object_array_and_aref_addresses_update_actual_entity_destinations() -> Result<()> {
    let mut host = AttributeHost::new()?;
    let mut vm = Runtime::compile(
        r#"object ships[2];int count=0;
        int choose(){count++;return 1;}
        int main(){CreateEntity(&ships[choose()],"script_attribute_observer");
            aref a,old;makearef(a,ships[0].child);old=a;
            CreateEntity(&a,"script_attribute_observer");
            if(IsEntity(old)!=0 || IsEntity(ships[0])!=0)return -1;
            return count*100+IsEntity(ships[1])*10+IsEntity(a);}"#,
    )?;
    assert_eq!(
        vm.call("main", &[], &mut host, Limits::default())?,
        Value::Int(111)
    );
    Ok(())
}

#[test]
fn array_addresses_support_resize_and_l2_indices_without_extending_removed_cell_lifetimes()
-> Result<()> {
    assert_eq!(
        run(
            "int values[2]={1,2};void set(ref r){r=19;}int main(){set(&values[(1<2)]);SetArraySize(&values,3);return values[1]+values[2]+GetArraySize(&values);}"
        )?,
        Value::Int(22)
    );
    let error = run(
        "int values[2];int main(){ref old;old=&values[1];SetArraySize(&values,1);return sti(old);}",
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("expired ref"));
    Ok(())
}

#[test]
fn retained_address_grammar_and_checked_runtime_errors_reject_invalid_targets() -> Result<()> {
    for source in [
        "int main(){return &19;}",
        "int main(){int n;return &(n);}",
        "int main(){object o;return &o.child;}",
        "int f(){return 1;}ref main(){return &f();}",
        "int main(){int n;return &n+1;}",
        "int main(){int n;return (&n);}",
        "int main(){int n;return 1+&n;}",
        "ref main(){int v[2];return &v[1==1];}",
        "ref main(){int v[2];return &v[1<2];}",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    for source in [
        "int main(){ref r;return sti(&r);}",
        "int main(){int n;ref r;r=&n[0];return 0;}",
        "int main(){int v[2];ref r;r=&v[-1];return 0;}",
        "int main(){int v[2];ref r;r=&v[1.0];return 0;}",
    ] {
        let error = format!("{:#}", run(source).unwrap_err());
        assert!(error.contains("main <source>:"), "{error}");
    }
    Ok(())
}

#[test]
fn escaped_addresses_are_weak_and_runtime_globals_keep_their_original_lifetimes() -> Result<()> {
    let error =
        run("ref escape(){int local=7;return &local;}int main(){ref r;r=escape();return sti(r);}")
            .unwrap_err();
    assert!(format!("{error:#}").contains("expired ref"));
    let source = "int value=7;ref alias(){return &value;}";
    let transient = Program::compile(source)?.call("alias", &[], &mut (), Limits::default())?;
    assert!(transient.resolved().is_err());
    let mut runtime = Runtime::compile(source)?;
    let persistent = runtime.call("alias", &[], &mut (), Limits::default())?;
    assert_eq!(persistent.resolved()?, Value::Int(7));
    drop(runtime);
    assert!(persistent.resolved().is_err());
    Ok(())
}

#[test]
fn original_address_call_spelling_reproduces_the_complete_creation_report() -> Result<()> {
    let source = include_str!("../fixtures/entity_creation.c");
    let mut normal = Runtime::compile(source)?;
    let mut normal_host = AttributeHost::new()?;
    let result = normal.call("main", &[], &mut normal_host, Limits::default())?;
    let mut address = Runtime::compile(source)?;
    let mut address_host = AttributeHost::new()?;
    let other = address.call("AddressMain", &[], &mut address_host, Limits::default())?;
    assert_eq!(result, Value::Int(19));
    assert_eq!(other, result);
    assert_eq!(normal_host.report(&result), address_host.report(&other));
    Ok(())
}
