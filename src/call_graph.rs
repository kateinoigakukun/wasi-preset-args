use std::collections::{HashMap, HashSet};
use walrus::{ElementId, ExportId, FunctionId};

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum FunctionUse {
    Call { caller: FunctionId },
    InElement { element: ElementId, index: usize },
    Export { export: ExportId },
}

#[derive(Debug, Default)]
pub struct CallGraph {
    // FIXME: Think more efficient data structure
    callee_to_uses: HashMap<FunctionId, HashSet<FunctionUse>>,
}

impl CallGraph {
    pub fn build_from(module: &walrus::Module) -> Self {
        let mut graph = CallGraph::default();

        // Collect direct calls
        struct CallCollector<'graph> {
            func_id: FunctionId,
            graph: &'graph mut CallGraph,
        }

        impl<'instr> walrus::ir::Visitor<'instr> for CallCollector<'_> {
            fn visit_call(&mut self, instr: &walrus::ir::Call) {
                self.graph.add_use(
                    instr.func,
                    FunctionUse::Call {
                        caller: self.func_id,
                    },
                );
            }
        }
        for (func_id, func) in module.funcs.iter_local() {
            let mut collector = CallCollector {
                graph: &mut graph,
                func_id,
            };
            walrus::ir::dfs_in_order(&mut collector, func, func.entry_block());
        }

        // Collect indirect function table elements
        for element in module.elements.iter() {
            for (index, member) in element.members.clone().iter().enumerate() {
                if let Some(member) = member {
                    graph.add_use(
                        *member,
                        FunctionUse::InElement {
                            element: element.id(),
                            index,
                        },
                    );
                }
            }
        }

        // Collect exports having references to functions
        for export in module.exports.iter() {
            if let walrus::ExportItem::Function(func) = export.item {
                graph.add_use(
                    func,
                    FunctionUse::Export {
                        export: export.id(),
                    },
                )
            }
        }

        graph
    }

    pub fn get_func_uses(&self, func_id: &FunctionId) -> Option<&HashSet<FunctionUse>> {
        self.callee_to_uses.get(func_id)
    }

    pub fn add_use(&mut self, callee: FunctionId, use_entry: FunctionUse) {
        self.callee_to_uses
            .entry(callee)
            .or_default()
            .insert(use_entry);
    }
}

pub fn replace_func_use(
    map: &HashMap<walrus::FunctionId, walrus::FunctionId>,
    module: &mut walrus::Module,
    call_graph: &mut CallGraph,
) {
    let mut func_worklist = HashSet::new();

    for (from, to) in map.iter() {
        let uses = match call_graph.get_func_uses(from) {
            Some(uses) => uses,
            None => continue,
        };

        for func_use in uses {
            match func_use {
                FunctionUse::Call { caller } => {
                    func_worklist.insert(caller);
                }
                FunctionUse::InElement { element, index } => {
                    let element = module.elements.get_mut(*element);
                    element.members[*index] = Some(*to);
                }
                FunctionUse::Export { export } => {
                    let export = module.exports.get_mut(*export);
                    if let walrus::ExportItem::Function(func) = &mut export.item {
                        *func = *to;
                    } else {
                        unreachable!("unexpected non-function export name={}", export.name);
                    }
                }
            }
        }
    }

    struct Replacer<'a> {
        replacing_map: &'a HashMap<walrus::FunctionId, walrus::FunctionId>,
    }

    impl walrus::ir::VisitorMut for Replacer<'_> {
        fn visit_call_mut(&mut self, instr: &mut walrus::ir::Call) {
            if let Some(replacing_id) = self.replacing_map.get(&instr.func) {
                instr.func = *replacing_id;
            }
        }
    }

    for func in func_worklist {
        let func = module.funcs.get_mut(*func).kind.unwrap_local_mut();
        let entry = func.entry_block();
        walrus::ir::dfs_pre_order_mut(&mut Replacer { replacing_map: map }, func, entry);
    }

    // Add replaced use edges to the graph
    for (from, to) in map.iter() {
        let uses = match call_graph.get_func_uses(from) {
            Some(uses) => uses.clone(),
            None => continue,
        };

        for func_use in uses {
            call_graph.add_use(*to, func_use);
        }
    }
}
