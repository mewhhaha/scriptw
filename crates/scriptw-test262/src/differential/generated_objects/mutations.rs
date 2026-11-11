//! One closed mutation algebra owns execution order and all reduction aliases.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Integrity {
    Seal,
    Freeze,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Operation {
    Get {
        target: NodeId,
        key: Key,
        receiver: NodeId,
    },
    Set {
        target: NodeId,
        key: Key,
        value: Value,
        receiver: NodeId,
    },
    Define {
        target: NodeId,
        key: Key,
        value: Value,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    },
    Delete {
        target: NodeId,
        key: Key,
    },
    Has {
        target: NodeId,
        key: Key,
    },
    Prototype {
        target: NodeId,
        prototype: Option<NodeId>,
    },
    PreventExtensions {
        target: NodeId,
    },
    Integrity {
        target: NodeId,
        level: Integrity,
    },
    ArrayLength {
        target: NodeId,
        length: u8,
        writable: bool,
    },
}

impl Operation {
    fn target(&self) -> NodeId {
        match self {
            Self::Get { target, .. }
            | Self::Set { target, .. }
            | Self::Define { target, .. }
            | Self::Delete { target, .. }
            | Self::Has { target, .. }
            | Self::Prototype { target, .. }
            | Self::PreventExtensions { target }
            | Self::Integrity { target, .. }
            | Self::ArrayLength { target, .. } => *target,
        }
    }

    /// This is the only reference traversal. Validation, liveness and remapping
    /// consume it, so adding an operation cannot silently lose one of its aliases.
    fn map_references(
        &mut self,
        mut node: impl FnMut(&mut NodeId),
        mut symbol: impl FnMut(&mut SymbolId),
    ) {
        let (target, key, value, receiver, prototype) = match self {
            Self::Get {
                target,
                key,
                receiver,
            } => (target, Some(key), None, Some(receiver), None),
            Self::Set {
                target,
                key,
                value,
                receiver,
            } => (target, Some(key), Some(value), Some(receiver), None),
            Self::Define {
                target, key, value, ..
            } => (target, Some(key), Some(value), None, None),
            Self::Delete { target, key } | Self::Has { target, key } => {
                (target, Some(key), None, None, None)
            }
            Self::Prototype { target, prototype } => (target, None, None, None, prototype.as_mut()),
            Self::PreventExtensions { target }
            | Self::Integrity { target, .. }
            | Self::ArrayLength { target, .. } => (target, None, None, None, None),
        };
        node(target);
        if let Some(receiver) = receiver {
            node(receiver);
        }
        if let Some(prototype) = prototype {
            node(prototype);
        }
        if let Some(key) = key {
            match key {
                Key::String(_) => {}
                Key::Symbol(id) => symbol(id),
            }
        }
        if let Some(value) = value {
            match value {
                Value::Node(id) => node(id),
                Value::Symbol(id) => symbol(id),
                Value::Undefined
                | Value::Null
                | Value::Boolean(_)
                | Value::Integer(_)
                | Value::NegativeZero
                | Value::NaN
                | Value::String(_)
                | Value::BigInt(_) => {}
            }
        }
    }
    pub(super) fn references(
        &self,
        mut node: impl FnMut(NodeId),
        mut symbol: impl FnMut(SymbolId),
    ) {
        self.clone()
            .map_references(|id| node(*id), |id| symbol(*id));
    }
    pub(super) fn remap(&mut self, nodes: &[Option<NodeId>], symbols: &[Option<SymbolId>]) {
        self.map_references(
            |id| *id = nodes[usize::from(id.0)].expect("live operation node"),
            |id| *id = symbols[usize::from(id.0)].expect("live operation symbol"),
        );
    }
    pub(super) fn valid(&self, nodes: &[Node], symbols: usize) -> bool {
        let mut valid_nodes = true;
        let mut valid_symbols = true;
        self.references(
            |id| valid_nodes &= usize::from(id.0) < nodes.len(),
            |id| valid_symbols &= usize::from(id.0) < symbols,
        );
        if !valid_nodes || !valid_symbols {
            return false;
        }
        match self {
            Self::Set {
                target, key, value, ..
            }
            | Self::Define {
                target, key, value, ..
            } => {
                value.valid(nodes.len(), symbols)
                    && !(nodes[usize::from(target.0)].kind == Kind::Array
                        && *key == Key::String("length"))
            }
            Self::ArrayLength { target, length, .. } => {
                nodes[usize::from(target.0)].kind == Kind::Array && *length <= 8
            }
            Self::Get { .. }
            | Self::Delete { .. }
            | Self::Has { .. }
            | Self::Prototype { .. }
            | Self::PreventExtensions { .. }
            | Self::Integrity { .. } => true,
        }
    }
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Set { value, .. } | Self::Define { value, .. } => value.weight(),
            Self::Prototype { prototype, .. } => usize::from(prototype.is_some()),
            Self::ArrayLength { length, .. } => usize::from(*length),
            Self::Get { .. }
            | Self::Delete { .. }
            | Self::Has { .. }
            | Self::PreventExtensions { .. }
            | Self::Integrity { .. } => 0,
        }
    }
    pub(super) fn reductions(&self) -> Vec<Self> {
        let mut smaller = self.clone();
        match &mut smaller {
            Self::Set { value, .. } | Self::Define { value, .. } if value.weight() > 0 => {
                *value = Value::Undefined
            }
            Self::Prototype {
                prototype: value @ Some(_),
                ..
            } => *value = None,
            Self::ArrayLength { length, .. } if *length != 0 => *length = 0,
            Self::Set { .. }
            | Self::Define { .. }
            | Self::Prototype {
                prototype: None, ..
            }
            | Self::ArrayLength { .. }
            | Self::Get { .. }
            | Self::Delete { .. }
            | Self::Has { .. }
            | Self::PreventExtensions { .. }
            | Self::Integrity { .. } => return vec![],
        }
        vec![smaller]
    }
    pub(super) fn emit(&self, source: &mut String, index: usize) {
        let target = self.target().0;
        let (name, expression) = match self {
            Self::Get { key, receiver, .. } => ("get", format!("Reflect.get(n{target},{},n{})", key.source(), receiver.0)),
            Self::Set { key, value, receiver, .. } => ("set", format!("Reflect.set(n{target},{},{},n{})", key.source(), value.source(), receiver.0)),
            Self::Define { key, value, writable, enumerable, configurable, .. } => ("define", format!(
                "Reflect.defineProperty(n{target},{},{{value:{},writable:{writable},enumerable:{enumerable},configurable:{configurable}}})", key.source(), value.source())),
            Self::Delete { key, .. } => ("delete", format!("Reflect.deleteProperty(n{target},{})", key.source())),
            Self::Has { key, .. } => ("has", format!("Reflect.has(n{target},{})", key.source())),
            Self::Prototype { prototype, .. } => ("prototype", format!("Reflect.setPrototypeOf(n{target},{})",
                prototype.map_or_else(|| "null".into(), |id| format!("n{}", id.0)))),
            Self::PreventExtensions { .. } => ("prevent_extensions", format!("Reflect.preventExtensions(n{target})")),
            Self::Integrity { level, .. } => match level {
                Integrity::Seal => ("seal", format!("Object.seal(n{target})")),
                Integrity::Freeze => ("freeze", format!("Object.freeze(n{target})")),
            },
            Self::ArrayLength { length, writable, .. } => ("array_length", format!(
                "Reflect.defineProperty(n{target},'length',{{value:{length},writable:{writable}}})")),
        };
        // No result is string-coerced. The original v5 observer retains its
        // primitive payload or final object identity alongside the final graph.
        writeln!(
            source,
            "print('step_{index}:{name}');\noperationResults[{index}]={expression};"
        )
        .unwrap();
    }
}

pub(super) fn generate(
    nodes: &[Node],
    symbols: u8,
    steps: u8,
    random: &mut Random,
) -> Vec<Operation> {
    let mut result = Vec::with_capacity(usize::from(steps));
    for _ in 0..steps {
        let target = NodeId((random.next() % nodes.len() as u64) as u8);
        let node = &nodes[usize::from(target.0)];
        let key = if random.boolean() && !node.properties.is_empty() {
            node.properties[(random.next() % node.properties.len() as u64) as usize].key
        } else if random.boolean() {
            Key::Symbol(SymbolId((random.next() % u64::from(symbols)) as u8))
        } else {
            Key::String(["mutation", "0", "2", "4294967294"][(random.next() % 4) as usize])
        };
        let receiver = NodeId((random.next() % nodes.len() as u64) as u8);
        let operation = match random.next() % 10 {
            0 => Operation::Get {
                target,
                key,
                receiver,
            },
            1 => Operation::Set {
                target,
                key,
                value: random.value(nodes.len() as u8, symbols),
                receiver,
            },
            2 => Operation::Define {
                target,
                key,
                value: random.value(nodes.len() as u8, symbols),
                writable: random.boolean(),
                enumerable: random.boolean(),
                configurable: random.boolean(),
            },
            3 => Operation::Delete { target, key },
            4 => Operation::Has { target, key },
            5 => Operation::Prototype {
                target,
                prototype: random.boolean().then_some(receiver),
            },
            6 => Operation::PreventExtensions { target },
            7 => Operation::Integrity {
                target,
                level: Integrity::Seal,
            },
            8 => Operation::Integrity {
                target,
                level: Integrity::Freeze,
            },
            9 if node.kind == Kind::Array => Operation::ArrayLength {
                target,
                length: (random.next() % 9) as u8,
                writable: random.boolean(),
            },
            9 => Operation::Get {
                target,
                key,
                receiver: target,
            },
            _ => unreachable!(),
        };
        result.push(operation);
    }
    result
}
