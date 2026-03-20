// crates/weaveback-macro/src/evaluator/tests/test_rhaidef.rs

use crate::evaluator::{EvalConfig, Evaluator, eval_string};

fn evaluator() -> Evaluator {
    Evaluator::new(EvalConfig::default())
}

#[test]
fn test_basic_arithmetic() {
    // Body must be wrapped in %{ %} so weaveback doesn't parse parentheses as arg lists
    let mut ev = evaluator();
    let src = r#"%rhaidef(double, x, %{(parse_int(x) * 2).to_string()%})
%double(21)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "42");
}

#[test]
fn test_helper_function_in_body() {
    let mut ev = evaluator();
    let src = r#"%rhaidef(factorial, n, %{
fn fact(k) { if k <= 1 { 1 } else { k * fact(k - 1) } }
fact(parse_int(n)).to_string()
%})
%factorial(5)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "120");
}

#[test]
fn test_variable_capture_from_scope() {
    // Outer weaveback scope variables are injected into Rhai scope
    let mut ev = evaluator();
    let src = r#"%set(greeting, Hello)
%rhaidef(greet, name, %{
let g = greeting;
let n = name;
g + ", " + n + "!"
%})
%greet(Rhai)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "Hello, Rhai!");
}

#[test]
fn test_hex_formatting() {
    let mut ev = evaluator();
    let src = r#"%rhaidef(as_hex, n, %{to_hex(parse_int(n))%})
%as_hex(255)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "0xFF");
}

#[test]
fn test_error_propagation() {
    let mut ev = evaluator();
    // @@@ is not valid Rhai syntax
    let src = r#"%rhaidef(broken, x, %{@@@%})
%broken(foo)"#;
    let result = eval_string(src, None, &mut ev);
    assert!(result.is_err(), "expected error from bad Rhai code");
}

// --- store tests ---

#[test]
fn test_rhaiset_rhaiget_roundtrip() {
    let mut ev = evaluator();
    let src = r#"%rhaiset(color, red)
%rhaiget(color)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "red");
}

#[test]
fn test_store_counter_auto_writeback() {
    // The script mutates `counter` directly; the store write-back persists it.
    let mut ev = evaluator();
    let src = r#"%rhaiset(counter, 0)
%rhaidef(tick, %{
  counter += 1;
  counter.to_string()
%})
%tick()
%tick()
%tick()"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    // Each call emits the new counter value; trim whitespace between them
    let values: Vec<&str> = result.split_whitespace().collect();
    assert_eq!(values, ["1", "2", "3"]);
}

#[test]
fn test_store_integer_type_preserved() {
    // %rhaiset auto-parses numeric strings, so arithmetic works natively
    let mut ev = evaluator();
    let src = r#"%rhaiset(n, 10)
%rhaidef(double_n, %{
  n *= 2;
  n.to_string()
%})
%double_n()
%double_n()"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    let values: Vec<&str> = result.split_whitespace().collect();
    // First call: n=10 → *2 = 20; second call: n=20 → *2 = 40
    assert_eq!(values, ["20", "40"]);
}

#[test]
fn test_store_map_tree() {
    // Build a tree as nested Rhai maps, then query it across calls.
    // %rhaiexpr initialises the store key with a typed Rhai value.
    let mut ev = evaluator();
    let src = r#"%rhaiexpr(root, #{})
%rhaidef(build_tree, %{
  root = #{
    name: "root",
    children: [
      #{ name: "a", children: [] },
      #{ name: "b", children: [] }
    ]
  };
  ""
%})
%rhaidef(child_count, %{
  root.children.len().to_string()
%})
%build_tree()
%child_count()"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "2");
}

#[test]
fn test_store_array_accumulation() {
    // Accumulate items into a Rhai array across calls.
    // %rhaiexpr initialises `items` as a typed Rhai array, not a string.
    let mut ev = evaluator();
    let src = r#"%rhaiexpr(items, [])
%rhaidef(push_item, x, %{
  items.push(x);
  items.len().to_string()
%})
%push_item(apple)
%push_item(banana)
%push_item(cherry)"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    let counts: Vec<&str> = result.split_whitespace().collect();
    assert_eq!(counts, ["1", "2", "3"]);
}

#[test]
fn test_store_shadowed_by_weaveback_scope() {
    // An weaveback %set variable does NOT override a same-named store key —
    // store takes priority so scripts see the persistent value.
    let mut ev = evaluator();
    let src = r#"%rhaiset(x, store_val)
%set(x, weaveback_val)
%rhaidef(get_x, %{x%})
%get_x()"#;
    let result = eval_string(src, None, &mut ev).expect("eval failed");
    assert_eq!(result.trim(), "store_val");
}
