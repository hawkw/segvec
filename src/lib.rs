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
    cmp, fmt,
    iter::FromIterator,
    mem,
    ops::{Index, IndexMut},
    slice,
};

#[cfg(test)]
macro_rules! test_dbg {
    (let $name:ident = $x:expr;) => {
        let $name = $x;
        let name = stringify!($name);
        eprintln!(
            "[{}:{}] let {} = {:<width$} // {}",
            file!(),
            line!(),
            name,
            $name,
            stringify!($x),
            width = 20 - name.len(),
        );
    };
    (let mut $name:ident = $x:expr;) => {
        let mut $name = $x;
        eprintln!(
            "[{}:{}] let mut {} = {:<width$} // {}",
            file!(),
            line!(),
            name,
            $name,
            stringify!($x),
            width = 16 - name.len(),
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

#[cfg(test)]
mod tests;

pub struct SegVec<T> {
    meta: Meta,

    /// The total capacity of the `SegVec`. This _includes_ used capacity.
    ///
    /// This should always be >= `self.len()`.
    capacity: usize,

    /// The "index block". This holds pointers to the allocated data blocks.
    index: Vec<Block<T>>,

    #[cfg(debug_assertions)]
    is_initialized: bool,
}

#[derive(Debug)]
pub struct Iter<'segvec, T> {
    len: usize,
    blocks: slice::Iter<'segvec, Block<T>>,
    curr_block: slice::Iter<'segvec, T>,
}

#[derive(Debug)]
pub struct IterMut<'segvec, T> {
    len: usize,
    blocks: slice::IterMut<'segvec, Block<T>>,
    curr_block: slice::IterMut<'segvec, T>,
}

#[derive(Debug)]
struct Meta {
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

    skipped_blocks: usize,
    skipped_indices: usize,

    /// The current empty data block to push in.
    empty_data_block: usize,
}

struct Block<T> {
    elements: Vec<T>,
}

/// TODO(eliza): consider making this an API?
#[cfg(test)]
struct DebugDetails<'segvec, T>(&'segvec SegVec<T>);

impl<T> SegVec<T> {
    pub const fn new() -> Self {
        Self {
            meta: Meta::empty(),
            index: Vec::new(),
            capacity: 0,
            #[cfg(debug_assertions)]
            is_initialized: false,
        }
    }

    // Minimum size of the first data block `Vec`.
    // This is what `std` will allocate initially if a `Vec` is constructed
    // without using `with_capacity`.
    // Copied from https://github.com/rust-lang/rust/blob/996ff2e0a0f911f52bb1de6bdd0cfd5704de1fc9/library/alloc/src/raw_vec.rs#L117-L128
    const MIN_NON_ZERO_CAP: usize = if mem::size_of::<T>() == 1 {
        8
    } else if mem::size_of::<T>() <= 1024 {
        4
    } else {
        1
    };

    pub fn with_capacity(capacity: usize) -> Self {
        let mut this = Self::new();
        this.reserve(capacity);
        this
    }

    /// Returns the number of elements the `SegVec` can hold without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use segvec::SegVec;
    ///
    /// let sv: SegVec<i32> = SegVec::with_capacity(10);
    /// assert!(sv.capacity() >= 10);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of elements in the `SegVec`, also referred to
    /// as its 'length'.
    ///
    /// # Examples
    ///
    // TODO(eliza): fix this example.
    /// ```ignore
    /// use segvec::segvec;
    ///
    /// let sv = segvec![1, 2, 3];
    /// assert_eq!(a.len(), 3);
    /// ```
    #[doc(alias = "length")]
    #[inline]
    pub fn len(&self) -> usize {
        self.meta.len
    }

    /// Reserves capacity for at least `additional` more elements to be inserted
    /// in the given `SegVec<T>`. The collection may reserve more space to avoid
    /// frequent reallocations. After calling `reserve`, capacity will be
    /// greater than or equal to `self.len() + additional`. Does nothing if
    /// capacity is already sufficient.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity exceeds `isize::MAX` bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use segvec::SegVec;
    ///
    /// let mut sv: SegVec<i32> = SegVec::with_capacity(1);
    /// sv.reserve(10);
    /// assert!(sv.capacity() >= 11);
    /// ```
    pub fn reserve(&mut self, mut additional: usize) {
        assert!(additional < isize::MAX as usize);

        if additional == 0 {
            return;
        }

        if self.capacity == 0 {
            // If the requested capacity is not a power of two, round up to the next
            // power of two.
            if test_dbg!(!additional.is_power_of_two()) {
                additional = test_dbg!(additional.next_power_of_two());
            };

            // If the capacity is less than the reasonable minimum capacity for the
            // size of elements in the `SegVec`, use that capacity instead.
            test_dbg!(let additional = cmp::max(additional, Self::MIN_NON_ZERO_CAP););
            self.initialize(additional);
            return;
        }

        #[cfg(debug_assertions)]
        debug_assert!(self.is_initialized);

        let _ = test_dbg!((self.capacity(), self.len(), additional));

        if test_dbg!(self.capacity() - self.len() >= additional) {
            return;
        }

        while test_dbg!(self.capacity() - self.len() < additional) {
            self.grow();
            let _ = test_dbg!(&self.meta);
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
        test_dbg!(let r = i + 1 + self.meta.skipped_indices;);
        // 2. Note that the desired element `i` is element `e` of data block `b`
        //    of superblock `k`, where:
        //  (a). `k = |r| - 1`
        test_dbg!(let k = BITS2.saturating_sub(r.leading_zeros() as usize););
        //   (c). `e` is the last `ceil(k/2)` bits of `r`.
        test_dbg!(let e_bits = (k + 1) >> 1;);
        test_dbg!(let e = r & !(usize::MAX << e_bits););
        test_dbg!(let r = r >> e_bits;);
        //  (b). `b` is the last `floor(k/2)` bits of `r` immediately after the
        //       leading 1-bit
        test_dbg!(let b_bits = k >> 1;);
        test_dbg!(let b = r & !(usize::MAX << b_bits););
        // 3. let `p = 2^k - 1` be the number of datablocks in superblocks prior to
        //   `SB[k]`.
        test_dbg!(let p = (1 << e_bits) + (1 << b_bits) - 2;);

        // 4. Return the location of element `e` in data block `DB[p + b]`.
        // NOTE: also compensate for skipped low-size blocks.
        test_dbg!(let data_block = p + b - self.meta.skipped_blocks;);

        // If the data block index is out of bounds, panic with a nicer
        // assertion with more debugging information.
        debug_assert!(
            data_block < self.index.len(),
            "assertion failed: data_block < self.index.len(); \
            data_block={}; self.index.len()={}; p={}; b={}; \
            metadata={:#?}",
            data_block,
            self.index.len(),
            p,
            b,
            self.meta,
        );

        (data_block, test_dbg!(e))
    }

    pub fn is_empty(&self) -> bool {
        self.meta.len == 0
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        if idx > self.meta.len {
            return None;
        }

        let (block, idx) = self.locate(idx);
        self.index.get(block)?.elements.get(idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        if idx > self.meta.len {
            return None;
        }

        let (block, idx) = self.locate(idx);
        self.index.get_mut(block)?.elements.get_mut(idx)
    }

    pub fn push(&mut self, element: T) -> usize {
        if self.capacity == 0 {
            self.initialize(Self::MIN_NON_ZERO_CAP);
        } else {
            #[cfg(debug_assertions)]
            debug_assert!(self.is_initialized);
        };

        let mut curr_block = &mut self.index[self.meta.empty_data_block];

        // Is the current block to push in full?
        if curr_block.is_full() {
            // Is the current block the last one (e.g. are we out of blocks)?
            // If so, allocate a new block. Otherwise, we have additional free
            // blocks to fill (due to `reserve`/`with_capacity`) calls.
            // NOTE: the Brodnik et al paper doesn't consider that you might
            //       want to reserve capacity, so this is one of our deviations
            //       from their algorithm.
            if self.meta.empty_data_block == self.index.len() - 1 {
                self.grow();
            }

            self.meta.empty_data_block += 1;
            curr_block = &mut self.index[self.meta.empty_data_block];
        }

        curr_block.push(element);

        let len = self.meta.len;
        self.meta.len += 1;
        len
    }

    pub fn iter(&self) -> Iter<'_, T> {
        let mut blocks = self.index.iter();
        let curr_block = blocks
            .next()
            .map(|block| block.elements.iter())
            .unwrap_or_else(|| [].iter());
        Iter {
            len: self.len(),
            blocks,
            curr_block,
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let len = self.len();
        let mut blocks = self.index.iter_mut();
        let curr_block = blocks
            .next()
            .map(|block| block.elements.iter_mut())
            .unwrap_or_else(|| [].iter_mut());
        IterMut {
            len,
            blocks,
            curr_block,
        }
    }

    fn grow(&mut self) {
        if self.capacity == 0 {
            self.initialize(Self::MIN_NON_ZERO_CAP);
        } else {
            #[cfg(debug_assertions)]
            debug_assert!(self.is_initialized);
        };

        self.meta.grow();
        self.index.push(Block::new(self.meta.block_cap));
        self.capacity += self.meta.block_cap;
    }

    fn initialize(&mut self, capacity: usize) {
        #[cfg(debug_assertions)]
        debug_assert!(!self.is_initialized);
        debug_assert!(capacity.is_power_of_two());
        debug_assert!(capacity >= Self::MIN_NON_ZERO_CAP);

        // Grow the metadata up to the requested capacity.
        while test_dbg!(self.meta.block_cap) < capacity {
            self.meta.grow();
            self.meta.skipped_blocks += 1;
            self.meta.skipped_indices += self.meta.block_cap;
            let _ = test_dbg!(&self.meta);
        }

        // Build the index, in a vector with enough room for at least the number
        // of skipped data blocks plus the first actual data block.
        self.index.reserve(self.meta.skipped_blocks);

        // Grow the metadata again and push the first actual data block.
        self.meta.grow();
        self.index.push(Block::new(capacity));
        debug_assert_eq!(self.meta.block_cap, capacity);

        let _ = test_dbg!(&self.meta);

        #[cfg(debug_assertions)]
        {
            self.is_initialized = true;
        }
        self.capacity = capacity;
    }

    // TODO(eliza): consider making this an API?
    #[cfg(test)]
    fn debug_details(&self) -> DebugDetails<'_, T> {
        DebugDetails(self)
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
        let len = self.meta.len;
        match self.get_mut(idx) {
            None => panic!(
                "SegVec index out of bounds: the len is {} but the index is {}",
                len, idx
            ),
            Some(elem) => elem,
        }
    }
}

impl<T> Extend<T> for SegVec<T> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        let iter = iter.into_iter();

        // If the iterator provides a size hint, try to reserve enough capacity
        // to hold its elements before pushing.
        let cap = size_hint_capacity(&iter);
        self.reserve(cap);

        for item in iter {
            self.push(item);
        }
    }

    // TODO(eliza): add `extend_reserve` once that's stable!
}

impl<T> FromIterator<T> for SegVec<T> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let iter = iter.into_iter();

        // If the iterator provides us with a size hint, try to preallocate the
        // segvec with enough capacity for the iterator.
        let cap = size_hint_capacity(&iter);
        // TODO(eliza): we could just use `Vec::collect` and push that as block 1...
        let mut this = Self::with_capacity(cap);

        for item in iter.into_iter() {
            this.push(item);
        }

        this
    }
}

impl<'segvec, T> IntoIterator for &'segvec SegVec<T> {
    type IntoIter = Iter<'segvec, T>;
    type Item = &'segvec T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'segvec, T> IntoIterator for &'segvec mut SegVec<T> {
    type IntoIter = IterMut<'segvec, T>;
    type Item = &'segvec mut T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: fmt::Debug> fmt::Debug for SegVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Default for SegVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

// === impl Iter ===

impl<'segvec, T> Iterator for Iter<'segvec, T> {
    type Item = &'segvec T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(elem) = self.curr_block.next() {
                return Some(elem);
            }
            self.curr_block = self.blocks.next()?.elements.iter();
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.len
    }
}

// === impl IterMut ===

impl<'segvec, T> Iterator for IterMut<'segvec, T> {
    type Item = &'segvec mut T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(elem) = self.curr_block.next() {
                return Some(elem);
            }
            self.curr_block = self.blocks.next()?.elements.iter_mut();
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> ExactSizeIterator for IterMut<'_, T> {
    fn len(&self) -> usize {
        self.len
    }
}

// === impl Meta ===

impl Meta {
    /// Returns new metadata describing an empty `SegVec`.
    const fn empty() -> Self {
        Self {
            len: 0,
            superblock: 0,
            sb_cap: 1,
            sb_len: 0,
            block_cap: 1,
            skipped_blocks: 0,
            skipped_indices: 0,
            empty_data_block: 0,
        }
    }

    /// Grow the `SegVec` described by this `Meta`.
    ///
    /// This does *not* allocate a new data block. Instead, it increments the
    /// variables tracking the numbers of blocks and superblocks, and increments
    /// the block size or superblock size, as needed.
    ///
    /// This should be called prior to allocating a new data block.
    fn grow(&mut self) {
        // 1. If the last non-empty data block `DB[d-1]` is full:
        //   (a). if the last superblock `SB[s-1]` is full:
        if self.sb_cap == self.sb_len {
            // i. increment `s`
            self.superblock += 1;
            self.sb_len = 0;
            // ii. if `s` is odd, double the number of data block in a superblock
            if self.superblock % 2 == 0 {
                self.sb_cap *= 2;
            // iii. otherwise, double the number of elements in a data block.
            } else {
                self.block_cap *= 2;
            }
        }

        //   (b). if there are no empty data blocks:
        self.sb_len += 1;
    }
}

// === impl Block ==

impl<T> Block<T> {
    fn new(capacity: usize) -> Self {
        let elements = Vec::with_capacity(capacity);
        debug_assert_eq!(capacity, elements.capacity());
        Self { elements }
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

// === impl DebugDetails ===

#[cfg(test)]
impl<T: fmt::Debug> fmt::Debug for DebugDetails<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("SegVec");
        f.field("meta", &self.0.meta)
            .field("capacity", &self.0.capacity)
            .field("index", &self.0.index);
        #[cfg(debug_assertions)]
        {
            f.field("is_initialized", &self.0.is_initialized);
        }
        f.finish()
    }
}

/// Determine the capacity to preallocate for an iterator, based on its
/// `size_hint`.
///
/// This is used when extending or collecting iterators into a `SegVec`.
#[inline(always)]
fn size_hint_capacity(iter: &impl Iterator) -> usize {
    let (lower, upper) = iter.size_hint();
    // If the size hint has an upper bound, use that as the capacity so we
    // can put all the elements in one block. Otherwise, make the first
    // block the size hint's lower bound.
    upper.unwrap_or(lower)
}
