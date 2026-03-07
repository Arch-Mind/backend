use neo4rs::query;
fn main() {
    let q1 = query("RETURN 1");
    let q2 = q1.clone();
}
