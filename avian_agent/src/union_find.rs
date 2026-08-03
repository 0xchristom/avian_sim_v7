//! Sprint 2 (Audit 5, B28): a shared disjoint-set (union-find) with
//! union-by-size and *iterative* path halving, so an adversarial union order
//! can never blow the call stack (no recursion). Reused by metrics flock
//! detection and any future spatial clustering.

/// Disjoint-set with `u32` element indices (capable of ~4G agents).
///
/// `find` performs path halving while climbing; `union` attaches the smaller
/// tree under the larger so the maximum tree height stays `O(log N)`. Both are
/// non-recursive, keeping the worst-case stack depth at `O(1)` regardless of
/// union order (B28).
#[derive(Clone, Debug)]
pub struct UnionFind {
    parent: Vec<u32>,
    size: Vec<u32>,
}

impl UnionFind {
    pub fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n as u32).collect(),
            size: vec![1; n],
        }
    }

    /// Root of `x` with path halving (jumps every other node straight to its
    /// grandparent on the way up).
    pub fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            self.parent[x as usize] = self.parent[self.parent[x as usize] as usize];
            x = self.parent[x as usize];
        }
        x
    }

    /// Union the sets containing `a` and `b` (no-op if already joined).
    pub fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        let (big, small) = if self.size[ra as usize] >= self.size[rb as usize] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent[small as usize] = big;
        self.size[big as usize] += self.size[small as usize];
    }

    /// Size of the component containing `x`.
    pub fn component_size(&mut self, x: u32) -> u32 {
        let r = self.find(x);
        self.size[r as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_joins_and_sizes_components() {
        let mut dsu = UnionFind::new(5);
        dsu.union(0, 1);
        dsu.union(2, 3);
        assert_eq!(dsu.find(0), dsu.find(1));
        assert_eq!(dsu.find(2), dsu.find(3));
        assert_ne!(dsu.find(0), dsu.find(2));
        assert_eq!(dsu.component_size(0), 2);
        assert_eq!(dsu.component_size(4), 1);
        dsu.union(1, 2);
        assert_eq!(dsu.find(0), dsu.find(3));
        assert_eq!(dsu.component_size(3), 4);
    }

    /// B28: a deep adversarial chain of unions (0-1, 0-2, 0-3, …) must stay
    /// shallow — union-by-size keeps the tree flat, and find is iterative, so
    /// no recursion and no stack overflow even for a huge chain.
    #[test]
    fn handles_adversarial_chain_without_recursion() {
        let n = 4096;
        let mut dsu = UnionFind::new(n);
        for i in 1..n {
            dsu.union(0, i as u32);
        }
        assert_eq!(dsu.component_size(0), n as u32);
        // Height of any leaf root path is tiny (union-by-size ⇒ ≤ log2 n).
        let mut x = 0u32;
        let mut height = 0;
        while dsu.parent[x as usize] != x {
            x = dsu.parent[x as usize];
            height += 1;
        }
        assert!(height <= (n as f64).log2().ceil() as usize);
    }
}
