//! Navigation on the loaded first-parent forest, independent of display order.

#[derive(Debug)]
pub struct Lineage {
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    stops: Vec<bool>,
    preferred_children: Vec<Option<usize>>,
}

impl Lineage {
    pub fn new(parents: Vec<Option<usize>>, branch_tips: Vec<bool>) -> Self {
        let mut children = vec![Vec::new(); parents.len()];
        // Indices are graph display order, giving siblings a stable ordering.
        for (child, parent) in parents.iter().enumerate() {
            if let Some(parent) = parent {
                children[*parent].push(child);
            }
        }
        let stops = parents
            .iter()
            .enumerate()
            .map(|(index, parent)| {
                branch_tips[index] || parent.is_none() || children[index].len() != 1
            })
            .collect();
        let preferred_children = vec![None; parents.len()];
        Self {
            parents,
            children,
            stops,
            preferred_children,
        }
    }

    /// Walk to the next older landmark and remember the path just descended.
    pub fn toward_root(&mut self, start: usize) -> Option<usize> {
        let mut child = start;
        while let Some(parent) = *self.parents.get(child)? {
            self.preferred_children[parent] = Some(child);
            if self.stops[parent] {
                return Some(parent);
            }
            child = parent;
        }
        None
    }

    fn next_stop(&self, mut child: usize) -> usize {
        while !self.stops[child] {
            child = self.children[child][0];
        }
        child
    }

    /// One reachable landmark per outgoing first-parent path.
    pub fn toward_tips(&self, start: usize) -> Vec<usize> {
        self.children
            .get(start)
            .into_iter()
            .flatten()
            .map(|&child| self.next_stop(child))
            .collect()
    }

    pub fn preferred_tip(&self, start: usize) -> Option<usize> {
        let child = *self.preferred_children.get(start)?;
        child.map(|child| self.next_stop(child))
    }

    pub fn choose_tip(&mut self, start: usize, target: usize) -> bool {
        if !self.toward_tips(start).contains(&target) {
            return false;
        }
        let mut child = target;
        while child != start {
            let parent = self.parents[child].expect("target is a first-parent descendant");
            self.preferred_children[parent] = Some(child);
            child = parent;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Display order: D, C, B, intermediate, A, root; B and C fork from A.
    fn tree() -> Lineage {
        Lineage::new(
            vec![Some(2), Some(4), Some(3), Some(4), Some(5), None],
            vec![true, true, true, false, false, false],
        )
    }

    #[test]
    fn rootward_skips_linear_commits_but_stops_at_tips_forks_and_root() {
        let mut tree = tree();
        assert_eq!(tree.toward_root(0), Some(2));
        assert_eq!(tree.toward_root(2), Some(4));
        assert_eq!(tree.toward_root(4), Some(5));
        assert_eq!(tree.toward_root(5), None);
    }

    #[test]
    fn tipward_stays_on_path_and_orders_forks_by_display_order() {
        let tree = tree();
        assert_eq!(tree.toward_tips(5), vec![4]);
        assert_eq!(tree.toward_tips(4), vec![1, 2]);
        assert_eq!(tree.toward_tips(2), vec![0]);
        assert!(tree.toward_tips(0).is_empty());
        assert_eq!(tree.toward_tips(3), vec![2]);
    }

    #[test]
    fn descending_remembers_path_and_explicit_choice_can_replace_it() {
        let mut tree = tree();
        assert_eq!(tree.preferred_tip(4), None);
        tree.toward_root(2);
        assert_eq!(tree.preferred_tip(4), Some(2));
        assert!(tree.choose_tip(4, 1));
        assert_eq!(tree.preferred_tip(4), Some(1));
        assert!(!tree.choose_tip(4, 0)); // cannot skip B
        assert_eq!(tree.preferred_tip(4), Some(1));
    }

    #[test]
    fn unrelated_roots_and_missing_history_are_not_crossed() {
        let mut forest = Lineage::new(vec![Some(1), None, None], vec![true, false, true]);
        assert_eq!(forest.toward_tips(1), vec![0]);
        assert_eq!(forest.toward_root(0), Some(1));
        assert_eq!(forest.toward_root(1), None);
        assert!(forest.toward_tips(2).is_empty());
    }

    #[test]
    fn empty_forest_is_safe() {
        let mut forest = Lineage::new(vec![], vec![]);
        assert_eq!(forest.toward_root(0), None);
        assert!(forest.toward_tips(0).is_empty());
        assert_eq!(forest.preferred_tip(0), None);
    }
}
