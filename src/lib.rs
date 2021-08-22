//! # What's this?
//!
//! A **resizeable array** based on a **segmented array structure**.
//!
//! ## When should I use it?
//!
//! You may want to use a `SegVec` instead of a `Vec` if:
//! - **...you have a `Vec` that grows a lot** over the lifetime of your program
//! - ...**you want to shrink a `Vec`**, but don't want to pay the overhead of
//!   copying every element in the `Vec`.
//! - ...you want elements in the `Vec` to have **stable memory locations**
//!
//! You should *not* use `SegVec` if:
//! - **...the `Vec` will never be resized**. In order to provide optimal resizing
//!   performance, `SegVec` introduces some constant-factor overhead when
//!   indexing the vector. This overhead is fairly small, but if your vector
//!   never changes its size after it's allocated, it's not worth paying that
//!   cost for no reason.
//! - **...you want to slice the `Vec`**. Because a `SegVec` is _segmented_,
//!   storing chunks of data at different non-contiguous memory locations, you
//!   cannot slice a contiguous region of the vector. It is possible to
//!   _iterate_ over ranges of a `SegVec`, but you cannot obtain a slice of data
//!   in a `SegVec`. If you need to slice your vector, you can't use this.
use std::{
    fmt,
    ops::{Index, IndexMut},
};

#[cfg(test)]
macro_rules! test_dbg {
    (let $name:ident = $x:expr;) => {
        let $name = $x;
        eprintln!(
            "[{}:{}] let {} = {};\t// {}",
            file!(),
            line!(),
            stringify!($name),
            $name,
            stringify!($x)
        );
    };
    (let mut $name:ident = $x:expr;) => {
        let mut $name = $x;
        eprintln!(
            "[{}:{}] let mut {} = {};\t// {}",
            file!(),
            line!(),
            stringify!($name),
            $name,
            stringify!($x)
        );
    };
    ($x:expr) => {
        dbg!($x)
    };
}

#[cfg(not(test))]
macro_rules! test_dbg {
    (let $name:ident = $x:expr;) => {
        let $name = $x;
    };
    (let mut $name:ident = $x:expr;) => {
        let mut $name = $x;
    };
    ($x:expr) => {
        $x
    };
}

#[derive(Debug)]
pub struct SegVec<T> {
    /// The total number of elements in this `SegVec`.
    ///
    /// This is denoted by _n_ in the paper.
    len: usize,

    /// Current superblock index.
    superblock: usize,

    /// The capacity of the current superblock.
    ///
    /// When the superblock has `sb_cap` blocks in it, allocating a new block
    /// will increment the superblock index and reset `sb_cap`.
    sb_cap: usize,

    /// The current number of blocks in the current superblock.
    sb_len: usize,

    /// The capacity of the blocks in the current superblock.
    block_cap: usize,

    /// The "index block". This holds pointers to the allocated data blocks.
    index: Vec<Block<T>>,
}

struct Block<T> {
    elements: Vec<T>,
}

impl<T> SegVec<T> {
    pub fn new() -> Self {
        // XXX(eliza): blah
        Self::with_capacity(1)
    }

    fn with_capacity(capacity: usize) -> Self {
        // XXX(eliza): this doesn't actually work for capacities other than 1...
        Self {
            len: 0,
            superblock: 0,
            sb_cap: 1,
            sb_len: 1,
            block_cap: capacity,
            index: vec![Block::new(capacity)],
        }
    }

    // this code was implemented from a computer science paper lol
    #[allow(clippy::many_single_char_names)]
    fn locate(&self, i: usize) -> (usize, usize) {
        const BITS2: usize = (usize::BITS - 1) as usize;
        // TODO(eliza): it is almost certainly possible to optimize this using
        // the `log2` of the current block size...

        // 1. Let `r` denote the binary representation of `i + 1`, with all
        //    leading zeroes removed.
        test_dbg!(let r = i + 1;);
        // 2. Note that the desired element `i` is element `e` of data block `b`
        //    of superblock `k`, where:
        //  (a). `k = |r| - 1`
        test_dbg!(let k = BITS2.saturating_sub(r.leading_zeros() as usize););
        //   (c). `e` is the last `ceil(k/2)` bits of `r`.
        test_dbg!(let e_bits = (k + 1) >> 1;);
        test_dbg!(let e = r & !((-1isize as usize) << e_bits););
        test_dbg!(let r = r >> e_bits;);
        //  (b). `b` is the last `floor(k/2)` bits of `r` immediately after the
        //       leading 1-bit
        test_dbg!(let b_bits = k >> 1;);
        test_dbg!(let b = r & !((-1isize as usize) << b_bits););
        // 3. let `p = 2^k - 1` be the number of datablocks in superblocks prior to
        //   `SB[k]`.
        test_dbg!(let p = (1 << e_bits) + (1 << b_bits) - 2;);

        debug_assert!(
            p + b < self.index.len(),
            "assertion failed: p + b < self.index.len(); p={}; b={}; self.index.len()={}",
            p,
            b,
            self.index.len()
        );
        (test_dbg!(p + b), test_dbg!(e))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        if idx > self.len {
            return None;
        }

        let (block, idx) = self.locate(idx);
        self.index.get(block)?.elements.get(idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        if idx > self.len {
            return None;
        }

        let (block, idx) = self.locate(idx);
        self.index.get_mut(block)?.elements.get_mut(idx)
    }

    pub fn push(&mut self, element: T) -> usize {
        if self.index.is_empty() {
            todo!("allocate first block");
        }

        let curr_block_idx = self.index.len() - 1;
        let mut curr_block = &mut self.index[curr_block_idx];
        if curr_block.is_full() {
            self.grow();
            curr_block = &mut self.index[curr_block_idx + 1];
        }
        curr_block.push(element);
        let len = self.len;
        self.len += 1;
        len
    }

    /// Grow:
    /// 1. If the last non-empty data block `DB[d-1]` is full:
    fn grow(&mut self) {
        // (a). if the last superblock `SB[s-1]` is full:
        if self.sb_cap == self.sb_len {
            // i. increment `s`
            self.superblock += 1;
            self.sb_len = 0;
            // ii. if `s` is odd, double the number of data block sin a superblock
            if self.superblock % 2 == 0 {
                self.sb_cap *= 2;
            // iii. otherwise, double the number of elements in a data block.
            } else {
                self.block_cap *= 2;
            }
        }

        // (b). if there are no empty data blocks:
        self.index.push(Block::new(self.block_cap));
        self.sb_len += 1;
    }
}

impl<T> Index<usize> for SegVec<T> {
    type Output = T;

    #[track_caller]
    fn index(&self, idx: usize) -> &Self::Output {
        match self.get(idx) {
            None => panic!(
                "SegVec index out of bounds: the len is {} but the index is {}",
                self.len(),
                idx
            ),
            Some(elem) => elem,
        }
    }
}

impl<T> IndexMut<usize> for SegVec<T> {
    #[track_caller]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        let len = self.len;
        match self.get_mut(idx) {
            None => panic!(
                "SegVec index out of bounds: the len is {} but the index is {}",
                len, idx
            ),
            Some(elem) => elem,
        }
    }
}

// === impl Block ==

impl<T> Block<T> {
    fn new(capacity: usize) -> Self {
        Self {
            elements: Vec::with_capacity(capacity),
            // capacity,
        }
    }

    fn is_full(&self) -> bool {
        self.elements.capacity() == self.elements.len()
    }

    fn push(&mut self, element: T) {
        debug_assert!(!self.is_full(), "Block vectors should never reallocate");
        self.elements.push(element);
    }
}

impl<T: fmt::Debug> fmt::Debug for Block<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("len", &self.elements.len())
            .field("capacity", &self.elements.capacity())
            .field("elements", &self.elements)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert_eq, proptest};

    #[test]
    fn push_one_element() {
        let mut segvec = SegVec::with_capacity(1);
        dbg!(&mut segvec).push(1);
        assert_eq!(dbg!(segvec)[0], 1);
    }

    proptest! {
        #[test]
        fn indexing_basically_works(vec: Vec<usize>) {
            if vec.is_empty() {
                return Ok(());
            }

            let mut segvec = SegVec::with_capacity(1);
            for (i, elem) in vec.iter().enumerate() {
                let pushed_idx = segvec.push(elem);
                prop_assert_eq!(pushed_idx, i, "   vec={:?}\nsegvec={:#?}", vec, segvec);
            }

            for (i, elem) in vec.iter().enumerate() {
                println!("vec[{}] = {}", i, elem);
                prop_assert_eq!(segvec[i], elem, "i={}\n   vec={:?}\nsegvec={:#?}", i, vec, segvec)
            }
        }
    }
}
