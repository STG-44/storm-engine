use super::*;
use std::cell::Cell;
#[test]
fn destruction_keeps_lookup_binding_and_drains_nested_erasures_before_slot_reuse() {
    struct Actor {
        child: EntityId,
        root: AttributeId,
        observed: Rc<Cell<bool>>,
    }
    impl Entity for Actor {
        fn destroy(&mut self, context: &mut EntityDestroy<'_>) -> Result<()> {
            assert!(!context.manager.is_valid(context.id));
            assert!(context.manager.get(context.id).is_some());
            assert!(context.manager.get(context.id).unwrap().borrow().is_err());
            assert_eq!(context.attributes, Some(self.root));
            assert_eq!(context.manager.attribute_root(context.id), Some(self.root));
            assert!(context.manager.layer_ids(LayerType::Execute).is_empty());
            context.manager.erase(self.child);
            context.manager.new_lifecycle()?; // defer recursive entry
            assert!(context.manager.get(self.child).is_some());
            self.observed.set(true);
            Ok(())
        }
    }
    struct Empty;
    impl Entity for Empty {}
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    let child = manager.insert("model", Empty).unwrap();
    let observed = Rc::new(Cell::new(false));
    let ship = manager
        .insert(
            "ship",
            Actor {
                child,
                root,
                observed: observed.clone(),
            },
        )
        .unwrap();
    manager.bind_attributes(ship, &store, Some(root)).unwrap();
    manager.set_layer_type(0, LayerType::Execute).unwrap();
    manager.add_to_layer(0, ship, 2).unwrap();
    manager.erase(ship);
    manager.new_lifecycle().unwrap();
    assert!(observed.get());
    assert!(manager.get(ship).is_none() && manager.get(child).is_none());
    let replacement = manager.insert("new", Empty).unwrap();
    assert_eq!(replacement.raw() as u32, child.raw() as u32);
    assert_ne!(replacement, child);
}
#[test]
fn teardown_errors_release_once_and_do_not_strand_queued_dependencies() {
    struct Actor {
        child: EntityId,
        calls: Rc<Cell<u32>>,
    }
    impl Entity for Actor {
        fn destroy(&mut self, c: &mut EntityDestroy<'_>) -> Result<()> {
            self.calls.set(self.calls.get() + 1);
            c.manager.erase(self.child);
            anyhow::bail!("checked teardown error")
        }
    }
    struct Empty;
    impl Entity for Empty {}
    let mut m = EntityManager::default();
    let child = m.insert("child", Empty).unwrap();
    let calls = Rc::new(Cell::new(0));
    let id = m
        .insert(
            "ship",
            Actor {
                child,
                calls: calls.clone(),
            },
        )
        .unwrap();
    m.erase(id);
    assert!(m.new_lifecycle().is_err());
    assert!(m.get(id).is_none() && m.get(child).is_none());
    m.new_lifecycle().unwrap();
    assert_eq!(calls.get(), 1);
}
