(module
  (func $dive (call $dive))
  (func (export "tick") (call $dive)))
