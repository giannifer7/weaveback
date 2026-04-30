// weaveback-macro/src/evaluator/core/py_store.rs
// I'd Really Rather You Didn't edit this generated file.

impl Evaluator {
    pub fn pystore_set(&mut self, key: String, value: String) {
        self.py_store.insert(key, value);
    }

    pub fn pystore_get(&self, key: &str) -> String {
        self.py_store.get(key).cloned().unwrap_or_default()
    }
}


