(module
  (import "world" "spawn" (func $spawn (result i32)))
  (func (export "tick") (drop (call $spawn))))
