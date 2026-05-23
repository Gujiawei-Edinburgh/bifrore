fn main() {
    println!("cargo:rerun-if-changed=src/parser/tenon.lalrpop");
    lalrpop::process_root().expect("generate Tenon DSL parser");
}
