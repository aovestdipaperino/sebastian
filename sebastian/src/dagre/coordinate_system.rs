//! Port of dagre-d3-es `src/dagre/coordinate-system.js`.

use super::types::LayoutGraph;

pub fn adjust(g: &LayoutGraph) {
    let rank_dir = g.graph().borrow().rankdir.to_lowercase();
    if rank_dir == "lr" || rank_dir == "rl" {
        swap_width_height(g);
    }
}

pub fn undo(g: &LayoutGraph) {
    let rank_dir = g.graph().borrow().rankdir.to_lowercase();
    if rank_dir == "bt" || rank_dir == "rl" {
        reverse_y(g);
    }
    if rank_dir == "lr" || rank_dir == "rl" {
        swap_xy(g);
        swap_width_height(g);
    }
}

fn swap_width_height(g: &LayoutGraph) {
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let mut n = node.borrow_mut();
        let n = &mut *n;
        std::mem::swap(&mut n.width, &mut n.height);
    }
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        let l = &mut *l;
        std::mem::swap(&mut l.width, &mut l.height);
    }
}

fn reverse_y(g: &LayoutGraph) {
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let mut n = node.borrow_mut();
        if let Some(y) = n.y.as_mut() {
            *y = -*y;
        }
    }
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        if let Some(points) = l.points.as_mut() {
            for p in points {
                p.y = -p.y;
            }
        }
        if let Some(y) = l.y.as_mut() {
            *y = -*y;
        }
    }
}

fn swap_xy(g: &LayoutGraph) {
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let mut n = node.borrow_mut();
        let n = &mut *n;
        std::mem::swap(&mut n.x, &mut n.y);
    }
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        let l = &mut *l;
        if let Some(points) = l.points.as_mut() {
            for p in points {
                std::mem::swap(&mut p.x, &mut p.y);
            }
        }
        if l.x.is_some() {
            std::mem::swap(&mut l.x, &mut l.y);
        }
    }
}
