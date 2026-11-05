use super::*;

fn run(source: &str) -> Result<Value> {
    Program::compile(source)?.call("main", &[], &mut (), Limits::default())
}

#[test]
fn scalar_assignment_converts_to_current_type_but_compound_operations_can_promote() -> Result<()> {
    assert_eq!(run("int main(){int n=3.9; return n;}")?, Value::Int(3));
    assert_eq!(
        run("float main(){float n; n=3; return n;}")?,
        Value::Float(3.0)
    );
    assert_eq!(
        run("float main(){int n=3; n+=0.5; return n;}")?,
        Value::Float(3.5)
    );
    assert_eq!(
        run("string main(){string n; n=19; return n;}")?,
        Value::String("19".into())
    );
    assert_eq!(
        run("int main(){int n=\"19tail\";return n;}")?,
        Value::Int(19)
    );
    Ok(())
}

#[test]
fn reference_operands_preserve_original_arithmetic_and_conversion_side_effects() -> Result<()> {
    assert_eq!(
        run("int main(){int n=3;ref r;makeref(r,n);int value=r+2;return n*10+value;}")?,
        Value::Int(55)
    );
    assert_eq!(
        run("int main(){int n=3;ref r;makeref(r,n);int value=-r;return n*10+value;}")?,
        Value::Int(-33)
    );
    assert_eq!(
        run("int main(){int n=2;ref r;makeref(r,n);int value=r && 0;return n*10+value;}")?,
        Value::Int(10)
    );
    assert_eq!(
        run("int main(){int n=2;ref r;makeref(r,n);int value=r || 0;return n*10+value;}")?,
        Value::Int(11)
    );
    assert_eq!(
        run("int main(){float n=0.5;ref r;makeref(r,n);if(r)return 9;return n;}")?,
        Value::Int(0)
    );
    assert_eq!(
        run("int main(){float n=3.5;ref r;makeref(r,n);int value=sti(r);return n*10+value;}")?,
        Value::Int(33)
    );
    assert_eq!(
        run("string main(){int n=3;ref r;makeref(r,n);object o;o.hd=r;return n;}")?,
        Value::String("3".into())
    );
    assert!(run("int main(){int n=3;ref r;makeref(r,n);return !r;}").is_err());
    assert!(run("int main(){int n=3;ref r;makeref(r,n);return 2^r;}").is_err());
    Ok(())
}

#[test]
fn reference_assignment_rebinds_and_function_calls_mutate_the_original_scalar() -> Result<()> {
    let source = "void add(ref p){p+=2;} int main(){int a=3,b=20;ref r,s;makeref(r,a);makeref(s,b);add(r);r=s;r=7;return a*10+b;}";
    assert_eq!(run(source)?, Value::Int(57));
    assert_eq!(
        run("int main(){int n=3;ref r;makeref(r,n);r++;r--;r=8.5;return n;}")?,
        Value::Int(8)
    );
    Ok(())
}

#[test]
fn object_parameters_copy_while_reference_parameters_alias() -> Result<()> {
    let source = r#"
        void copied(object value){value.hd=1;}
        void aliased(ref value){value.hd=274;}
        int main(){object ship;ship.hd=548;copied(ship);if(sti(ship.hd)!=548)return -1;
            ref r;makeref(r,ship);aliased(r);return sti(ship.hd);}
    "#;
    assert_eq!(run(source)?, Value::Int(274));
    assert_eq!(
        run(
            "int main(){object blank,target;target.hd=548;ref empty;makeref(empty,blank);target=empty;return sti(target.hd);}"
        )?,
        Value::Int(548)
    );
    assert_eq!(
        run(
            "int main(){object blank,target;target.hd=548;target=blank;return CheckAttribute(target,\"hd\");}"
        )?,
        Value::Int(0)
    );
    Ok(())
}

#[test]
fn fixed_and_dynamic_paths_keep_original_creation_and_value_rules() -> Result<()> {
    let source = r#"
        int main(){object ship;aref a,b;string path="sails.rey_a1.1";
            makearef(a,ship.fixed); makearef(b,ship.(path));
            if(CheckAttribute(ship,"sails.rey_a1.1")!=1)return -1;
            a.hd=548;b.hd=274;string key="sails.rey_a1.1.hd";
            return sti(ship.fixed.hd)+sti(ship.(key));}
    "#;
    assert_eq!(run(source)?, Value::Int(822));
    assert_eq!(
        run("int main(){object o;aref a;makearef(a,o.fixed);return sti(a);}")?,
        Value::Int(0)
    );
    assert!(
        run("int main(){object o;aref a;string p=\"dynamic\";makearef(a,o.(p));return sti(a);}")
            .is_err()
    );
    assert_eq!(
        run("string main(){int key=3;ref r;makeref(r,key);object o;o.(r)=1;return key;}")?,
        Value::String("3".into())
    );
    assert_eq!(
        run(
            "int update(ref key){key=\"after\";return 19;} int main(){object o;string key=\"before\";ref r;makeref(r,key);o.(key)=update(r);return CheckAttribute(o,\"before\")*10+CheckAttribute(o,\"after\");}"
        )?,
        Value::Int(10)
    );
    assert!(run("int main(){object o;return sti(o.missing);}").is_err());
    Ok(())
}

#[test]
fn copying_into_an_existing_object_preserves_root_alias_and_invalidates_old_children() -> Result<()>
{
    let source = "int main(){object a,b;b.old=1;a.new=2;aref root;makearef(root,b);b=a;return sti(root.new)+CheckAttribute(root,\"old\");}";
    assert_eq!(run(source)?, Value::Int(2));
    let source =
        "int main(){object a,b;b.old=1;a.new=2;aref old;makearef(old,b.old);b=a;return sti(old);}";
    assert!(format!("{:#}", run(source).unwrap_err()).contains("stale attribute"));
    Ok(())
}

#[test]
fn attribute_enumeration_copy_and_delete_use_original_builtin_names() -> Result<()> {
    let source = r#"
        int main(){object source,dest;source.first=12;source.second=19;ref r;makeref(r,dest);
            CopyAttributes(r,source);aref root;makearef(root,dest);
            aref child;child=GetAttributeN(root,0);
            if(GetAttributeName(child)!="first")return -1;
            if(GetAttributeValue(child)!="12")return -2;
            DeleteAttribute(root,"first");if(CheckAttribute(root,"first"))return -3;
            return GetAttributesNum(root)+CheckAttribute(source,"first");}
    "#;
    assert_eq!(run(source)?, Value::Int(2));
    assert!(
        run("int main(){object o;aref a;makearef(a,o);a=GetAttributeN(a,-1);return 0;}").is_err()
    );
    Ok(())
}

#[test]
fn expired_references_and_uninitialized_aliases_fail_safely() -> Result<()> {
    for source in [
        "int main(){ref r;return sti(r);}",
        "int main(){aref a;return sti(a);}",
        "ref bad(){int n=3;ref r;makeref(r,n);return r;}int main(){ref r;r=bad();return sti(r);}",
        "aref bad(){object o;o.hd=1;aref a;makearef(a,o);return a;}int main(){aref a;a=bad();return sti(a);}",
        "int main(){object o;aref root,a;makearef(root,o);makearef(a,o.hd);DeleteAttribute(root,\"hd\");return sti(a);}",
    ] {
        assert!(run(source).is_err(), "{source}");
    }
    assert_eq!(
        run("int main(){ref r;aref a;return CheckAttribute(r,\"x\")+CheckAttribute(a,\"x\");}")?,
        Value::Int(0)
    );
    Ok(())
}

#[test]
fn returning_an_object_transfers_ownership_and_call_cleanup_reclaims_temporary_trees() -> Result<()>
{
    let p = Program::compile("object main(){object o;o.hd=548;return o;}")?;
    let value = p.call("main", &[], &mut (), Limits::default())?;
    assert_eq!(
        value
            .attributes()?
            .walk(&[("hd".into(), false)], false)?
            .value()?,
        "548"
    );
    struct TestHost(World);
    impl Host for TestHost {
        fn world(&self) -> World {
            self.0.clone()
        }
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
            anyhow::bail!("no native")
        }
    }
    let world = World::default();
    let mut host = TestHost(world.clone());
    let p = Program::compile("void main(){object o;o.hd=1;object copy;copy=o;}")?;
    for _ in 0..100 {
        p.call("main", &[], &mut host, Limits::default())?;
        assert_eq!(world.live_nodes()?, 0);
    }
    let p = Program::compile("void main(){object o;o.hd=1;int n=1/0;}")?;
    assert!(p.call("main", &[], &mut host, Limits::default()).is_err());
    assert_eq!(world.live_nodes()?, 0);
    Ok(())
}

#[test]
fn ship_script_uses_refs_original_attribute_syntax_and_entity_notifications() -> Result<()> {
    let p = Program::compile(include_str!("../fixtures/ship_attributes.c"))?;
    let mut host = preview::AttributeHost::new()?;
    let result = p.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(6));
    let report = host.report(&result);
    assert_eq!(report["ship_attribute_notifications"], 7);
    assert_eq!(report["ship_attribute_changes"][5]["value"], "3.2500000");
    assert_eq!(report["ship_attribute_changes"][6]["value"], "1");
    assert_eq!(
        report["script_ship"]["children"][1]["children"][0]["children"][0]["children"][0]["children"]
            [0]["value"],
        "548"
    );
    Ok(())
}

#[test]
fn source_rejects_invalid_alias_commands_and_unsupported_attribute_mutations() {
    for source in [
        "int main(){object o;int n;makeref(n,o);return 0;}",
        "int main(){object o;int n;makearef(n,o);return 0;}",
        "int main(){object o;o.hd+=1;return 0;}",
        "int main(){object o;makearef(o,o);return 0;}",
        "int main(){object o;aref a;makearef(a,o.(\"path\"));return 0;}",
        "int sti(int x){return x;}",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
}

#[test]
fn cyclic_aliases_and_name_budget_fail_without_leaking_or_poisoning_live_trees() -> Result<()> {
    let slot = Variable::new(Value::Reference(Reference::default()));
    slot.assign(slot.reference(), true)?;
    assert!(format!("{:#}", slot.value().unwrap_err()).contains("cyclic ref"));
    let world = World::default();
    let object = world.object(EntityId::INVALID)?;
    let foreign = World::default().object(EntityId::INVALID)?;
    assert!(object.replace(&foreign).is_err());
    let root = object.root();
    let mut rejected = false;
    for n in 0..9000 {
        let path = Value::String(format!("missing{n}"));
        if let Err(error) = root.builtin("checkattribute", Some(&path)) {
            assert!(format!("{error:#}").contains("name budget"));
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    assert_eq!(world.live_nodes()?, 1);
    assert_eq!(root.value()?, "");
    drop(object);
    assert!(root.value().is_err());
    assert_eq!(world.live_nodes()?, 0);
    Ok(())
}
