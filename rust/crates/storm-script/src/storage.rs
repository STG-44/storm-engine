//! Persistent declarations and bounded array storage. References never own cells.
use crate::{Host, Program, Type, Value, Variable, World};
use anyhow::{Context, Result, ensure};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use storm_core::EntityId;

pub(crate) const MAX_ELEMENTS: usize = 4096;
const MAX_ARRAY_CELLS: usize = 16384;

#[derive(Clone, Default)]
pub(crate) struct Budget(Rc<Cell<usize>>);
impl Budget {
    fn reserve(&self, count: usize) -> Result<()> {
        ensure!(
            count <= MAX_ARRAY_CELLS - self.0.get(),
            "script array cell budget"
        );
        self.0.set(self.0.get() + count);
        Ok(())
    }
}

#[derive(Clone)]
pub struct Array(Rc<ArrayData>);
struct ArrayData {
    kind: Type,
    cells: RefCell<Vec<Variable>>,
    budget: Budget,
    world: World,
}
impl Drop for ArrayData {
    fn drop(&mut self) {
        self.budget
            .0
            .set(self.budget.0.get() - self.cells.get_mut().len());
    }
}
impl std::fmt::Debug for Array {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Array")
            .field("kind", &self.0.kind)
            .finish_non_exhaustive()
    }
}
impl PartialEq for Array {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Array {
    fn new(kind: Type, count: usize, world: &World, budget: &Budget) -> Result<Self> {
        ensure!(
            count > 0 && count <= MAX_ELEMENTS,
            "script array size limit"
        );
        budget.reserve(count)?;
        let cells = (0..count)
            .map(|_| initial(kind, world).map(Variable::new))
            .collect::<Result<_>>();
        match cells {
            Ok(cells) => Ok(Self(Rc::new(ArrayData {
                kind,
                cells: RefCell::new(cells),
                budget: budget.clone(),
                world: world.clone(),
            }))),
            Err(error) => {
                budget.0.set(budget.0.get() - count);
                Err(error)
            }
        }
    }
    pub fn len(&self) -> usize {
        self.0.cells.borrow().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(crate) fn element(&self, index: i32) -> Result<Variable> {
        let index = usize::try_from(index).context("negative array index")?;
        self.0
            .cells
            .borrow()
            .get(index)
            .cloned()
            .context("invalid array index")
    }
    pub(crate) fn resize(&self, count: i32) -> Result<()> {
        let count = usize::try_from(count).context("negative array size")?;
        ensure!(
            count > 0 && count <= MAX_ELEMENTS,
            "script array size limit"
        );
        let old = self.len();
        if count > old {
            let additional = count - old;
            self.0.budget.reserve(additional)?;
            let cells = (0..additional)
                .map(|_| initial(self.0.kind, &self.0.world).map(Variable::new))
                .collect::<Result<Vec<_>>>();
            match cells {
                Ok(cells) => self.0.cells.borrow_mut().extend(cells),
                Err(error) => {
                    self.0.budget.0.set(self.0.budget.0.get() - additional);
                    return Err(error);
                }
            }
        } else if count < old {
            self.0.cells.borrow_mut().truncate(count);
            self.0.budget.0.set(self.0.budget.0.get() - (old - count));
        }
        Ok(())
    }
}

pub(crate) struct Declaration {
    pub kind: Type,
    pub elements: usize,
    pub initial: Vec<Value>,
}
impl Declaration {
    pub fn allocate(&self, world: &World, budget: &Budget) -> Result<Variable> {
        // DATA::SetType treats both [0] and [1] as a scalar.
        let value = if self.elements > 1 {
            let array = Array::new(self.kind, self.elements, world, budget)?;
            for (index, value) in self.initial.iter().enumerate() {
                array.element(index as i32)?.assign(value.clone(), true)?;
            }
            Value::Array(array)
        } else {
            self.initial
                .first()
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| initial(self.kind, world))?
        };
        Ok(Variable::new(value))
    }
}
fn initial(kind: Type, world: &World) -> Result<Value> {
    Ok(if kind == Type::Object {
        Value::Object(world.object(EntityId::INVALID)?)
    } else {
        kind.initial()
    })
}

pub(crate) struct State {
    pub world: World,
    pub budget: Budget,
    pub globals: Vec<Variable>,
    // COMPILER starts with bEntityUpdate=true; calls/events share the flag.
    pub entity_update: Cell<bool>,
}
impl State {
    pub fn new(program: &Program, host: &impl Host) -> Result<Self> {
        let world = host.world();
        let budget = Budget::default();
        let globals = program
            .globals
            .iter()
            .map(|decl| decl.allocate(&world, &budget))
            .collect::<Result<_>>()?;
        Ok(Self {
            world,
            budget,
            globals,
            entity_update: Cell::new(true),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Slot {
    Local(usize),
    Global(usize),
}
impl Slot {
    pub fn variable<'a>(&self, locals: &'a [Variable], globals: &'a [Variable]) -> &'a Variable {
        match self {
            Self::Local(i) => &locals[*i],
            Self::Global(i) => &globals[*i],
        }
    }
}
