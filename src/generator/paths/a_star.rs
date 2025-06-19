use std::{fmt::Debug, hash::Hash};

pub async fn a_star<TNode>(
    start : TNode,
    is_end: impl Fn(&TNode) -> bool,
    neighbors : impl Fn(&TNode) -> Vec<TNode>,
    cost : impl Fn(u64, &TNode) -> u64,
    heuristic : impl Fn(&TNode) -> u64,
    mut explore_node_callback : impl AsyncFnMut(&TNode),
) -> Option<TNode> where TNode: Clone + Eq + Hash + Debug {
    use std::collections::{BinaryHeap, HashSet};

    #[derive(Eq, PartialEq, Debug, Clone)]
    struct Node<TNode> {
        cost: u64,
        heuristic: u64,
        state: TNode,
    }

    impl<TNode> Ord for Node<TNode> where TNode: Clone + Eq + Hash {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            (self.cost + self.heuristic).cmp(&(other.cost + other.heuristic)).reverse()
        }
    }

    impl<TNode> PartialOrd for Node<TNode> where TNode: Clone + Eq + Hash {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut open_set = BinaryHeap::new();
    let mut closed_set = HashSet::new();

    open_set.push(Node {
        cost: 0,
        heuristic: heuristic(&start),
        state: start.clone(),
    });

    while let Some(current_node) = open_set.pop() {
        explore_node_callback(&current_node.state).await;

        if is_end(&current_node.state) {
            return Some(current_node.state);
        }

        closed_set.insert(current_node.state.clone());

        for neighbor in neighbors(&current_node.state) {
            if closed_set.contains(&neighbor) {
                continue;
            }

            let new_cost = cost(current_node.cost, &neighbor);
            let new_heuristic = heuristic(&neighbor);

            open_set.push(Node {
                cost: new_cost,
                heuristic: new_heuristic,
                state: neighbor,
            });
        }
    }

    None
}