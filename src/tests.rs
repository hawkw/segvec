use super::*;
use proptest::{prop_assert, prop_assert_eq, proptest};

// Don't try to allocate stupidly big segvecs in proptests. 256kb seems fine.
const A_REASONABLE_CAPACITY: usize = (256 * 1024) / mem::size_of::<usize>();

#[test]
fn push_one_element() {
    let mut segvec = SegVec::new();
    dbg!(&mut segvec).push(1);
    assert_eq!(dbg!(segvec)[0], 1);
}

#[test]
fn push_one_element_with_capacity() {
    let mut segvec = SegVec::with_capacity(64);
    dbg!(&mut segvec).push(1);
    assert_eq!(dbg!(segvec)[0], 1);
}

proptest! {
    #[test]
    fn indexing_basically_works(vec: Vec<usize>) {
        if vec.is_empty() {
            return Ok(());
        }

        let mut segvec = SegVec::new();
        for (i, elem) in vec.iter().enumerate() {
            let pushed_idx = segvec.push(elem);
            prop_assert_eq!(pushed_idx, i, "   vec={:?}\nsegvec={:#?}", vec, segvec.debug_details());
        }

        test_dbg!(segvec.debug_details());

        for (i, elem) in vec.iter().enumerate() {
            println!("vec[{}] = {}", i, elem);
            prop_assert_eq!(segvec[i], elem, "i={}\n   vec={:?}\nsegvec={:#?}", i, vec, segvec.debug_details())
        }
    }

    #[test]
    fn indexing_basically_works_with_capacity(capacity in 0usize..1024, vec: Vec<usize>) {
        if vec.is_empty() {
            return Ok(());
        }

        let mut segvec = SegVec::with_capacity(capacity);
        for (i, elem) in vec.iter().enumerate() {
            let pushed_idx = segvec.push(elem);
            prop_assert_eq!(pushed_idx, i, "   vec={:?}\nsegvec={:#?}", vec, segvec.debug_details());
        }

        test_dbg!(segvec.debug_details());

        for (i, elem) in vec.iter().enumerate() {
            println!("vec[{}] = {}", i, elem);
            prop_assert_eq!(segvec[i], elem, "i={}\n   vec={:?}\nsegvec={:#?}", i, vec, segvec.debug_details())
        }
    }

    #[test]
    fn iter_roundtrip(vec: Vec<usize>) {
        let segvec: SegVec<usize> = vec.iter().cloned().collect();
        let vec2: Vec<usize> = segvec.iter().cloned().collect();
        prop_assert_eq!(&vec, &vec2, "vec={:?}, vec2={:?}, segvec={:#?}", vec, vec2, segvec.debug_details());
    }

    #[test]
    fn extend(vec1: Vec<usize>, vec2: Vec<usize>) {
        let mut segvec: SegVec<usize> = SegVec::new();
        test_dbg!(segvec.debug_details());

        // Extend the segvec with elements from vec1.
        segvec.extend(vec1.iter().copied());
        test_dbg!(segvec.debug_details());

        for (i, elem) in vec1.iter().enumerate() {
            println!("vec1[{}] = {}", i, elem);
            prop_assert_eq!(&segvec[i], elem);
        }

        // Extend the segvec with elements from vec2.
        segvec.extend(vec2.iter().copied());
        test_dbg!(segvec.debug_details());

        for (i, elem) in vec1.iter().chain(vec2.iter()).enumerate() {
            println!("vecs[{}] = {}", i, elem);
            prop_assert_eq!(&segvec[i], elem);
        }
    }

    #[test]
    fn reserve(cap in 0..A_REASONABLE_CAPACITY) {
        let mut segvec: SegVec<usize> = SegVec::new();
        segvec.reserve(cap);
        prop_assert!(
            segvec.capacity() >= cap,
            "segvec.capacity() >= cap; cap={}; actual={}; segvec={:#?};",
            cap, segvec.capacity(), segvec.debug_details(),
        );
    }


    #[test]
    fn reserve_with_elements(elems: Vec<usize>, cap in 0..A_REASONABLE_CAPACITY) {
        let mut segvec: SegVec<usize> = elems.iter().cloned().collect();
        let len = segvec.len();
        segvec.reserve(cap);
        prop_assert!(
            segvec.capacity() >= (cap + len),
            "segvec.capacity() >= (cap + len); cap={}; len={}; total={}; \
            actual={}; segvec={:#?};",
            cap, len, cap + len, segvec.capacity(), segvec.debug_details(),
        );
    }

    #[test]
    fn reserve_with_capacity(
        initial_cap in 0..A_REASONABLE_CAPACITY,
        cap in 0..A_REASONABLE_CAPACITY,
    ) {
        let mut segvec: SegVec<usize> = SegVec::with_capacity(initial_cap);
        segvec.reserve(cap);
        prop_assert!(
            segvec.capacity() >= initial_cap,
            "segvec.capacity() >= initial_cap; initial_cap={}; actual={}; \
            segvec={:#?};",
            initial_cap, segvec.capacity(), segvec.debug_details(),
        );
        prop_assert!(
            segvec.capacity() >= cap,
            "segvec.capacity() >= cap; cap={}; initial_cap={}; actual={}; \
            segvec={:#?};",
            cap, initial_cap, segvec.capacity(), segvec.debug_details(),
        );
    }

    #[test]
    fn reserve_twice(
        cap1 in 0..A_REASONABLE_CAPACITY,
        cap2 in 0..A_REASONABLE_CAPACITY,
    ) {
        let mut segvec: SegVec<usize> = SegVec::new();
        segvec.reserve(cap1);
        prop_assert!(
            segvec.capacity() >= cap1,
            "segvec.capacity() >= cap1; cap1={}; actual={}; segvec={:#?};",
            cap1, segvec.capacity(), segvec.debug_details(),
        );

        segvec.reserve(cap2);
        prop_assert!(
            segvec.capacity() >= cap2,
            "segvec.capacity() >= cap2; cap2={}; actual={}; segvec={:#?};",
            cap2, segvec.capacity(), segvec.debug_details(),
        );
    }
}
