use segvec::SegVec;

fn main() {
    let mut v = SegVec::new();
    for i in 0..200 {
        v.push(i);
    }
    println!("{:#?}", v.debug_details());
}
